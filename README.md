# plex-media-location-rename

A Rust command line tool that rewrites moved media paths inside a Plex library database.

Plex stores the path of every media file inside `com.plexapp.plugins.library.db`. When a media folder moves or renames, those stored paths break. The usual fix inside Plex is to remove the library folder and add it again, which loses the added dates and the watch history. This tool instead rewrites the stored paths in place, so the added dates, the play history, and the playlists survive.

## usage

Stop the Plex server first. The tool refuses to run while a Plex process exists.

Find the database inside your Plex databases folder:

- macOS: `~/Library/Application Support/Plex Media Server/Plug-in Support/Databases/`
- Windows: `%LOCALAPPDATA%\Plex Media Server\Plug-in Support\Databases\`
- Linux: `/var/lib/plexmediaserver/Library/Application Support/Plex Media Server/Plug-in Support/Databases/`

Then run a dry run, with your own old and new folder paths:

```sh
cargo run --release -- \
	--database 'PATH/TO/com.plexapp.plugins.library.db' \
	--map '/media/Plex/Bandcamp=/media/Plex/Music - Bandcamp' \
	--dry-run
```

Read the printed row counts. When they match your expectation, run without `--dry-run` and confirm the prompt. The tool backs up the databases before it writes, rewrites the paths in one transaction, and verifies the result.

Options:

- `--map OLD=NEW` — one folder move. Repeat the flag for each move.
- `--database PATH` — path to `com.plexapp.plugins.library.db`.
- `--backup-dir PATH` — directory for the backup copies. The default is the database directory.
- `--dry-run` — analyse and print, change no data.
- `--yes` — skip the confirmation prompt.

## what it changes

| Table.Column | Form |
|---|---|
| `section_locations.root_path` | plain path |
| `media_parts.file` | plain path |
| `media_streams.url` | percent encoded `file://` URL |

A value changes only when it equals the old path, or when it starts with the old path and a `/` follows. A path such as `Bandcamp Live` cannot match by accident. The tool binds the paths as SQL parameters, so spaces and special characters stay safe. The added dates live in `metadata_items.added_at`, which the tool never touches.

## safety

- The tool writes a backup before any change. Each run makes timestamped copies of the databases, such as `com.plexapp.plugins.library.db-2026-09-12-081500`.
- Every rewrite runs inside one transaction. A failure rolls back all changes.
- The run verifies that no row still holds an old path.

Rehearse first when you can: copy the databases to a folder, run the tool with `--database` on the copy and `--backup-dir` on a scratch folder, then inspect the result.

## after the run

Start the Plex server. Open the library and run "Scan Library Files". Check that the added dates stayed, then play a track from each renamed folder.

## restore

To undo a run: stop the server, copy the backup over the live database file, then start the server.

```sh
cp 'com.plexapp.plugins.library.db-2026-09-12-081500' 'com.plexapp.plugins.library.db'
cp 'com.plexapp.plugins.library.blobs.db-2026-09-12-081500' 'com.plexapp.plugins.library.blobs.db'
```

## troubleshooting

- The build of the bundled SQLite needs the macOS SDK. If the build fails with `stdio.h` not found, export `SDKROOT` first: `export SDKROOT="$(xcrun --show-sdk-path)"`.
- The tool skips a full integrity check, because the Plex full text tables use tokenizers that exist only inside Plex. The zero old rows, the new row counts, and the one transaction provide the verification.

<!-- LICENSE/ -->

## License

Unless stated otherwise all works are:

- Copyright &copy; [Benjamin Lupton](https://balupton.com)

and licensed under:

- [Reciprocal Public License 1.5](http://spdx.org/licenses/RPL-1.5.html)

<!-- /LICENSE -->
