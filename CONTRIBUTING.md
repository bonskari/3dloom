# Contributing

## Rules the tests enforce

1. **Every action is a `loom_commands::Command`.** Buttons, hotkeys, quad
   menus, the control socket and MCP all call `execute()`. UI code never
   mutates the `Scene` directly.
2. **Every action in the shortcut tables is accounted for.** Each row of
   `docs/reference/*-shortcuts.tsv` is a registered command, or is listed in
   `OUT_OF_SCOPE` / `DEFERRED` in `crates/loom_commands/src/command.rs` with a
   reason. `keymap::tests` fails otherwise. Commands with no row in the tables
   (agent and scripting helpers) go in `Group::Loom`.
3. **MCP parity:** `loom_mcp/tests/parity.rs` fails if a command has no tool.
   Tools are generated from the registry; never write one by hand.
4. **Undo is change-level** (`loom_scene::Change`): every scene edit goes
   through `UndoStack::commit`, and an interactive drag merges into one step.
5. **Every parameter is a `Param<T>`**, so it can be animated.

## Adding a command

1. Add a row to the `commands!` macro in `crates/loom_commands/src/command.rs`:
   variant, display name, group, description and typed arguments.
2. Handle it in `crates/loom_commands/src/execute.rs` (or the module the
   dispatcher routes its group to); build a `Change` and `commit()` it.
3. Add a test under `crates/loom_commands/tests/`.
4. If the command has a shortcut row, remove it from `DEFERRED`; the key map
   binds it from the table.
5. In the UI call `app.dispatch(Command::..)` or `app.dispatch_named("Name")`.

## Tests

```bash
cargo test --workspace     # includes an end-to-end test on a virtual display
cargo clippy --workspace
```

The end-to-end and visual checks run the app on a virtual X display with
`tools/run-virtual.sh`; do not run it on your desktop session while tests
drive it. `tools/xdo.py` drives the window with xdotool, and the control
socket takes screenshots.

## Live debugging

Every run of `loom`, however it was started, can be inspected and driven
while it runs:

- **Control socket.** `$XDG_RUNTIME_DIR/3dloom/<pid>.sock` (the directory is
  mode 0700, the socket 0600; without `XDG_RUNTIME_DIR` a per-user directory
  in the temp dir). `current` in that directory links to the newest run's
  socket; sockets of runs that are gone are removed when the next run starts.
  It is never a network port; `LOOM_CONTROL_PORT=<port>` additionally opens
  the same protocol on 127.0.0.1 (and turns off the self-updater, as before).
  Protocol: one JSON object per line, one reply line each, e.g.
  `echo '{"cmd":"diagnostics"}' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/3dloom/current`.
- **Requests** besides every registry command (`{"cmd": "State"}`, ...),
  listed with their arguments in `loom_commands::live::app_commands`:
  `ping`, `screenshot {path}`, `diagnostics` (egui modifiers, pointer buttons,
  focus, popups and floating layers, drag / create / region / menu sessions,
  the camera navigator, the active viewport and its camera, tool, sub-object
  level, selection lock, keyboard override, snaps, preferences and the last
  200 raw input events), `reset_interaction` (clears stuck drags, region
  select, Alt-click, quad menu, egui popups and injected modifiers),
  `ui_hit {x, y}`, `ui_tree`, `input {events}` (pointer, key, text, scroll and
  held-modifier events, one per frame, the reply comes when all were
  delivered), `log_path`, `log_tail {lines}`.
- **Log.** `$XDG_STATE_HOME/3dloom/logs/loom-<date>-<pid>.log`
  (`~/.local/state/3dloom/logs`, `%LOCALAPPDATA%\3dloom\logs` on Windows),
  mode 0600, the newest 10 runs kept, a file restarted past 32 MiB. It has
  every command with its arguments and result (back-to-back repeats are
  counted, arguments and results are clipped), warnings and errors from all
  crates (wgpu, Bevy), the update checker, and panics with a backtrace.
  Standard error keeps `RUST_LOG` (default `warn`). Typed text and input
  events stay in memory (`diagnostics`) and are not written to the log.
- **MCP.** `loom-mcp` talks to the newest run by default (or the TCP port
  when `LOOM_CONTROL_PORT` is set). `loom_instances` lists the runs,
  `loom_attach_instance {pid}` picks one, `loom_logs {pid}` reads a log file
  directly (also of a run that crashed), and every request above is a tool
  (`loom_diagnostics`, `loom_input`, `loom_log_tail`, ...).

Tests that launch the app set `XDG_RUNTIME_DIR` and `XDG_STATE_HOME` to a
temporary directory so they never replace a person's `current` link or logs.

The `.max` import tests read sample files from `$LOOM_TEST_ASSETS` and skip
when it is not set.
