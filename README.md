# 3dLoom

A standalone 3D modelling application written in Rust, built for game
asset work: polygon modelling, a modifier stack, UV unwrapping, rigging,
skinning and keyframe animation, with FBX and OBJ in and out.

The interface and workflow aim to feel familiar to users of 3ds Max: the
viewport layout, command panel, modifier stack, quad menus and default
shortcuts follow the same conventions.

Every action is a command in one registry, so everything the UI does can
also be done from a script, the built-in control socket or an MCP client.

## Features

- **Viewports:** four-view layout (14 layouts), orthographic and perspective
  views, wireframe / shaded / edged faces, ViewCube, viewport clipping,
  quad menus, transform gizmos, snaps, grid.
- **Modelling:** standard primitives, an editable polygon object
  (vertex / edge / border / polygon / element levels, soft selection, extrude,
  bevel, chamfer, bridge, connect, cut, loops and rings, smoothing groups,
  material ids, subdivision surfaces, paint deformation), splines and shape
  tools, booleans and lofts.
- **Modifier stack:** bend, twist, taper, FFD, TurboSmooth, MeshSmooth, shell,
  symmetry, edit poly, UVW map, unwrap UVW, skin, morpher, point cache and many
  more; instanced modifiers, collapse, show end result.
- **UVs:** map channels on every edit, projections, the Edit UVWs editor
  (relax, pack, flatten / unfold / pelt mapping, seams, stitch).
- **Rigging and animation:** bones, IK solvers, skin with envelopes and weight
  tools, auto key and set key, controllers and constraints, a curve editor and
  dope sheet, `.lxaf` animation files.
- **Files:** the documented `.loom` scene format ([docs/LOOM-FORMAT.md](docs/LOOM-FORMAT.md)),
  FBX import and export (meshes, rigs, skin weights, blend shapes, animation),
  OBJ import and export, import of `.max` scene files (nodes, hierarchy,
  editable poly / mesh geometry with UVs).

## Build and run

Needs a recent stable Rust toolchain.

```bash
cargo run -p loom_app --release
```

Linux and Windows are built for every commit on `main`; the newest builds are
on the [nightly release](../../releases/tag/nightly).

Cross-compiling the Windows build on Linux:

```bash
rustup target add x86_64-pc-windows-gnu
cargo build --release -p loom_app --bin loom --target x86_64-pc-windows-gnu
```

(needs the mingw-w64 toolchain: gcc, binutils, headers and CRT).

## Controls

| Input | Action |
| --- | --- |
| Q / W / E / R | Select, Move, Rotate, Scale |
| Middle drag | Pan |
| Alt + middle or left drag | Orbit (Alt + left click removes from the selection) |
| Ctrl + Alt + middle or left drag | Zoom |
| Wheel | Zoom |
| Right click | Quad menu |
| 1 – 5 | Sub-object levels |
| Z / Alt+Ctrl+Z | Zoom extents selected / zoom extents |
| Alt+W | Maximize viewport |
| F3 / F4 | Wireframe / edged faces |
| Ctrl+Z / Ctrl+Y | Undo / redo |

The full key map is in [`docs/reference/`](docs/reference/) as tab-separated
tables, one per shortcut group.

## Automation

- `LOOM_CONTROL_PORT=7833 loom` opens a control socket on 127.0.0.1: one JSON
  object per line, `{"cmd": "<command name>", "args": {...}}`, plus `ping` and
  `{"cmd": "screenshot", "path": "..."}`.
- `cargo run -p loom_mcp` is a stdio MCP server; its tools are generated from
  the command registry, one per command.
- `tools/run-virtual.sh [port]` runs the app on a virtual X display (Xvnc or
  Xvfb), for automated and visual tests.

## Architecture

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and
[CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT, see [LICENSE](LICENSE).

3ds Max is a trademark of Autodesk, Inc. 3dLoom is an independent project and
is not affiliated with or endorsed by Autodesk.
