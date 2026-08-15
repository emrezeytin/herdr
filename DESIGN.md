# sessionr — design

A fork of [herdr](https://github.com/herdrdev/herdr) (Rust terminal multiplexer for coding agents) with one product change: **the sidebar is session-first**. Workspaces and the global agents panel are gone. The sidebar is a Linear-style task list where each row is a session — a named unit of work with a goal, a git worktree, and the agents inside it.

## Product thesis

Sessions = tasks. Worktrees = isolation. Agents = workers. One repo, N parallel tasks, each in its own worktree, each with its agents, ordered by recency and attention.

## Sidebar anatomy

```
┌────────────────────────────┐
│ ⟳ Prototype landing…      │  ← session: name first line,
│   main                     │     mark + goal
│ ● Smaller campaign…        │  ← bare branch on second
│   main                     │     line, herdr-style indent
│ ✓ Check size order…        │  ← sorted by recency desc
│   main                     │
├────────────────────────────┤
│ Settled                    │  ← archive section header
│ ·  Mobile app setup help   │     dimmed, settled_at desc
│    main                    │
│ + Show 4 more              │  ← collapse old, shows count
└────────────────────────────┘
```

Rules:

- **Focused session** keeps its row highlight in the list (no separate card; the card was dropped in favor of a plain list).
- **Session row** = session name on the first line (gutter state mark + goal + right-aligned relative time; goal wraps 2 lines), branch line second — bare branch name on a three-space indent, matching herdr's spacing (no `⎇` glyph, no repo suffix). No group headers, no avatars — the session name leads.
- **Gutter mark** = rolled-up agent state: `⟳` working, `●` blocked, `✓` done, `·` idle/plain shell, dim dot = no agents.
- **Repo** = the session's git space (herdr's worktree-space metadata), shown dim on the branch line. Sorting is recency desc overall; same-repo sessions need no contiguity.
- **Settled** = manually marked (`session settle`). Auto-archive after N days inactivity is a later option; manual only at first. Settled sessions dim, sort by settled_at desc, collapse behind "Show N more".
- **Recency drives everything.** Every row shows relative time (`now`, `2h`, `7d`). Sessions sort by last_activity desc. New plumbing: `last_activity` timestamps (herdr has seq counters, not wall clock).
- **No agents panel.** The cross-session "who needs me" scan is the marks + recency; blocked sessions can float up (v2).
- **Tabs stay** as internal views (main area tab bar). Never in the sidebar.

## Session semantics

| Field | Meaning |
| --- | --- |
| `label` | short name (collapsed sidebar, CLI ergonomics). Defaults to branch/worktree slug. |
| `goal` | the title. User-set at creation, editable, wraps to 2 lines. |
| `branch` / worktree | the isolation. Optional — non-git dirs and non-worktree sessions still work, show `⎇ main` or no branch line. |
| `last_activity` | wall-clock ts, touched on agent state change, pane output, or focus. |
| `settled` | `Option<i64>` — `None` = active, `Some(ts)` = settled. |

Creation: `sessionr session create --goal "fix billing bug" [--cwd X] [--worktree] [--branch NAME] [--base REF]` — one command, reuses herdr's existing worktree plumbing (`worktree create` → workspace). Non-git dirs: worktree flag is a no-op with a note.

## Fork strategy

**Presentation fork.** Keep the internal `Workspace` type and the server model untouched; rename the surface (UI, CLI, config, docs). New fields get serde defaults so old snapshots load clean. This keeps upstream merges cheap and the diff reviewable.

- Internal: `Workspace` gains `goal`, `last_activity`, `settled` (serde default). Alias `Session = Workspace` where useful.
- CLI: `session` verb replaces `workspace` in help and docs; `workspace` stays as a hidden alias so existing herdr skills/scripts keep working.
- Config: `ui.sidebar.spaces` → `ui.sidebar.sessions`, default rows `[goal, branch]`; agents rows config reused for the focused-card chips.
- API schema: unchanged internally; docs rewritten in session terms.

## Implementation map (herdr v0.8.0)

### 1. Data model — `src/workspace.rs`, `src/persist.rs`
- Add `goal: Option<String>`, `last_activity: Option<i64>`, `settled: Option<i64>` with serde defaults.
- Touch points for `last_activity`:
  - `src/app/actions.rs` ~L3008 (`next_agent_state_change_seq` block — agent state change).
  - Pane output path (input handling) — every rendered/streamed chunk touches its session.
  - Session focus.

### 2. Sidebar — `src/ui/sidebar.rs`
- Delete the two-section split (`render_workspace_list` + `render_agent_detail`, `sidebar_section_heights`).
- New renderers: `render_session_row` (mark + name + branch + repo + time), `render_settled_section` (+ show-more collapse).
- Reuse: token machinery (`tokens.rs`), `state_icon`, scrollbar, truncation utils.
- New utils: relative-time formatting.
- Input — `src/app/input/sidebar.rs`: navigation moves over sessions; add `settle`/`unsettle` keys, goal edit prompt.

### 3. CLI — `src/cli/`
- `session create|list|focus|settle|unsettle|goal|rename` — create composes worktree create + workspace create.
- Hidden `workspace` aliases.

### 4. Config — `src/config/sidebar.rs`
- `sessions` section (renamed `spaces`), rows `[goal, branch]`, plus `settled` display options.

### 5. Rename layer
- Binary `sessionr`, docs, skills, README. Keep socket API compatible.

### Milestones
1. Data fields + persistence + touch points + CLI `session settle/list`.
2. Sidebar v1: focused card + flat session list + marks + settled section.
3. Repo on branch line, recency sort.
4. Worktree-first create flow (goal dialog → worktree → session).
5. Collapsed mode, config tokens, "show more", docs.
6. Auto-archive after N days (postponed).

## Open questions (parked)

- Auto-archive threshold (N days) — later.
- Blocked sessions floating to top — v1 sorts by recency only, marks carry attention.
- Global "all agents" filter view — dropped for v1; revisit if marks prove too weak.
