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

The `.max` import tests read sample files from `$LOOM_TEST_ASSETS` and skip
when it is not set.
