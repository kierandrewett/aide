# aide

A terminal IDE that manages multiple Claude Code instances as tabbed workspaces.

## Requirements

- **Rust** (1.70+)
- **tmux** (3.0+)
- **claude-run** — wrapper script for Claude CLI (must be in `$PATH`)
- **git** — for git panel features

## Install

```bash
cargo build --release
cp target/release/aide ~/.local/bin/
```

## Usage

```bash
aide
```

aide discovers projects from `~/dev` by default. Override with:

```bash
AIDE_PROJECTS_DIR=/path/to/projects aide
```

## Architecture

```
src/
├── main.rs        # Entry point, event loop
├── app.rs         # Application state
├── ui/mod.rs      # ratatui rendering (layout, tabs, panels, status bar, picker)
├── tmux/mod.rs    # tmux process management
├── git/mod.rs     # Git status, log, branch, upstream queries
├── sessions/mod.rs # Session lifecycle management
└── input/mod.rs   # Keybinding mapping
```

## Layout

```
┌─ Sessions ────────────────┬─ Git Status ──────┐
│ Tab1 │ Tab2 │ Tab3        │ ## main            │
├───────────────────────────│  M src/main.rs     │
│                           │ ?? new_file.txt    │
│ Claude Output             ├─ Git Log ──────────┤
│ (scrollable viewport)     │ * abc1234 feat:... │
│                           │ * def5678 fix:...  │
│                           │                    │
├───────────────────────────┴────────────────────┤
│ project │ session   branch   ⇣2 ⇡1  Tab next… │
└────────────────────────────────────────────────┘
```

The right panel auto-hides when terminal width < 100 columns.

## Keybindings

| Key        | Action                    |
|------------|---------------------------|
| Tab        | Next instance             |
| Shift+Tab  | Previous instance         |
| Ctrl+T     | Create new instance       |
| Ctrl+P     | Project picker            |
| Ctrl+W     | Close instance (confirm)  |
| Ctrl+G     | Toggle right panel        |
| Ctrl+X     | Exit aide (sessions live) |
| Up/Down    | Scroll Claude output      |

## Execution Model

aide **never** calls `claude` directly. It invokes:

```
claude-run claude
```

inside tmux sessions. `claude-run` handles notifications, logging, and wrapping.

## Session Persistence

Sessions run in tmux and survive:
- SSH disconnects
- aide exits
- Terminal crashes

Restarting aide reconnects to existing sessions automatically.

## Refresh Intervals

| Data         | Interval |
|--------------|----------|
| Claude output| 500ms    |
| Git status   | 2s       |
| Git log      | 3s       |
