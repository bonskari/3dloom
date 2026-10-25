# Kimodo motion in 3D Loom

Motion > Kimodo generates editable source-skeleton animation through the user's
Kimodo API. Enter the API base URL under Settings and use Test Connection. The
URL is stored only in local user preferences, never in a scene or source code.
A service exposing the gpu-arbiter durable queue and Kimodo `artifacts-v1`
downloads is required.

Enter a prompt and duration, then Generate. The app submits a queued Kimodo job,
shows its state without blocking the UI, downloads the actual BVH and imports
bones and linear animation keys. The result is selected, framed and played on
the timeline. The source BVH is kept in `kimodo-motions/` beside a saved scene;
an unsaved scene uses the local configuration directory. Save the scene to
preserve the editable animation itself.

Motion > Import Animation accepts BVH and FBX as separate source clips. It preserves the project FPS and existing character keys, retiming incoming keys to the same time in seconds. A source clip is framed and played after import. FBX imports use the first animated take.

Animation Clips above the key bar shows each source as a block. Dragging the block moves all its native keys in one undo step, extending the playback range as needed. Clips survive save/reload; deleting source bones updates membership, and deleting the complete source removes its block.

File > Import also supports Kimodo BVH directly. Import preserves channel order,
root motion, skeleton hierarchy, frame rate and frame count, converting Y-up
centimeters once into the scene's Z-up system units. The import is undoable.

## Current scope

This is source-motion generation and import. It does not yet retarget motion
automatically onto a selected MetaHuman or create a complete character-control
rig. No alternate skeleton, new skin weights or facial controls are generated.

## LoomRig direction

LoomRig will be a Biped-style control system layered over existing skeletons,
retaining original bone controllers, hierarchy and skin bindings. A MetaHuman
body-only characterization is the first profile, with editable mappings/chains
for other skeletons. Controls, IK/FK, pole directions and solver settings must
remain editable; changing the system must match the visible pose atomically
and retain the original motion. Matching one frame does not promise identical
motion throughout a clip when the solver changes.

The basic source clip timeline is implemented. The dedicated LoomRig timeline will add trimming, correction/control tracks and visible IK/FK and foot-contact transitions. Retargeting, blending and those character controls remain pending; current clips are movable groups of editable native keys.

## Verification

Focused tests cover BVH hierarchy/channel order/units/FPS, real Kimodo motion,
Euler continuity, invalid-import atomicity, undo, save/reload, local-only API
settings and Motion-panel availability without an object selection. An opt-in
live test exercises generation, download, import and playback through the real
configured GPU queue using `LOOM_KIMODO_API_URL`. Real source files and service
addresses are supplied locally, not included as project defaults.

Background jobs keep their original project identity and output folder. If a project changes while a job runs, its result is saved and waits for explicit Import Result instead of modifying the new scene. New, Reset and Open invalidate the previous project identity.
