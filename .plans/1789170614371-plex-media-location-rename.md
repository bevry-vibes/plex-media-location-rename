# plex-media-location-rename plan

Assisted-by: ZCode · GLM 5.3 <zcode-clinepass-glm53@local>

Provenance: [the prompts that shaped this plan](./1789170614371-plex-media-location-rename.prompts.md)

## goal

The media folders `/Volumes/MediaDisk/Plex/Soundcoud` and `/Volumes/MediaDisk/Plex/Bandcamp` moved to `/Volumes/MediaDisk/Plex/Music - Soundcloud` and `/Volumes/MediaDisk/Plex/Music - Bandcamp`. The Plex database still stores the old paths. Plex must keep the added dates, the play history, and the playlists. The tool rewrites the stored paths in place. It must not force a remove and re-add of the library.

## findings from the probes

The probes ran read-only against `com.plexapp.plugins.library.db` on 2026-09-12. The database journal mode is WAL. The server was stopped. PowerShell 7.6.5 and Rust 1.97.1 are installed.

| Table.Column | Old string | Rows | New string |
|---|---|---|---|
| `section_locations.root_path` | `/Volumes/MediaDisk/Plex/Soundcoud` | 1 | `/Volumes/MediaDisk/Plex/Music - Soundcloud` |
| `section_locations.root_path` | `/Volumes/MediaDisk/Plex/Bandcamp` | 1 | `/Volumes/MediaDisk/Plex/Music - Bandcamp` |
| `media_parts.file` | `/Volumes/MediaDisk/Plex/Soundcoud/…` | 862 | prefix swap |
| `media_parts.file` | `/Volumes/MediaDisk/Plex/Bandcamp/…` | 355 | prefix swap |
| `media_streams.url` | `file:///Volumes/MediaDisk/Plex/Bandcamp/…` | 163 | prefix swap, percent encoded |

A database dump and search found no other table with the old paths. Other findings:

- The old name `Soundcoud` is the stored spelling. The rename is correct; the library folders were always misspelled.
- Both old roots belong to the Music library section 4. That section has four roots, so it stays live during the rewrite.
- `media_streams.url` stores lyric sidecar files as percent encoded `file://` URLs. All 163 rows belong to Bandcamp. None belong to Soundcoud.
- `directories.path` stores relative subpaths, so a root rename does not affect it.
- The `Music Analysis */Mapping.db` stores key on numeric ids, not on paths. The sonic analysis survives the rewrite.

## design

A Rust command line tool with these phases:

1. **Preflight.** Verify the database file exists. Verify no process whose name starts with `Plex` runs. Verify each map: the old path is absent from disk, the new path exists on disk, and the maps do not overlap.
2. **Analyse.** Count the affected rows per target with boundary anchored matching. Print the counts. A map that matches zero rows is an error, because the old path string may be wrong.
3. **Backup.** Checkpoint the WAL, then copy the library database, the blobs database, and any WAL files to timestamped names. The backup names follow the existing convention in the databases folder. Check each backup copy with `quick_check`.
4. **Update.** Run every rewrite inside one `BEGIN IMMEDIATE` transaction. Commit once. A failure rolls back all changes.
5. **Verify.** Count the old path rows again. The count must be zero per target. Check `integrity_check`. Report the counts per table.
6. **Report.** Print the backup paths, the changed rows, and the next steps.

Matching is boundary anchored: a value matches when it equals the old path, or when it starts with the old path and a `/` follows. Substring and wildcard matching are never used, so a path such as `Bandcamp Live` cannot match by accident. The update statements bind the old and new paths as parameters. No path string is spliced into SQL text.

The targets are data:

- `section_locations.root_path` — plain path.
- `media_parts.file` — plain path.
- `media_streams.url` — `file://` URL, percent encoded.

The added dates live in `metadata_items.added_at`, which the tool never touches. Watch state and playlists key on ids, so they survive too.

## risk register

| Risk | Severity | Mitigation |
|---|---|---|
| Database corruption from a concurrent server | Critical | The preflight refuses to run while a `Plex` process exists. The update takes the write lock with `BEGIN IMMEDIATE`. |
| Missing or bad backup | Critical | The backup runs before any write. The tool checks each backup copy. The tool has no option to skip the backup. |
| Over matching of paths | High | Boundary anchored matching only. The analyse phase prints the counts for review. A zero count is an error. |
| Unknown tables with paths | Medium | A database dump and search found every reference before this plan. The verification rechecks the targets. |
| Stale WAL state | Medium | The tool checkpoints the WAL before the backup and after the commit. The backup includes the WAL files. |
| Wrong old path string | Medium | The probes confirmed the stored strings. A map that matches zero rows aborts the run. |
| The server restarts during the run | High | The preflight rechecks the process list. The user starts the server only after the tool reports success. |
| Quoting of paths with spaces | Low | The statements bind parameters. No SQL text is built from paths. |
| Need to undo | Low | The timestamped backups restore the old state: stop the server, copy the backup over the live database, start the server. |
| Environment gaps | Low | The preflight reports a missing database or a locked file with clear instructions. |

## sequence

1. Commit the plan and the prompts.
2. Scaffold the project per the bevry-vibes skills.
3. Implement the tool in Rust.
4. Unit test the matching and the encoding.
5. Rehearse against a copy of both databases.
6. The live run waits for the explicit go from the user.

## notes

- The user switched the language from PowerShell to Rust on 2026-09-12. Rust gives real parameter binding, which removes the SQL quoting risk.
- The reciprocity check passed after the user turned off the ZCode "Improve experience" toggle.
