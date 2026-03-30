# aide

A terminal IDE that manages multiple Claude Code instances as tabbed workspaces, powered by tmux.

## Install

### Quick install (Linux / macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/kierandrewett/aide/main/install.sh | bash
```

Or specify an install directory:

```bash
AIDE_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/kierandrewett/aide/main/install.sh | bash
```

### From GitHub releases

Download the latest binary for your platform from [Releases](https://github.com/kierandrewett/aide/releases):

| Platform             | Archive                      |
|----------------------|------------------------------|
| Linux (x86_64)       | `aide-x86_64-linux.tar.gz`   |
| Linux (aarch64/ARM)  | `aide-aarch64-linux.tar.gz`  |
| macOS (Intel)        | `aide-x86_64-macos.tar.gz`   |
| macOS (Apple Silicon) | `aide-aarch64-macos.tar.gz` |

```bash
tar xzf aide-*.tar.gz
mv aide ~/.local/bin/
```

### Build from source

```bash
git clone https://github.com/kierandrewett/aide.git
cd aide
cargo build --release
cp target/release/aide ~/.local/bin/
```

## Requirements

- **tmux** (3.0+)
- **claude-run** — wrapper script for Claude CLI (must be in `$PATH`)
- **git** — for git panel features

## Usage

```bash
aide
```

aide discovers projects from `~/dev` by default.

## Configuration

Config file is created automatically at `~/.config/aide/config.toml` on first run:

```toml
command = "claude-run claude"
projects_dir = "/home/you/dev"
```

| Key            | Description                                    | Default              |
|----------------|------------------------------------------------|----------------------|
| `command`      | Command to run in each tmux session            | `claude-run claude`  |
| `projects_dir` | Directory to scan for projects                | `~/dev`              |

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
│ /path │ project │ session  branch  ⇣2 ⇡1  ... │
└────────────────────────────────────────────────┘
```

The right panel auto-hides when terminal width < 80 columns. Toggle it with **Ctrl+G**.

All keystrokes not reserved by aide are forwarded directly to the active tmux session, so you can type into Claude Code normally.

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

All other keys are passed through to the Claude Code session.

## Execution Model

aide **never** calls `claude` directly. It invokes the configured `command` (default: `claude-run claude`) inside tmux sessions. The wrapper command is responsible for notifications, logging, and any other concerns.

## Session Persistence

Sessions run in tmux and survive:
- SSH disconnects
- aide exits
- Terminal crashes

Restarting aide reconnects to existing sessions automatically.

## Architecture

```
src/
├── main.rs         # Entry point, event loop, key forwarding
├── app.rs          # Application state, project discovery
├── config.rs       # TOML config loading (~/.config/aide/config.toml)
├── ui/mod.rs       # ratatui rendering (layout, tabs, panels, status bar, picker)
├── tmux/mod.rs     # tmux process management
├── git/mod.rs      # Git status, log, branch, upstream queries
├── sessions/mod.rs # Session lifecycle management
└── input/mod.rs    # Keybinding mapping + passthrough
```

## Refresh Intervals

| Data          | Interval |
|---------------|----------|
| Claude output | 500ms    |
| Git status    | 2s       |
| Git log       | 3s       |

## Releasing

Releases are built automatically via GitHub Actions when a version tag is pushed:

```bash
git tag v0.1.0
git push origin v0.1.0
```

This builds binaries for all platforms and creates a GitHub release with archives and checksums.
