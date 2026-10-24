# Architecture

A Cargo workspace; each crate owns one concern, and lower crates never know
about higher ones.

```
loom_app       →  loom_ui, loom_commands, loom_mesh
loom_mcp       →  loom_commands
loom_ui        →  loom_commands, loom_render, loom_scene, loom_shape, loom_input, loom_mesh
loom_commands  →  loom_scene, loom_render, loom_fbx, loom_max, loom_shape, loom_input, loom_mesh
loom_render    →  loom_input, loom_mesh
loom_scene     →  loom_shape, loom_mesh
loom_fbx, loom_max, loom_shape  →  loom_mesh
loom_input, loom_mesh           →  no loom dependencies
```

| Crate | Role |
| --- | --- |
| `loom_mesh` | Half-edge mesh, editing operations, primitives, UV projections. Pure, testable without a GPU. |
| `loom_shape` | Splines and shapes: the line tool, parametric shapes, spline editing. |
| `loom_scene` | Scene graph: nodes, animatable `Param<T>`, transforms, the modifier stack, `Change` and the undo stack. |
| `loom_input` | Input events, the orbit camera and viewport navigation. |
| `loom_commands` | The `Command` enum and registry, `execute()`, `AppState`, the key map, file formats. |
| `loom_render` | wgpu pipelines for meshes, edges and the grid; picking helpers. |
| `loom_ui` | The eframe / egui application: viewports, panels, dialogs, menus. |
| `loom_fbx` | FBX import (via ufbx) and a binary FBX 7.4 writer. |
| `loom_max` | A reader for `.max` scene files (compound file, chunk tree, geometry). |
| `loom_app` | The `loom` binary and the control socket. |
| `loom_mcp` | The `loom-mcp` stdio MCP server, generated from the registry. |

## Conventions

- Modelling operations are pure functions over a mesh that return a `Change`
  or a new mesh; they never touch the GPU or the UI.
- The UI never edits the scene: it dispatches commands.
- Viewport navigation: middle drag pans, Alt + middle (or left) drag orbits,
  Ctrl + Alt drag zooms.
