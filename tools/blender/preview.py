"""Rend une planche de poses pour vérifier les animations.

blender -b tools/blender/blend/player.blend -P tools/blender/preview.py -- out.png anim:frame anim:frame ...
"""
import math
import os
import sys

import bpy

argv = sys.argv[sys.argv.index("--") + 1:]
out, specs = argv[0], argv[1:]
sc = bpy.context.scene
objs = [o for o in sc.objects if o.animation_data]

cam_data = bpy.data.cameras.new("cam")
cam_data.type = "ORTHO"
cam = bpy.data.objects.new("cam", cam_data)
sc.collection.objects.link(cam)
sc.camera = cam
sun = bpy.data.objects.new("sun", bpy.data.lights.new("sun", "SUN"))
sun.rotation_euler = (math.radians(50), 0, math.radians(30))
sc.collection.objects.link(sun)
sc.render.engine = "BLENDER_WORKBENCH"
sc.display.shading.light = "STUDIO"
sc.display.shading.color_type = "TEXTURE"
sc.render.resolution_x = 320
sc.render.resolution_y = 320
sc.render.film_transparent = False
scale = float(os.environ.get("PREVIEW_SCALE", "2.6"))
cam_data.ortho_scale = scale
view = os.environ.get("PREVIEW_VIEW", "34")

weapon = os.environ.get("PREVIEW_WEAPON")
if weapon:
    path = os.path.join(os.path.dirname(os.path.abspath(__file__)), "blend", "weapons.blend")
    with bpy.data.libraries.load(path) as (src, dst):
        dst.objects = [weapon]
    w = dst.objects[0]
    sc.collection.objects.link(w)
    w.parent = sc.objects["grip_R"]
    w.location = (0, 0, 0)

tiles = []
for i, spec in enumerate(specs):
    name, frame = spec.split(":")
    act = bpy.data.actions.get(name)
    for o in objs:
        for tr in o.animation_data.nla_tracks:
            tr.mute = True
        o.animation_data.action = act
        slot = next((s for s in act.slots if s.identifier == "OB" + o.name), None)
        o.animation_data.action_slot = slot
    sc.frame_set(int(frame))
    h = scale * 0.42
    if view == "side":
        cam.location = (-6, 0, h)
        cam.rotation_euler = (math.radians(90), 0, math.radians(-90))
    elif view == "top":
        cam.location = (0, 0, 8)
        cam.rotation_euler = (0, 0, 0)
    else:
        cam.location = (-4.2, -4.2, h + 1.0)
        cam.rotation_euler = (math.radians(78), 0, math.radians(-45))
    path = f"{out}.{i}.png"
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    tiles.append(path)

# Assemble une planche horizontale.
imgs = [bpy.data.images.load(p) for p in tiles]
w, hgt = imgs[0].size
sheet = bpy.data.images.new("sheet", w * len(imgs), hgt)
px = [0.0] * (w * len(imgs) * hgt * 4)
for k, im in enumerate(imgs):
    src = list(im.pixels)
    for y in range(hgt):
        row = src[y * w * 4:(y + 1) * w * 4]
        start = (y * w * len(imgs) + k * w) * 4
        px[start:start + w * 4] = row
sheet.pixels.foreach_set(px)
sheet.filepath_raw = out
sheet.file_format = "PNG"
sheet.save()
for p in tiles:
    os.remove(p)
