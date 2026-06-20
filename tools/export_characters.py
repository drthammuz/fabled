"""Re-export Kenney character GLBs with FULL animation ranges.

The previous export baked only 2 frames per clip (scene range 1-2), leaving
characters frozen in place. This script imports the shared skeleton + mesh,
brings in the idle and run animations from their FBX files, retargets the
actions onto the main armature via NLA tracks, applies the per-class skin,
and exports each class GLB with each action baked over its OWN full range.

Run:
  blender --background --python tools/export_characters.py
"""

import bpy
import os
import math

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CHARS = os.path.join(ROOT, "assets", "characters")
MODEL_FBX = os.path.join(CHARS, "Model", "characterMedium.fbx")
IDLE_FBX = os.path.join(CHARS, "Animations", "idle.fbx")
RUN_FBX = os.path.join(CHARS, "Animations", "run.fbx")

# class output name -> skin png (relative to assets/characters/Skins)
CLASSES = [
    ("character_soldier", "criminalMaleA.png"),
    ("character_medic",   "skaterMaleA.png"),
    ("character_scout",   "skaterFemaleA.png"),
    ("character_tech",    "cyborgFemaleA.png"),
]


def clear_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    # Purge orphan actions/images from previous iterations.
    for block in (bpy.data.actions, bpy.data.images, bpy.data.armatures,
                  bpy.data.meshes, bpy.data.materials):
        for item in list(block):
            block.remove(item)


def objects_set():
    return set(bpy.data.objects.keys())


def import_fbx(path):
    """Import an FBX and return the list of newly-added objects."""
    before = objects_set()
    bpy.ops.import_scene.fbx(filepath=path, automatic_bone_orientation=True)
    after = objects_set()
    return [bpy.data.objects[n] for n in (after - before)]


def find_armature(objs):
    for o in objs:
        if o.type == 'ARMATURE':
            return o
    return None


def extract_action(new_action_names):
    """Pick the richest action among those just imported.

    Kenney anim FBX files contain a short 2-frame 'Targeting Pose' (set as the
    active action) AND the real multi-frame clip as a separate action. We must
    select the one with the most keyframes, not the active one.
    """
    best = None
    best_keys = -1
    for name in new_action_names:
        act = bpy.data.actions.get(name)
        if act is None:
            continue
        keys = max((len(fc.keyframe_points) for fc in act.fcurves), default=0)
        if keys > best_keys:
            best_keys = keys
            best = act
    return best


def build_class(out_name, skin_png):
    clear_scene()

    # 1) Main character: mesh + skeleton.
    main_objs = import_fbx(MODEL_FBX)
    main_arm = find_armature(main_objs)
    main_meshes = [o for o in main_objs if o.type == 'MESH']
    if main_arm is None:
        raise RuntimeError("no armature in characterMedium.fbx")

    # 2) Idle action — track which actions the import adds, pick the richest.
    actions_before = set(bpy.data.actions.keys())
    idle_objs = import_fbx(IDLE_FBX)
    idle_new = set(bpy.data.actions.keys()) - actions_before
    idle_action = extract_action(idle_new)
    for o in idle_objs:
        bpy.data.objects.remove(o, do_unlink=True)
    # Drop the leftover 2-frame pose actions we didn't pick.
    for name in idle_new:
        act = bpy.data.actions.get(name)
        if act is not None and act is not idle_action:
            bpy.data.actions.remove(act)

    # 3) Walk (run) action.
    actions_before = set(bpy.data.actions.keys())
    run_objs = import_fbx(RUN_FBX)
    run_new = set(bpy.data.actions.keys()) - actions_before
    walk_action = extract_action(run_new)
    for o in run_objs:
        bpy.data.objects.remove(o, do_unlink=True)
    for name in run_new:
        act = bpy.data.actions.get(name)
        if act is not None and act is not walk_action:
            bpy.data.actions.remove(act)

    if idle_action is None or walk_action is None:
        raise RuntimeError("missing idle/walk action after import")

    idle_action.name = "Idle"
    walk_action.name = "Walk"
    idle_action.use_fake_user = True
    walk_action.use_fake_user = True

    # 4) Push both actions to NLA tracks on the MAIN armature so the glTF
    #    exporter emits two named animations, each over its own full range.
    main_arm.animation_data_create()
    main_arm.animation_data.action = None
    for action, track_name in ((idle_action, "Idle"), (walk_action, "Walk")):
        track = main_arm.animation_data.nla_tracks.new()
        track.name = track_name
        start = int(action.frame_range[0])
        track.strips.new(track_name, start, action)

    # Scene range covers the longest action (belt + suspenders).
    last = int(max(idle_action.frame_range[1], walk_action.frame_range[1]))
    bpy.context.scene.frame_start = 0
    bpy.context.scene.frame_end = max(last, 2)

    # 5) Apply the class skin to the mesh material.
    skin_path = os.path.join(CHARS, "Skins", skin_png)
    img = bpy.data.images.load(skin_path)
    for mesh_obj in main_meshes:
        if not mesh_obj.data.materials:
            mat = bpy.data.materials.new(out_name + "_mat")
            mat.use_nodes = True
            mesh_obj.data.materials.append(mat)
        mat = mesh_obj.data.materials[0]
        mat.use_nodes = True
        nt = mat.node_tree
        bsdf = next((n for n in nt.nodes if n.type == 'BSDF_PRINCIPLED'), None)
        if bsdf is None:
            bsdf = nt.nodes.new("ShaderNodeBsdfPrincipled")
        tex = nt.nodes.new("ShaderNodeTexImage")
        tex.image = img
        nt.links.new(bsdf.inputs["Base Color"], tex.outputs["Color"])

    # 6) Export GLB with per-action ranges (NLA tracks), full bake.
    out_path = os.path.join(CHARS, out_name + ".glb")
    bpy.ops.export_scene.gltf(
        filepath=out_path,
        export_format='GLB',
        export_animation_mode='NLA_TRACKS',
        export_animations=True,
        export_frame_range=False,          # use NLA strip ranges, not scene
        export_bake_animation=True,
        export_anim_single_armature=True,
        export_yup=True,
        export_apply=False,
    )
    print("EXPORTED %s  idle=%s frames  walk=%s frames" % (
        out_name,
        int(idle_action.frame_range[1] - idle_action.frame_range[0]) + 1,
        int(walk_action.frame_range[1] - walk_action.frame_range[0]) + 1,
    ))


def main():
    for out_name, skin in CLASSES:
        build_class(out_name, skin)
    print("ALL CHARACTERS EXPORTED")


if __name__ == "__main__":
    main()
