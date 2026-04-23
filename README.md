# Ratatui Email Client

Open source email client written in Rust.

## Build Instructions

You need to have latest versions Rust and Cargo to run this project.

Clone this repo and run

```shell
cargo build
```

To get it running first see the [setup instructions](./docs/SETUP.md).

If setup is complete, run the program with

```shell
cargo run
```

## Terminal Client Keybindings

### View Modes
These keybindings are context-sensitive and depend on the active screen:
- **Inbox View**: thread list and navigation
- **Thread View**: selected conversation details
- **Compose View**: drafting a new message or reply

### Global
- `q` — Quit the application

### Inbox View
- `j` / `↓` — Move selection down
- `k` / `↑` — Move selection up
- `Enter` — Open selected thread
- `r` — Refresh inbox
- `n` — New compose message
- `q` — Quit

### Thread View
- `Esc` / `b` — Back to inbox
- `r` — Refresh current thread
- `n` — New compose message
- `y` — Reply to current thread
- `q` — Quit

### Compose View
- `Tab` — Move to next field (`To` → `Subject` → `Body`)
- `Shift+Tab` — Move to previous field
- `Enter` — In `To`/`Subject`: move to next field; in `Body`: insert newline
- `Backspace` — Delete one character in active field
- `Ctrl+s` — Send message
- `Esc` — Cancel compose
- `q` — Quit
