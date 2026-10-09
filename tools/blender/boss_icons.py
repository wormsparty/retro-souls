"""Portraits of the boss encounters for the "Choose the boss" menu (assets/ui/boss_<n>.png).

blender -b --factory-startup -P tools/blender/boss_icons.py

Each encounter (tools/blender/timings.json, `encounters`) is posed as in the arena (scale,
member offsets), seen three-quarter front, and rendered at 64×64 on a transparent background: big
pixels, like the rest of the interface.
"""

import math
import os
import sys

import bpy
from mathutils import Vector

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import MODELS, ROOT, load_timings, reset_scene  # noqa: E402

SIZE = 64
OUT = os.path.join(ROOT, "assets", "ui")


def portrait(index, members):
    sc = reset_scene()
    objs = []
    for m in members:
        before = set(bpy.data.objects)
        bpy.ops.import_scene.gltf(filepath=os.path.join(MODELS, m["model"] + ".glb"))
        new = [o for o in bpy.data.objects if o not in before]
        roots = [o for o in new if o.parent is None]
        # Game frame (x, z) → Blender (x, -y); models face -Y.
        ox, oz = m["offset"]
        for r in roots:
            r.scale = [m["scale"]] * 3
            r.location = (r.location.x - ox * 1.0, r.location.y - oz, r.location.z)
        objs += [o for o in new if o.type == "MESH"]
    # Idle pose (the first imported animation), without the puppet's cross and strings
    # in the frame.
    sc.frame_set(0)
    bpy.context.view_layer.update()
    framed = [o for o in objs if not o.name.startswith("control")]
    pts = [o.matrix_world @ Vector(c) for o in framed for c in o.bound_box]
    lo = Vector((min(p.x for p in pts), min(p.y for p in pts), min(p.z for p in pts)))
    hi = Vector((max(p.x for p in pts), max(p.y for p in pts), max(p.z for p in pts)))
    center = (lo + hi) / 2

    cam_data = bpy.data.cameras.new("cam")
    cam_data.type = "ORTHO"
    cam = bpy.data.objects.new("cam", cam_data)
    sc.collection.objects.link(cam)
    sc.camera = cam
    # Three-quarter front view, slightly from above.
    yaw, pitch = math.radians(28), math.radians(10)
    d = 40.0
    direction = Vector((math.sin(yaw) * math.cos(pitch), -math.cos(yaw) * math.cos(pitch), math.sin(pitch)))
    cam.location = center + direction * d
    cam.rotation_euler = (-direction).to_track_quat("-Z", "Y").to_euler()
    # Framing: the largest extent seen from the camera.
    right = Vector((math.cos(yaw), math.sin(yaw), 0))
    up = direction.cross(right).normalized() * -1
    xs = [(p - center).dot(right) for p in pts]
    ys = [(p - center).dot(up) for p in pts]
    cam_data.ortho_scale = max(max(xs) - min(xs), max(ys) - min(ys)) * 1.06
    cam.location += right * (max(xs) + min(xs)) / 2 + up * (max(ys) + min(ys)) / 2

    sc.render.engine = "BLENDER_WORKBENCH"
    sc.display.shading.light = "STUDIO"
    sc.display.shading.color_type = "TEXTURE"
    sc.display.shading.show_cavity = False
    sc.view_settings.view_transform = "Standard"
    sc.view_settings.exposure = 1.3
    sc.display.shading.show_object_outline = True
    sc.display.shading.object_outline_color = (0.02, 0.02, 0.02)
    sc.render.resolution_x = SIZE
    sc.render.resolution_y = SIZE
    sc.render.film_transparent = True
    sc.render.filter_size = 0.0
    sc.display.render_aa = "OFF"
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    sc.render.filepath = os.path.join(OUT, f"boss_{index}.png")
    bpy.ops.render.render(write_still=True)
    print(f"[assets] {sc.render.filepath}")


for i, members in enumerate(load_timings()["encounters"]):
    portrait(i, members)
