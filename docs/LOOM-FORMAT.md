# The `.loom` file format

A `.loom` file is one UTF-8 JSON document. It is written pretty-printed so
it diffs in version control and can be read by any JSON tool; `loom_mcp`
reads it without the GUI (`loom_read_file`).

## Top level

| key        | type    | meaning |
|------------|---------|---------|
| `format`   | integer | Format number. This program writes `1`. A file whose `format` is **greater** than the program's is refused with a message naming both numbers. A file with a smaller number is read; missing keys take their defaults. |
| `app`      | string  | Program and version that wrote the file (`3dloom <version>`). Informational. |
| `scene`    | object  | The scene graph: `loom_scene::Scene`. |
| `settings` | object  | What the scene file keeps besides objects: `loom_commands::file::SceneSettings`. |

The Rust types are the schema: every field is serialised by serde with its
field name, enums as tagged objects or strings as serde derives them.

## `scene`

- `nodes`: map from node id to node. A node has `id`, `name`, `parent`,
  `transform` (position, rotation quaternion, scale, each a controllable
  `Param`), `object_offset`, `object` (the base object: every
  `ObjectKind` — primitives with their parameters, `EditablePoly` with the
  full half-edge mesh, smoothing groups, material ids, vertex colours,
  soft-selection and paint settings, `EditableMesh` (the same, all
  triangles, with the mesh's `invisible` edge flags), `EditablePatch` (the
  patch lattice: vertices, handles, edges, tri / quad patches, View Steps,
  Show Interior Edges), `QuadPatch` / `TriPatch`, `EditableSpline` with its
  knots and tangents, the parametric shapes, `Boolean` / `ProBoolean` / `Loft` with
  their operand copies and parameters, `Camera`, `Light` (a standard light:
  type, target node, every rollout parameter, exclusion list), `Group`, `Bone` with its
  length / width / height / taper / fins / Bone Tools properties, `Dummy`,
  `Point`, `IkGoal`), `hidden`,
  `frozen`, `wire_color`, `locks`, `inherit`, `display` properties,
  `layer`, `modifiers` (the modifier stack, bottom first: each slot has
  the modifier with all its parameters and sub-object data — FFD control
  points, Edit Poly / Edit Mesh / Edit Patch / Edit Spline edits and
  snapshots, gizmo transforms —
  plus `enabled`, `enabled_in_viewport`, `name`, `sub_level`,
  `instance_of`), `active_entry`, `instance_of` and `materials` (material
  names by material id, `materials[i]` is id i+1; FBX import fills it,
  FBX export writes it). Rig modifiers keep their data in the slot: `Skin`
  (bones with bind matrices and envelopes, per-vertex weights, excluded
  vertices, skin pose, display / mirror settings, gizmos), `Morpher`
  (channels with targets and offsets), `SkinMorph` (bones and angle-keyed
  deltas), `SkinWrap` (bases and per-vertex face bindings).
  Phase 9 modifiers store their data the same way: `EditNormals` (normal
  groups, explicit directions, selection), `MeshSelect` / `VolSelect` /
  `SplineSelect` (stored selections, the Vol. Select gizmo),
  `FilletChamfer` (its operations on input vertices), `TrimExtend` (picked
  points, object space), `AffectRegion` (its end point as a gizmo),
  `PathDeform` / `CameraMap` / `MapScaler` / `PointCache` / `Subdivide`
  with `world: true` for the world-space versions, `CameraMap` the camera
  relative to the object at pick time, `HSDS` its levels (polygon ids and
  the counts they were made on; the finest-level cage is rebuilt, not
  saved), `PointCache` the `.pc2` file path (the samples stay in the file),
  `SplineIkControl` its helper node ids. Edit Normals writes explicit
  normals into the mesh's reserved map channel 4294967294.
  Every mesh carries `maps`: map channel number → `{ verts: [[u, v, w]…], corners: [tv index per half-edge] }`
  (channel 1 is the default texture channel; the vertex colour is on the vertices, not here).
- Every animatable value is a `Param`: `{ "controller": … }` where the
  controller is `{"Constant": value}`, `{"Keys": {ty, tracks, rest, linked}}`
  (one float track per component — Position XYZ / Euler XYZ style, or
  `linked` for Bezier / TCB / Linear Position, Rotation, Scale; each track
  has `interp` Bezier / Tcb / Linear, `keys` with `time` (frames), `value`,
  `in_type` / `out_type` (Auto, Smooth, Linear, Step, Fast, Slow, Custom),
  `in_tan` / `out_tan` (value per frame), TCB `tension` / `continuity` /
  `bias` / `ease_to` / `ease_from` (0–50), `broken`, `constant_velocity`,
  and `before` / `after` out-of-range types Constant, Cycle, Loop,
  PingPong, Linear, RelativeRepeat; `rest` holds the value of a component
  without keys), `{"Noise": {seed, frequency, fractal, roughness, ramp_in,
  ramp_out, end, strength}}` or `{"List": [{name, weight (percent),
  controller}]}`. Rotation stays Euler XYZ degrees.
- A node also carries `constraints` (`position`: `Path` with targets,
  `% Along Path` param and Follow / Bank / … options, `Position` with
  weighted targets and initial offset, or `Attachment` with face and
  barycentric A / B; `rotation`: `LookAt` (targets, axis, flip, upnode,
  offset) or `Orientation`; `link`: `[frame, parent or null]` switches)
  and `visibility` (an optional Param, 0–1), `render` (Object Properties >
  Rendering Control: `renderable`, `inherit_visibility`, `visible_to_camera`,
  `receive_shadows`, `cast_shadows`) and `user_props` (User Defined text).
- `next_id`: the next free node id.
- `track_data`: Track View `note_tracks` (node, optional track, keys with
  time / text / locked), `track_sets` (name → track keys `{node, owner,
  pointer}`) and per-track `flags` (`not_keyable`, `locked`).
- `ik_chains`: the Hierarchy > IK chains (`id`, `name`, `solver` HI / Limb /
  Spline, `start`, `end`, `goal` node ids, `enabled`, `swivel_deg`,
  `threshold`, `iterations`, `spline`, `ik_for_fk`). Solved bone rotations
  are not stored; they are recomputed on load.
- `layers`, `current_layer`: the Layer Manager.
- `render`: Render Setup — `width`, `height`, `pixel_aspect`,
  `lock_image_aspect`, `lock_pixel_aspect`, `render_hidden`,
  `force_two_sided`, `atmospherics`, `effects`, `save_file`, `output_path`,
  Area to Render `area` (View / Selected / Region / Crop / Blowup) with the
  `region` and `blowup` rectangles (x0, y0, x1, y1 as fractions of the
  frame), and `bevy` (the renderer panel: `antialiasing`, `supersampling`,
  `shadows`, `shadow_map_size`). The rendered image itself is not stored.
  Older files kept `background` and `ambient` colours in `bevy`; they load
  into `environment`.
- `environment`: Rendering > Environment and Effects — `background`,
  `tint`, `level`, `ambient` (Params; colours sRGB 0–1), `background_map`,
  `use_map`, `map_mapping` (Screen / Spherical / Cylindrical / ShrinkWrap),
  `exposure` (`kind` None / Automatic / Linear / Logarithmic, `active`,
  `process_background`, `brightness`, `contrast`, `exposure_value`,
  `mid_tones`), `atmospheres` and `effects` (lists of `name`, `active` and
  a `kind`: Fog, VolumeFog, VolumeLight; Glow, Blur, BrightnessContrast,
  ColorBalance, FilmGrain, DepthOfField, each with its parameters).

## `settings`

`layout` (one of the 14 viewport layouts), `viewports` (view kind, orbit
camera target / distance / yaw / pitch, shading, edged faces, grid,
disabled, safe frames, FOV, scene or default lights and the default light
count, background image path, lock and Use Environment Background),
`active_viewport`, `units` (Units Setup), `named_sets` and
`named_sub_selections` (named selection sets), `selection`, `frame_range`,
`fps`, `current_frame`, `real_time`, `loop_playback`, `play_speed`,
`play_direction` (Time Configuration) and `default_tangent` (the
Default In / Out Tangents for new keys), and `display` (Display Color
`wireframe_color` / `shaded_color`, `hide_frozen`, `hide_bones`, the
Statistics panel `statistics`, `display_selected_edged`, the Safe Frames
panel `safe_frames`, and the Object Color dialog's `assign_random_colors`,
`current_color`, `custom_colors`), `scene_explorers` (every Scene
Explorer, open or closed: name, columns, sort, display-type filters,
options, collapsed rows; the most recent and the default setup) and
`schematic_views` (every Schematic View: name, Hierarchy / References
mode, options, shown entity types, free box positions, collapsed boxes,
pan and zoom).

Animation clips (Animation > Save / Load Animation) are their own JSON
files, `.lxaf`: `{format: "loom-animation", version, fps, range, nodes:
[{name, tracks: [{owner, pointer, name, ty, controller}]}]}`; tracks are
matched by node name and track pointer when loading.

## Not stored

The undo stack, the Hold buffer (its own file `autoback/loomhold.loom`),
open dialogs, the active tool, pick modes, preferences (program-wide, not
per scene) and the per-session caches.

## Writing

`Save` writes `<name>.loom.tmp` beside the target, syncs it to disk and
renames it over the target, so an interrupted save never truncates the
previous file.

## Compatibility

Unknown keys are ignored on read. A missing `settings` key takes a new
scene's value; a `viewports` list that does not fit `layout` is replaced
by the layout's defaults. Inside `scene`, node fields marked
`#[serde(default)]` in the Rust types may be omitted; the rest (ids,
names, transforms, objects) are required. A field whose meaning changes
bumps `format`.
