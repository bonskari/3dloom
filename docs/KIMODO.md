# Kimodo and LoomRig motion authoring

Motion > Kimodo generates editable animation through the configured API. Set the
API URL under Kimodo > Settings and use Test Connection. The address is a local
user preference; it is never stored in a scene. Generate submits a durable GPU
queue job, downloads its real BVH, imports a source clip, frames it and starts
playback. A completed job for a different scene waits for Import Result rather
than changing the new scene. Original BVH files are retained in `kimodo-motions/`
beside a saved scene, or under local configuration for an unsaved scene.

Motion > Import Animation accepts BVH and animated FBX. It preserves project FPS
and existing character animation, retiming incoming keys to the same time in
seconds. FBX reading and body-profile preparation run off the UI thread for the
file-picker workflow. Cancel or a changed scene invalidates the result before
commit. The synchronous command API retains its original response semantics.

## Importing an existing MetaHuman

In the FBX import options, enable MetaHuman Body Only to import the original
Body skeleton and skin, without the separate face rig or facial animation. Head
geometry and its body/neck influences remain. The profile remaps duplicate
component skeletons and facial influences with reference-frame compensation;
it does not generate new skin weights. Unsupported active facial morphs fail
before scene changes. Source FBX files are untouched. This profile is never
applied to an animation-source import.

Materials and actual texture references in the FBX import with the meshes.
Unreal procedural materials need export-side material baking; an FBX with names
but no texture references cannot reproduce them. Groom strands and Unreal-only
shader behavior are not represented by an ordinary skeletal FBX.

## Character controls

Under Motion > LoomRig > Create Character Rig, choose the existing skeleton
root. Create LoomRig automatically characterizes MetaHuman body names. For
other names, expand Characterization and map root, pelvis, spine, head and
contiguous three-joint limbs. Limb chains can be added, removed and named. More
than one character can have its own rig in the same scene.

Select Master positions the whole character. FK Controls selects ordinary
animatable correction helpers for the original bones. Select Goal and Select
Pole expose limb endpoints and elbow/knee directions; goal rotation controls
the hand or foot orientation. Neutral controls follow original animation.
Auto Key records corrections without replacing imported bone tracks.

IK/FK blends between source/FK and goal control. HI and IK Limb solvers are
editable; switching matches the current visible pose. At full IK, world-space
goals provide planted feet. Key IK/FK creates an explicit transition key. The
LoomRig IK/FK Tracks lane shows blend curves and draggable keys; right-click a
key to delete it. Goal, pole and FK correction transforms use native keys and
the existing track tools.

Edit Characterization changes mapped joints while retaining matching control
nodes and their animation. A change that would discard authored controls or
replace the character root is rejected before mutation. Rig creation, mapping,
solver edits and keys are undoable and saved with the scene.

## Transferring and arranging motion

Choose a source under Source Motion, inspect or edit Retarget Mapping, then
Apply Motion to Character. Automatic Kimodo/MetaHuman mappings account for
reference-pose arm directions, palm frames, metacarpals, body proportions and
character placement. Original hierarchy, bone lengths and skin bindings stay
intact. Repeated imports use collision namespaces that preserve original joint
names, including numbered fingers and spine joints.

Animation Clips above the key bar are movable blocks backed by editable native
keys. Dragging a block moves its owned keys in one undo step. Right-click it to
set its In or Out at the current frame. Moving or trimming one character clip
preserves neighboring clips on the same bones. Source and character clips
remain separate. Rig correction and IK/FK tracks are independent of source-clip
placement.

Multiple character clips can be arranged on the same skeleton when their ranges
are disjoint. Move a new source clip to its desired time before applying it.
Overlapping/touching target ranges and unowned target keys are rejected rather
than overwritten. Gaps use native curve interpolation; this is not an
independent overlapping NLA/crossfade mixer.

## Preview performance

Preferences > Viewports exposes Texture Preview Size and Texture Preview Budget
(MiB), defaulting to 2048 pixels and 256 MiB. A bounded worker decodes previews;
aggregate visible texture variants determine the effective limit. Hidden and
obsolete previews are evicted. Cropped/procedural outputs respect the same cap.
Material Editor and Crop use a separate small preview budget and background
loader. Source images and crop coordinates are unchanged.

## Verification

Focused tests cover source units/FPS/Euler continuity, actual queued generation,
project changes during jobs, repeat imports, native clip sequencing/trim,
reference-pose retargeting, control evaluation, solver/pose matching, keyed
blends, deletion/merge/save/undo, and preview budgets/async loading. Opt-in tests
use actual MetaHuman FBX and Kimodo BVH paths from environment variables; private
assets and service addresses are not included as repository defaults.
