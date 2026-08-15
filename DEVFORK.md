# sessionr — fork dev notes

sessionr is a fork of [herdr](https://github.com/herdrdev/herdr). Upstream remote: `upstream`, base branch `upstream/master`. Work lands on `sessions-sidebar` (or feature branches off it).

## Build

macOS 26 SDK breaks zig 0.15.2 (libghostty-vt pins exactly 0.15.2). Build against the older CommandLineTools SDK:

```bash
export DEVELOPER_DIR=/Library/Developer/CommandLineTools
export ZIG=$HOME/.local/share/mise/installs/zig/0.15.2/bin/zig
cargo build
```

## Rename layer (done)

- Package/binary: `sessionr` (Cargo.toml)
- Config/state dir: `~/.config/sessionr` (release) / `~/.config/sessionr-dev` (debug) — `src/config/io.rs::app_dir_name`
- User-facing CLI strings: `src/cli.rs`, `src/main.rs`
- Update checks: default OFF (`src/config/model.rs::UpdateConfig` default + test). Upstream checks point at herdr.dev and would offer herdr releases.
- Kept for compatibility: `HERDR_*` env vars (existing herdr skill keeps working inside sessionr panes), socket/plugin internals, `skills/herdr/` skill, herdr.dev doc URLs in agent prompts.

## Design

See DESIGN.md (product + sidebar spec) and mockup.html.

## Milestones

1. [x] Fork + rename layer
2. [x] Data model: `goal`, `last_activity`, `settled` on Workspace + persistence + touch points
3. [x] Sidebar v1: focused card + flat session list + marks + settled section
4. [x] Repo on branch line, recency sort
5. [ ] Worktree-first create flow (`session create --goal ... --worktree`)
6. [ ] Collapsed mode, config tokens, docs

## Sync upstream

```bash
git fetch upstream
git checkout sessions-sidebar
git merge upstream/master
```
