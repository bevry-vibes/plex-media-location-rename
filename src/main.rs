use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Local;
use clap::Parser;
use rusqlite::{params, Connection};

const BLOBS_FILE_NAME: &str = "com.plexapp.plugins.library.blobs.db";
const CONFIRM_WORD: &str = "RENAME";

#[derive(Parser, Debug)]
#[command(name = "plex-media-location-rename", about = "Rewrite moved media paths inside a Plex library database, without losing added dates.")]
struct Args {
	/// One path move, written as OLD=NEW. Repeat the flag for each move.
	#[arg(long = "map", value_name = "OLD=NEW", required = true)]
	maps: Vec<String>,

	/// Path to com.plexapp.plugins.library.db inside your Plex databases folder.
	#[arg(long, value_name = "PATH", required = true)]
	database: PathBuf,

	/// Directory for the backup copies. The default is the database directory.
	#[arg(long, value_name = "PATH")]
	backup_dir: Option<PathBuf>,

	/// Analyse and print, change no data.
	#[arg(long)]
	dry_run: bool,

	/// Skip the confirmation prompt.
	#[arg(long)]
	yes: bool,
}

struct Map {
	old: String,
	new: String,
}

impl Map {
	fn parse(spec: &str) -> Result<Map, String> {
		let Some((old, new)) = spec.split_once('=') else {
			return Err(format!("The map '{spec}' lacks '='. Write it as OLD=NEW."));
		};
		Ok(Map { old: old.to_string(), new: new.to_string() })
	}
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Encoding {
	Plain,
	FileUrl,
}

#[derive(Clone, Copy)]
struct Target {
	table: &'static str,
	column: &'static str,
	encoding: Encoding,
}

const TARGETS: &[Target] = &[
	Target { table: "section_locations", column: "root_path", encoding: Encoding::Plain },
	Target { table: "media_parts", column: "file", encoding: Encoding::Plain },
	Target { table: "media_streams", column: "url", encoding: Encoding::FileUrl },
];

fn main() {
	match run() {
		Ok(()) => {}
		Err(err) => {
			eprintln!("error: {err}");
			std::process::exit(1);
		}
	}
}

fn run() -> Result<(), String> {
	let args = Args::parse();
	let maps = parse_maps(&args.maps)?;
	validate_maps(&maps)?;
	check_disk(&maps)?;
	check_processes()?;
	let conn = open_database(&args.database)?;

	let analysis = analyse(&conn, &maps)?;
	print_analysis(&maps, &analysis);
	if args.dry_run {
		println!("dry run: the data did not change.");
		return Ok(());
	}
	confirm(&args)?;

	let backup_dir = args.backup_dir.unwrap_or_else(|| default_backup_dir(&args.database));
	let backups = backup(&conn, &args.database, &backup_dir)?;

	let changed = update(&conn, &maps)?;
	checkpoint(&conn)?;
	let mut leftover_old = 0;
	for entry in &changed {
		leftover_old += count_anchored(&conn, &entry.target, &entry.old_key).map_err(db_error)?;
	}
	if leftover_old != 0 {
		return Err(format!("{leftover_old} rows still hold an old path. Restore the backups and inspect the database."));
	}
	print_report(&backups, &changed, &conn, &maps)?;
	Ok(())
}

fn parse_maps(specs: &[String]) -> Result<Vec<Map>, String> {
	specs.iter().map(|s| Map::parse(s)).collect()
}

fn validate_maps(maps: &[Map]) -> Result<(), String> {
	for map in maps {
		if map.old == map.new {
			return Err(format!("The map '{}' has the same old and new path.", map.old));
		}
	}
	for (i, a) in maps.iter().enumerate() {
		for b in maps.iter().skip(i + 1) {
			if boundary_starts_with(&a.old, &b.old) || boundary_starts_with(&b.old, &a.old) {
				return Err(format!("The old paths '{}' and '{}' overlap.", a.old, b.old));
			}
			if boundary_starts_with(&a.new, &b.new) || boundary_starts_with(&b.new, &a.new) {
				return Err(format!("The new paths '{}' and '{}' overlap.", a.new, b.new));
			}
			if a.old == b.new || b.old == a.new {
				return Err("One map writes a path that another map reads. Chain the maps in separate runs.".to_string());
			}
		}
	}
	Ok(())
}

fn check_disk(maps: &[Map]) -> Result<(), String> {
	for map in maps {
		if Path::new(&map.old).exists() {
			return Err(format!("The old path '{}' still exists on disk. Move the folder first.", map.old));
		}
		if !Path::new(&map.new).exists() {
			return Err(format!("The new path '{}' does not exist on disk.", map.new));
		}
	}
	Ok(())
}

fn check_processes() -> Result<(), String> {
	let output = Command::new("ps").args(["axo", "comm="]).output().map_err(|e| format!("The process list failed: {e}"))?;
	let list = String::from_utf8_lossy(&output.stdout).into_owned();
	let mut found = Vec::new();
	for name in list.lines() {
		let name = name.trim();
		if name.starts_with("Plex") && !name.starts_with("Plexamp") {
			found.push(name.to_string());
		}
	}
	if !found.is_empty() {
		return Err(format!("A Plex process runs: {}. Stop the Plex server, then run again.", found.join(", ")));
	}
	Ok(())
}

fn open_database(path: &Path) -> Result<Connection, String> {
	if !path.is_file() {
		return Err(format!("The database '{}' does not exist.", path.display()));
	}
	let conn = Connection::open(path).map_err(db_error)?;
	conn.busy_timeout(std::time::Duration::from_millis(2000)).map_err(db_error)?;
	Ok(conn)
}

struct Analysis {
	old_count: i64,
}

fn analyse<'a>(conn: &Connection, maps: &'a [Map]) -> Result<Vec<(&'a Map, Vec<(Target, Analysis)>)>, String> {
	let mut result = Vec::new();
	for map in maps {
		let mut per_target = Vec::new();
		let mut total = 0;
		for target in TARGETS {
			let old_key = encode_key(target.encoding, &map.old);
			let old_count = count_anchored(conn, target, &old_key).map_err(db_error)?;
			total += old_count;
			per_target.push((*target, Analysis { old_count }));
		}
		if total == 0 {
			return Err(format!("The old path '{}' matches no row. Check the OLD path string.", map.old));
		}
		result.push((map, per_target));
	}
	Ok(result)
}

fn print_analysis(maps: &[Map], analysis: &[(&Map, Vec<(Target, Analysis)>)]) {
	println!("plan:");
	for (map, per_target) in analysis {
		println!("  {} -> {}", map.old, map.new);
		for (target, a) in per_target {
			println!("    {}.{}: {} rows", target.table, target.column, a.old_count);
		}
	}
	if maps.len() > 1 {
		println!("{} maps.", maps.len());
	}
}

fn confirm(args: &Args) -> Result<(), String> {
	if args.yes {
		return Ok(());
	}
	println!("The tool writes to the database. Type {CONFIRM_WORD} to continue.");
	let mut answer = String::new();
	io::stdin().lock().read_line(&mut answer).map_err(|e| format!("The input failed: {e}"))?;
	if answer.trim() != CONFIRM_WORD {
		return Err("The confirmation word did not match. The data did not change.".to_string());
	}
	Ok(())
}

fn default_backup_dir(database: &Path) -> PathBuf {
	database.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

fn timestamp() -> String {
	Local::now().format("%Y-%m-%d-%H%M%S").to_string()
}

fn backup(conn: &Connection, database: &Path, backup_dir: &Path) -> Result<Vec<PathBuf>, String> {
	conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).map_err(db_error)?;
	std::fs::create_dir_all(backup_dir).map_err(|e| format!("The backup directory failed: {e}"))?;
	let stamp = timestamp();
	let mut sources = vec![database.to_path_buf()];
	if let Some(dir) = database.parent() {
		let blobs = dir.join(BLOBS_FILE_NAME);
		if blobs.is_file() {
			sources.push(blobs);
		}
	}
	let mut sources_all = Vec::new();
	for source in &sources {
		sources_all.push(source.clone());
		for suffix in ["-wal", "-shm"] {
			let extra = PathBuf::from(format!("{}{suffix}", source.display()));
			if extra.is_file() {
				sources_all.push(extra);
			}
		}
	}
	let mut backups = Vec::new();
	for source in &sources_all {
		let Some(base) = source.file_name() else {
			return Err("The database path has no file name.".to_string());
		};
		let name = format!("{}-{stamp}", base.to_string_lossy());
		let dest = backup_dir.join(name);
		let expected = std::fs::metadata(source).map_err(|e| format!("The read of '{}' failed: {e}", source.display()))?.len();
		let copied = std::fs::copy(source, &dest).map_err(|e| format!("The copy of '{}' failed: {e}", source.display()))?;
		if copied != expected {
			return Err(format!("The copy of '{}' wrote {copied} bytes, expected {expected}.", source.display()));
		}
		backups.push(dest);
	}
	for dest in &backups {
		if !dest.to_string_lossy().ends_with(".db") {
			continue;
		}
		// A quick_check or integrity_check reads the FTS shadow tables, and their tokenizers exist only inside Plex. Open the copy and read its schema instead.
		let tables = Connection::open(dest)
			.map_err(db_error)?
			.query_row("SELECT count(*) FROM sqlite_master", [], |row| row.get::<_, i64>(0))
			.map_err(db_error)?;
		if tables == 0 {
			return Err(format!("The backup '{}' holds no schema.", dest.display()));
		}
	}
	println!("backup:");
	for dest in &backups {
		println!("  {}", dest.display());
	}
	Ok(backups)
}

struct Changed {
	target: Target,
	old_key: String,
	rows: usize,
}

fn update(conn: &Connection, maps: &[Map]) -> Result<Vec<Changed>, String> {
	conn.execute_batch("BEGIN IMMEDIATE").map_err(db_error)?;
	match update_inner(conn, maps) {
		Ok(changed) => {
			conn.execute_batch("COMMIT").map_err(db_error)?;
			Ok(changed)
		}
		Err(err) => {
			let _ = conn.execute_batch("ROLLBACK");
			Err(err)
		}
	}
}

fn update_inner(conn: &Connection, maps: &[Map]) -> Result<Vec<Changed>, String> {
	let mut changed = Vec::new();
	for map in maps {
		for target in TARGETS {
			let old_key = encode_key(target.encoding, &map.old);
			let new_key = encode_key(target.encoding, &map.new);
			let rows = update_anchored(conn, target, &old_key, &new_key).map_err(db_error)?;
			changed.push(Changed { target: *target, old_key, rows });
		}
	}
	Ok(changed)
}

fn checkpoint(conn: &Connection) -> Result<(), String> {
	conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(())).map_err(db_error)?;
	Ok(())
}

fn print_report(backups: &[PathBuf], changed: &[Changed], conn: &Connection, maps: &[Map]) -> Result<(), String> {
	println!("changed:");
	for entry in changed {
		if entry.rows > 0 {
			println!("  {}.{}: {} rows", entry.target.table, entry.target.column, entry.rows);
		}
	}
	let mut old_total = 0;
	let mut new_total = 0;
	for map in maps {
		for target in TARGETS {
			old_total += count_anchored(conn, target, &encode_key(target.encoding, &map.old)).map_err(db_error)?;
			new_total += count_anchored(conn, target, &encode_key(target.encoding, &map.new)).map_err(db_error)?;
		}
	}
	println!("old path rows: {old_total}");
	println!("new path rows: {new_total}");
	println!("note: the bundled engine cannot validate the Plex full text tables, because their tokenizers live inside Plex. The zero old rows, the new row counts, and the one transaction provide the verification.");
	println!("next steps:");
	println!("  1. Start the Plex server.");
	println!("  2. Run 'Scan Library Files' on the affected library.");
	println!("  3. Check that the added dates stayed, then play a track.");
	println!("to undo, stop the server and restore the backups above.");
	let _ = backups;
	Ok(())
}

fn encode_key(encoding: Encoding, path: &str) -> String {
	match encoding {
		Encoding::Plain => path.to_string(),
		Encoding::FileUrl => format!("file://{}", url_encode_path(path)),
	}
}

fn url_encode_path(path: &str) -> String {
	let mut out = String::with_capacity(path.len());
	for byte in path.bytes() {
		match byte {
			b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => out.push(byte as char),
			_ => out.push_str(&format!("%{byte:02X}")),
		}
	}
	out
}

fn boundary_starts_with(value: &str, prefix: &str) -> bool {
	value.len() >= prefix.len()
		&& value.as_bytes().starts_with(prefix.as_bytes())
		&& (value.len() == prefix.len() || value.as_bytes()[prefix.len()] == b'/')
}

fn count_anchored(conn: &Connection, target: &Target, needle: &str) -> rusqlite::Result<i64> {
	let sql = format!(
		"SELECT count(*) FROM {table} WHERE substr({column}, 1, length(?1)) = ?1 AND (length({column}) = length(?1) OR substr({column}, length(?1) + 1, 1) = '/')",
		table = target.table,
		column = target.column
	);
	conn.query_row(&sql, params![needle], |row| row.get(0))
}

fn update_anchored(conn: &Connection, target: &Target, old: &str, new: &str) -> rusqlite::Result<usize> {
	let sql = format!(
		"UPDATE {table} SET {column} = ?2 || substr({column}, length(?1) + 1) WHERE substr({column}, 1, length(?1)) = ?1 AND (length({column}) = length(?1) OR substr({column}, length(?1) + 1, 1) = '/')",
		table = target.table,
		column = target.column
	);
	conn.execute(&sql, params![old, new])
}

fn db_error(err: rusqlite::Error) -> String {
	format!("The database failed: {err}")
}

#[cfg(test)]
mod tests {
	use super::*;

	fn memory() -> Connection {
		Connection::open_in_memory().unwrap()
	}

	#[test]
	fn plain_target_matches_exact_and_child_only() {
		let conn = memory();
		conn.execute_batch(
			"CREATE TABLE media_parts (file TEXT);
			INSERT INTO media_parts VALUES ('/old'), ('/old/a.mp3'), ('/old/sub/b.mp3'), ('/old2/c.mp3'), ('/oldlive/d.mp3'), ('/other/e.mp3');",
		)
		.unwrap();
		let target = Target { table: "media_parts", column: "file", encoding: Encoding::Plain };
		assert_eq!(count_anchored(&conn, &target, "/old").unwrap(), 3);
		assert_eq!(update_anchored(&conn, &target, "/old", "/new").unwrap(), 3);
		let mut values = Vec::new();
		let mut stmt = conn.prepare("SELECT file FROM media_parts ORDER BY file").unwrap();
		let mut rows = stmt.query([]).unwrap();
		while let Some(row) = rows.next().unwrap() {
			values.push(row.get::<_, String>(0).unwrap());
		}
		assert_eq!(values, vec!["/new", "/new/a.mp3", "/new/sub/b.mp3", "/old2/c.mp3", "/oldlive/d.mp3", "/other/e.mp3"]);
	}

	#[test]
	fn url_target_uses_encoded_prefix() {
		let conn = memory();
		conn.execute_batch(
			"CREATE TABLE media_streams (url TEXT);
			INSERT INTO media_streams VALUES ('file:///media/Plex/Bandcamp/A%20B.lrc'), ('file:///media/Plex/Bandcamp%20Live/x.lrc');",
		)
		.unwrap();
		let target = Target { table: "media_streams", column: "url", encoding: Encoding::FileUrl };
		let old_key = encode_key(Encoding::FileUrl, "/media/Plex/Bandcamp");
		let new_key = encode_key(Encoding::FileUrl, "/media/Plex/Music - Bandcamp");
		assert_eq!(old_key, "file:///media/Plex/Bandcamp");
		assert_eq!(new_key, "file:///media/Plex/Music%20-%20Bandcamp");
		assert_eq!(count_anchored(&conn, &target, &old_key).unwrap(), 1);
		assert_eq!(update_anchored(&conn, &target, &old_key, &new_key).unwrap(), 1);
		let url: String = conn.query_row("SELECT url FROM media_streams WHERE url LIKE '%A%20B.lrc'", [], |row| row.get(0)).unwrap();
		assert_eq!(url, "file:///media/Plex/Music%20-%20Bandcamp/A%20B.lrc");
		let other: String = conn.query_row("SELECT url FROM media_streams WHERE url LIKE '%Live%'", [], |row| row.get(0)).unwrap();
		assert_eq!(other, "file:///media/Plex/Bandcamp%20Live/x.lrc");
	}

	#[test]
	fn url_encode_keeps_unreserved_and_slash() {
		assert_eq!(url_encode_path("/a b-c_d~e.f"), "/a%20b-c_d~e.f");
		assert_eq!(url_encode_path("/Music - Bandcamp"), "/Music%20-%20Bandcamp");
	}

	#[test]
	fn boundary_starts_with_separates_siblings() {
		assert!(boundary_starts_with("/old", "/old"));
		assert!(boundary_starts_with("/old/a", "/old"));
		assert!(!boundary_starts_with("/old2/a", "/old"));
		assert!(!boundary_starts_with("/oldlive", "/old"));
		assert!(!boundary_starts_with("/ol", "/old"));
	}

	#[test]
	fn validate_maps_rejects_overlap_and_chains() {
		let single = vec![Map { old: "/a".into(), new: "/b".into() }];
		assert!(validate_maps(&single).is_ok());
		let overlapping_old = vec![Map { old: "/a".into(), new: "/b".into() }, Map { old: "/a/c".into(), new: "/d".into() }];
		assert!(validate_maps(&overlapping_old).is_err());
		let chain = vec![Map { old: "/a".into(), new: "/b".into() }, Map { old: "/b".into(), new: "/c".into() }];
		assert!(validate_maps(&chain).is_err());
		let same = vec![Map { old: "/a".into(), new: "/a".into() }];
		assert!(validate_maps(&same).is_err());
	}
}
