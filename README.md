<p align="center">
  <h1 align="center">aide</h1>
  <p align="center">A terminal IDE for managing multiple Claude Code sessions in tabbed workspaces.</p>
</p>

<p align="center">
  <a href="https://github.com/kierandrewett/aide/releases">Download</a> &middot;
  <a href="#install">Install</a> &middot;
  <a href="#usage">Usage</a> &middot;
  <a href="#configuration">Configuration</a>
</p>

---

**aide** wraps [Claude Code](https://docs.anthropic.com/en/docs/claude-code) in a proper workspace — tabs, git context, and keyboard-driven navigation — all inside your terminal. Each session runs in tmux, so they survive disconnects, crashes, and restarts.

```
┌─ Sessions ────────────────┬─ Git Status ──────┐
│ api │ frontend │ infra     │ ## main            │
├───────────────────────────│  M src/main.rs     │
│                           │ ?? new_file.txt    │
│  Claude Code output       ├─ Git Log ──────────┤
│  with full ANSI color     │ * abc1234 feat:... │
│                           │ * def5678 fix:...  │
│                           │                    │
├───────────────────────────┴────────────────────┤
│ ~/d/api │ api  main ✓     ^T new ^G git ^X exit│
└────────────────────────────────────────────────┘
```

## Why aide?

- **Multiple Claude sessions at once** — work on your API, frontend, and infra in parallel, each in its own tab
- **Git at a glance** — branch, status, upstream, and log always visible without switching context
- **Sessions that survive everything** — powered by tmux, so SSH drops and terminal crashes don't kill your work
- **Zero config to start** — just run `aide` and go. Customize later if you want
- **Keyboard native** — everything is a keystroke away. All non-reserved keys pass straight through to Claude

## Install

### Quick install (Linux / macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/kierandrewett/aide/main/install.sh | bash
```

Pin a specific version:

```bash
./install.sh v0.1.0
```

Change install location:

```bash
AIDE_INSTALL_DIR=/usr/local/bin curl -fsSL https://raw.githubusercontent.com/kierandrewett/aide/main/install.sh | bash
```

### From GitHub releases

Grab the latest binary from [Releases](https://github.com/kierandrewett/aide/releases):

| Platform              | Archive                      |
|-----------------------|------------------------------|
| Linux (x86_64)        | `aide-x86_64-linux.tar.gz`   |
| Linux (aarch64/ARM)   | `aide-aarch64-linux.tar.gz`  |
| macOS (Intel)         | `aide-x86_64-macos.tar.gz`   |
| macOS (Apple Silicon)  | `aide-aarch64-macos.tar.gz` |

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

**Requirements:** tmux 3.0+, git

## Usage

```bash
aide
```

That's it. aide scans your projects directory (`~/dev` by default), and you pick what to work on.

### Keybindings

| Key         | Action                           |
|-------------|----------------------------------|
| `Tab`       | Next session                     |
| `Shift+Tab` | Previous session                |
| `Ctrl+T`   | New session (project picker)     |
| `Ctrl+P`   | Project picker                   |
| `Ctrl+W`   | Close session (with confirmation)|
| `Ctrl+G`   | Toggle git panel                 |
| `Ctrl+X`   | Quit aide                        |
| `PgUp/PgDn`| Scroll output                    |
| Mouse wheel | Scroll output                   |

Everything else goes straight to Claude Code — type normally.

### Responsive layout

On wide terminals (100+ cols), you get the full split view: Claude output on the left, git panels on the right. On narrow terminals, borders are stripped and the git panel becomes a fullscreen overlay toggled with `Ctrl+G`.

## Configuration

Config lives at `~/.config/aide/config.toml` and is created automatically on first run:

```toml
command = "claude"
projects_dir = "/home/you/dev"
```

| Key            | What it does                              | Default  |
|----------------|-------------------------------------------|----------|
| `command`      | Command to launch in each tmux session    | `claude` |
| `projects_dir` | Directory to scan for project folders     | `~/dev`  |

### Custom commands

The `command` field is flexible — use it to wrap Claude with your own tooling:

```toml
# Run through a wrapper script
command = "my-claude-wrapper"

# Pass flags
command = "claude --verbose"
```

aide never calls `claude` directly. It runs your configured command inside tmux, so the command is responsible for any setup, notifications, or logging you need.

## How it works

aide creates a tmux session per project. Your keystrokes are forwarded to the active tmux pane in real-time, and the pane output (with full ANSI color) is captured and rendered in the TUI at ~60fps. Git data refreshes in the background on a 2-3 second cycle.

Because sessions are just tmux, they persist across:
- aide exits and restarts (auto-reconnects)
- SSH disconnects
- Terminal crashes
- System sleep/wake

### Architecture

```
src/
├── main.rs         Event loop, key forwarding, refresh timers
├── app.rs          Application state, project discovery
├── config.rs       TOML config (~/.config/aide/config.toml)
├── ui/mod.rs       TUI rendering (ratatui)
├── tmux/mod.rs     tmux process management
├── git/mod.rs      Git queries (status, log, branch, upstream)
├── sessions/mod.rs Session lifecycle
└── input/mod.rs    Input handling, key batching, passthrough
```

Built with [ratatui](https://github.com/ratatui/ratatui) + [crossterm](https://github.com/crossterm-rs/crossterm) + tmux.

## Releasing

Releases are automated via GitHub Actions when a version tag is pushed:

```bash
git tag v0.1.0
git push origin v0.1.0
```

Builds binaries for all 4 platforms (x86_64/aarch64 Linux + macOS), packages them with SHA256 checksums, and publishes a GitHub release.

## License

MIT
