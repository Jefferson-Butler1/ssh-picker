# ssh-picker

A minimal, vim-friendly Ratatui wrapper around your `~/.ssh/config` that lets you quickly browse, filter, add, edit, delete, and connect to hosts.

- No database. No daemons. Just edits `~/.ssh/config` directly.
- Vim-style navigation and a clean terminal UI.
- Safe by default: easy to back up and revert.

## Requirements
- Rust (stable). If needed, update with:
  ```sh
  rustup update stable
  ```
  Note: The project currently targets Rust edition "2024" to match common toolchains.

## Build & Run
```sh
# from the repo root
cargo build
cargo run
```

## Install (optional)
```sh
# install a release binary into ~/.cargo/bin
cargo install --path .
```

## Use
- Run `ssh-picker` to launch the UI.
- Select a host and press Enter to connect using your system `ssh`.
- After the SSH session ends, you return to the picker.

### Shell integration (replace bare `ssh`)
Add this to your shell config (e.g., `~/.zshrc` or `~/.bashrc`):
```sh
ssh() {
  if [ $# -eq 0 ]; then
    command ssh-picker
  else
    command ssh "$@"
  fi
}
```
- Typing `ssh` with no args opens the picker.
- Typing `ssh user@host` works as normal.

## Keybindings
- j / k or Down / Up: move selection
- Enter: ssh to selected host (ignored while a confirm dialog is open)
- /: start filter; type to filter; Esc to exit filter
- a: add a host
- e: edit selected host
- d: delete selected host (confirm with y / n or Esc)
- PageDown / Ctrl-f: page down
- PageUp / Ctrl-b: page up
- q: quit

## What gets edited
- Hosts are read from and written to `~/.ssh/config`.
- Add/Edit writes a `Host <pattern>` block with common fields (`HostName`, `User`, `Port`).
- Delete removes the entire `Host <pattern>` block.

### Safety & backups
This tool edits `~/.ssh/config`. Before first use, consider:
```sh
cp ~/.ssh/config ~/.ssh/config.bak
```
If you want to revert, restore the backup.

## Limitations (by design for simplicity)
- Only the main `~/.ssh/config` file is parsed. `Include`d files are ignored.
- Each `Host` entry is treated as a single pattern (e.g., `Host my-alias`).
- Comments inside replaced host blocks are not preserved when you edit that host (we append/replace cleanly).
- Only a small set of fields are editable in-UI. You can still hand-edit `~/.ssh/config` for advanced options.

## Troubleshooting
- UI looks garbled after exiting SSH: the app re-initializes the terminal automatically; if it still looks off, press `q` and relaunch.
- Edition mismatch errors: update Rust (`rustup update stable`).
- Nothing shows up: ensure you have at least one `Host` block in `~/.ssh/config`.

## Roadmap
- Optional app config (colors, defaults) in `~/.config/ssh-picker/config.toml`.
- Support for reading from `Include`d files.
- Preserve comments within edited blocks.
- Mosh support: choose to connect with `mosh` if available; per-host toggle in edit form.
- Quick connect: a command palette for ad-hoc `user@host` without saving.
- Theming: light/dark, accent color config, and minimal/compact list styles.
- Per-host actions: open SSH in new tab/split (iTerm/WezTerm integration when possible).
- Export/import: sync host entries via a simple TOML/JSON format.
- Advanced edit: optional multiline editor for raw host block.

---
Built with Ratatui + Crossterm. Minimal by intent; PRs to keep it simple are welcome.

