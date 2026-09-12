# AGENTS.md

This project conforms to Bevry's skills.
Reference their remote URLs only — do not pull their contents into this file.
When a referenced skill applies with your project's tweaks, the local `<name>.md` file at this repo root references the remote URL and lists the tweaks underneath;
this process is documented in the upstream repo's [local tweaks pattern](https://github.com/bevry-vibes/skills#local-tweaks-pattern).

- https://github.com/bevry-vibes/skills/blob/main/policy.md — **applies**. Check the reciprocity of the harness, the provider, and the model with agent-detect before you work.
- https://github.com/bevry-vibes/skills/blob/main/commits.md — **applies**. Conventional Commits, the system git identity as author, and the agent-detect co-author trailer.
- https://github.com/bevry-vibes/skills/blob/main/plans.md — **applies**. Plans live in `.plans/` with a prompts companion.
- https://github.com/bevry-vibes/skills/blob/main/build.md — does not apply. This project is a command line tool, not a menu-bar or tray app.
- https://github.com/bevry-vibes/skills/blob/main/powershell.md — does not apply. The implementation language is Rust, not PowerShell.
- https://github.com/bevry-vibes/skills/blob/main/zig.md — does not apply. The implementation language is Rust, not Zig.
- https://github.com/bevry-vibes/skills/blob/main/minimax.md — does not apply. The active model is GLM 5.3, not MiniMax M3.

## project rules

- Run the tool with `cargo run --release` or `cargo test` from the repository root.
- The tool writes to a Plex database. Rehearse against a copy before you run it against the live database.
- Stop the Plex server before the tool writes to the database.
