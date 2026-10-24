import bpy, math, sys
out = sys.argv[sys.argv.index("--") + 1]
bpy.ops.wm.read_factory_settings(use_empty=True)

def sel(o):
    bpy.ops.object.select_all(action='DESELECT')
    o.select_set(True)
    bpy.context.view_layer.objects.active = o

# Smooth sides, flat caps (sharp edges at 30 degrees).
bpy.ops.mesh.primitive_cylinder_add(vertices=16, radius=1, depth=2, location=(0, 0, 0))
cyl = bpy.context.active_object
cyl.name = "Cylinder"
sel(cyl)
bpy.ops.object.shade_smooth_by_angle(angle=math.radians(30))

# Four-sided pyramid, all edges sharp: opposite sides meet only at the apex.
bpy.ops.mesh.primitive_cone_add(vertices=4, radius1=1, depth=1.5, location=(3, 0, 0))
pyr = bpy.context.active_object
pyr.name = "Pyramid"
sel(pyr)
bpy.ops.object.shade_smooth_by_angle(angle=math.radians(30))

# Flat-shaded cube (faces not smooth).
bpy.ops.mesh.primitive_cube_add(size=1.5, location=(-3, 0, 0))
cube = bpy.context.active_object
cube.name = "FlatCube"
sel(cube)
bpy.ops.object.shade_flat()

# A smooth sphere: one smooth region.
bpy.ops.mesh.primitive_uv_sphere_add(segments=12, ring_count=6, radius=1, location=(0, 3, 0))
sph = bpy.context.active_object
sph.name = "Sphere"
sel(sph)
bpy.ops.object.shade_smooth()

for mode in ["OFF", "FACE", "EDGE", "SMOOTH_GROUP"]:
    bpy.ops.export_scene.fbx(filepath=f"{out}/blender_smoothing_{mode.lower()}.fbx", mesh_smooth_type=mode, use_mesh_modifiers=True, add_leaf_bones=False, bake_anim=False)
