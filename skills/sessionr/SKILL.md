---
name: sessionr
description: "Control sessionr, a session-first terminal multiplexer for coding agents. Use only when the user explicitly mentions sessionr or asks to use sessionr to inspect or control sessions, panes, tabs, workspaces, commands, or another agent. Do not use merely because a task could benefit from a background terminal, delegation, or parallel work. Requires HERDR_ENV=1."
---

# sessionr

sessionr organizes terminal work into sessions, tabs, and panes, recognizes coding agents running inside panes, and exposes the current session through the `sessionr` CLI. A session is a named unit of work with a goal, an optional git worktree, and the agents inside it. The sidebar is session-first: a focused card, then active sessions sorted by recency, then a settled archive.

Before issuing any control command, verify that this agent is running inside a sessionr-managed pane:

```bash
test "${HERDR_ENV:-}" = 1
```

If the check fails, say that you are not running inside sessionr and stop. Do not inspect or control the focused sessionr session from outside sessionr.

When the check passes, the `sessionr` binary in `PATH` talks to the current session. Use it to inspect neighboring work, create terminal layout, start agents and commands, read output, and wait for state changes.

sessionr is a fork of herdr: the `herdr` CLI verbs for workspaces, worktrees, tabs, and panes are unchanged and still work, so existing herdr skills and scripts keep running.

## Learn the current CLI

The installed binary is the authority for command syntax. Start with:

```bash
sessionr --help
```

Then print the relevant command group by running the group without a subcommand:

```bash
sessionr agent
sessionr pane
sessionr workspace
sessionr tab
sessionr worktree
sessionr terminal
sessionr notification
sessionr integration
sessionr session
sessionr instance
```

Do not run bare `sessionr` for discovery; it launches or attaches the TUI. Do not probe a mutating nested command by omitting arguments. Commands such as `sessionr session create` are valid with defaults and will execute.

Most control commands return JSON. Read identifiers and state from those responses instead of predicting them.

## Sessions

A session is the unit of work in the sidebar. Each session has:

- a `goal` — the title, editable with `sessionr session goal <session_id> <text>`.
- a `label` — a short name used in the collapsed sidebar and CLI ergonomics; defaults to the branch or worktree slug.
- an optional git worktree — the isolation for the work; `sessionr session create --worktree` composes the worktree and session in one command.
- a `settled` state — settled sessions are archived (dimmed, sorted by settle time, collapsed behind "Show N more"). Settle with `sessionr session settle <session_id>`, undo with `sessionr session unsettle <session_id>`.

Create, list, and focus sessions with:

```bash
sessionr session create --goal "fix billing bug" [--cwd PATH] [--label NAME] [--worktree] [--branch NAME] [--base REF] [--focus]
sessionr session list
sessionr session focus <session_id>
```

Non-git directories still work: `--worktree` is a no-op there. `sessionr workspace` remains available as a hidden compatibility alias for the same underlying workspaces.

## Background server sessions

A persistent background server session (the long-running process that owns the panes) is managed with the `instance` verb:

```bash
sessionr instance list [--json]
sessionr instance attach <name>
sessionr instance stop <name> [--json]
sessionr instance delete <name> [--json]
```

The `--session <name>` launch flag is kept as an alias for `--instance <name>`. Use `default` as the name to target the default instance.

## Understand layout, panes, and agents

Choose the primitive that matches the job:

- Session, tab, and pane topology organize terminal locations.
- Pane commands control raw terminals, shells, tests, servers, input, and output.
- Agent commands control the recognized coding agent currently occupying a pane.

A pane exists whether or not it contains an agent. `agent start` requires an existing available shell pane and never creates, splits, or moves layout. Use pane commands for ordinary processes. Use agent commands when sessionr must validate agent identity or interpret `idle`, `working`, `blocked`, `done`, and `unknown` lifecycle states.

Agent commands accept either a unique live agent name or the pane ID currently hosting that agent. They do not accept terminal IDs or bare agent-kind labels. Names must match `[a-z][a-z0-9_-]{0,31}` and be unique among live agents. A name follows the current pane occupant and is cleared when that agent exits, is released, or is replaced.

`idle` means the agent is ready for input and its tab has been seen in the focused sessionr UI. `done` is the same underlying idle state after unseen background work finishes. Focusing the tab or targeting the pane or agent with a focus command marks it seen. CLI reads do not mark it seen. `blocked` means sessionr recognized an approval or question UI. `unknown` means an agent is present but sessionr cannot classify it confidently; it does not prove completion.

## Use IDs and caller context

Public IDs are opaque stable handles:

- workspace: `w1`
- tab: `w1:t1`
- pane: `w1:p1`

Closed tab and pane IDs are not reused. A pane moved into another workspace receives a new workspace-qualified pane ID. After `pane move`, continue with `.result.move_result.pane.pane_id` or the live agent name. The old value is reported as `.result.move_result.previous_pane_id`; only the moved process's inherited caller context keeps resolving that old ID, so do not use it as a general agent target.

sessionr injects the caller's context into each managed pane:

```bash
printf '%s\n' "$HERDR_WORKSPACE_ID" "$HERDR_TAB_ID" "$HERDR_PANE_ID"
```

Prefer `--current` when a pane command should target the calling pane. Omitting a target may use the UI-focused pane, which can belong to the user or another client.

Discover live state with:

```bash
sessionr workspace list
sessionr tab list --workspace "$HERDR_WORKSPACE_ID"
sessionr pane current --current
sessionr pane list --workspace "$HERDR_WORKSPACE_ID"
sessionr agent list
```

Creation responses expose the IDs to use next. `workspace create` returns `.result.workspace`, `.result.tab`, and `.result.root_pane`. `tab create` returns `.result.tab` and `.result.root_pane`. `pane split` returns the new pane as `.result.pane`.

## Start and coordinate an agent

Default to a sibling pane in the current tab and the current working directory. Do not create a session, tab, worktree, or different cwd unless the user explicitly requests that topology or location.

Honor a direction requested by the user. Otherwise inspect the caller pane:

```bash
sessionr pane layout --pane "$HERDR_PANE_ID"
```

Split a wide pane to the right and a narrow or tall pane down. Avoid repeated same-direction splits that create unusably narrow columns or short rows. Keep the user's focus in the calling pane and explicitly preserve the caller's working directory:

```bash
sessionr pane split --current --direction right --cwd "$PWD" --no-focus
```

Replace `right` with `down` when appropriate. Read the new pane ID from `.result.pane.pane_id`.

An available shell pane must be at its interactive prompt, with the shell itself in the foreground and no foreground command, editor, or agent running. Start a supported agent in that pane with a useful unique name:

```bash
sessionr agent start reviewer --kind codex --pane <returned-pane-id>
```

Use the kind requested by the user. Run `sessionr agent` to inspect the installed kind list and options. Pass native agent arguments only after `--`:

```bash
sessionr agent start reviewer --kind codex --pane <returned-pane-id> -- <agent-args...>
```

`agent start` returns only after sessionr detects the expected agent in the same pane and considers it ready for interactive input. It defaults to a 30-second startup timeout.

Submit work through the agent surface:

```bash
sessionr agent prompt reviewer "Review the current diff and report only actionable findings." --wait --timeout 120000
```

`agent prompt` sends text, then encoded Enter after a short delay, while honoring the pane's live bracketed-paste mode. If the agent is already `blocked`, it returns `agent_blocked` without sending input; inspect the dialog and use `agent send-keys` for a deliberate response. For normal agent work, `--wait` is enough: it waits for the first settled `idle`, `done`, or `blocked` state reached after an accepted submission. Do not repeat those defaults with `--until`.

An accepted prompt sent from another non-working state must produce an observed lifecycle change within five seconds. Otherwise sessionr returns `agent_prompt_stalled` instead of waiting indefinitely; if the caller sets `--timeout` to five seconds or less, sessionr returns the normal `timeout` error instead. This wait tracks lifecycle state, not an individual turn; if the agent is already working, completion of the active turn may satisfy it.

Use `--until` only for a state-specific workflow, such as waiting for an already-running agent to request input:

```bash
sessionr agent wait reviewer --until blocked --timeout 120000
```

Without `--until`, standalone `agent wait` uses the same settled-state defaults as `agent prompt --wait`.

Use logical keys for interactive agent UI controls:

```bash
sessionr agent send-keys reviewer esc
sessionr agent send-keys reviewer ctrl+c
```

sessionr validates all keys before writing any bytes. Read the result through the resolved agent:

```bash
sessionr agent get reviewer
sessionr agent read reviewer --source recent-unwrapped --lines 120
```

If a wait fails or returns `blocked`, inspect `agent get` and `agent read` before deciding what input to send. Use the pane surface only when raw terminal control is intentional.

## Run an ordinary command in another pane

Create a sibling pane with the same geometry rule, preserve the caller's working directory, and keep user focus unchanged:

```bash
sessionr pane split --current --direction right --cwd "$PWD" --no-focus
```

Read the new pane ID from `.result.pane.pane_id`, then run and inspect the command:

```bash
sessionr pane run <returned-pane-id> "just test"
sessionr pane wait-output <returned-pane-id> --match "test result" --timeout 120000
sessionr pane read <returned-pane-id> --source recent-unwrapped --lines 120
```

`pane run` atomically sends command text and Enter. `pane wait-output` searches the selected snapshot immediately, so output that already exists can match. Use `--match <text>` for a literal substring or `--regex <pattern>` for a Rust regular expression. Omitting `--timeout` allows an indefinite wait.

Use the read source that matches the task:

- `visible`: the currently rendered viewport.
- `recent`: recent rendered output, including soft wraps.
- `recent-unwrapped`: recent output with soft wraps joined; prefer it for logs and transcripts.
- `detection`: the plain-text bottom-buffer snapshot used for agent detection.

Use `--format ansi` when colors and terminal styling are evidence. Otherwise use text.

`--lines` asks sessionr for more rows from the pane's available screen and host scrollback. If increasing it does not reveal more of a completed response, the pane is probably running the agent on the terminal's alternate screen. Rows that leave the alternate screen do not enter sessionr's host scrollback, so a larger line count cannot recover them.

After that failed read, ask the agent to write its complete response as Markdown in a temporary directory and reply only with the file path, then read the file directly. Use this only as a fallback; do not request file output in the initial prompt.

## Safety and coordination rules

- Use `--no-focus` for background work unless the user asked to switch context.
- Use `--current`, an explicit pane ID, or a unique agent name. Do not rely on another client's focused pane.
- Parse IDs from JSON responses. Do not derive them from sidebar order or examples.
- Do not close sessions, workspaces, tabs, panes, or instances you did not create unless the user explicitly asked.
- Never run `sessionr instance stop` from an active session unless the user explicitly intends to stop the server and its pane processes.
- Never kill the main sessionr process. Use named instances for experiments that need an isolated server.
- CLI server errors are JSON on stderr with exit status 1. CLI syntax errors exit with status 2.
