"""Génère les armes : assets/models/rapier.glb et greatsword.glb.

Origine = centre de la poignée (là où la main tient l'arme), lame vers l'avant (-Y Blender,
+Z dans Bevy).
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *  # noqa: E402,F403

reset_scene()
STEEL = material("steel", (0.78, 0.8, 0.86))
STEEL_DARK = material("steel_dark", (0.45, 0.47, 0.52))
BRASS = material("brass", (0.8, 0.62, 0.28))
GRIP = material("grip", tex=tex_noise((0.22, 0.13, 0.08), 0.5, seed=21))


def weapon(name, build):
    mb = MeshBuilder()
    build(mb)
    obj = bpy.data.objects.new(name, mb.finish(name + "_mesh"))
    bpy.context.scene.collection.objects.link(obj)
    return obj


def rapier(mb):
    mb.cylinder((0, 0.02, 0), 0.016, 0.15, GRIP, sides=6, axis="Y")
    mb.box((0, 0.11, 0), (0.035, 0.035, 0.035), BRASS)
    # Coquille + quillons.
    mb.cylinder((0, -0.07, 0), 0.055, 0.02, BRASS, sides=8, axis="Y", radius_top=0.035)
    mb.box((0, -0.08, 0), (0.2, 0.012, 0.012), BRASS)
    mb.seg((0.0, -0.05, 0.0), (0.0, -0.02, -0.07), 0.01, 0.01, BRASS)
    # Lame fine.
    mb.seg((0, -0.08, 0), (0, -1.08, 0), 0.035, 0.02, STEEL, taper=0.3)


def greatsword(mb):
    # Greatsword : ~1,75 m de lame, large et épaisse, longue poignée à deux mains.
    mb.cylinder((0, 0.14, 0), 0.024, 0.38, GRIP, sides=6, axis="Y")
    mb.box((0, 0.35, 0), (0.07, 0.07, 0.07), STEEL_DARK, taper=(0.7, 0.7))
    mb.box((0, -0.08, 0), (0.46, 0.06, 0.06), STEEL_DARK)
    mb.box((0, -0.08, 0), (0.1, 0.08, 0.08), BRASS)
    mb.box((-0.23, -0.08, 0), (0.05, 0.08, 0.08), STEEL_DARK)
    mb.box((0.23, -0.08, 0), (0.05, 0.08, 0.08), STEEL_DARK)
    # Ricasso puis lame large.
    mb.seg((0, -0.11, 0), (0, -0.3, 0), 0.085, 0.03, STEEL_DARK)
    mb.seg((0, -0.3, 0), (0, -1.62, 0), 0.13, 0.028, STEEL, taper=0.82)
    mb.seg((0, -1.62, 0), (0, -1.86, 0), 0.107, 0.024, STEEL, taper=0.08)
    mb.seg((0, -0.3, 0.0), (0, -1.45, 0.0), 0.03, 0.034, STEEL_DARK)


for name, build in (("rapier", rapier), ("greatsword", greatsword)):
    for o in list(bpy.context.scene.objects):
        o.select_set(False)
    obj = weapon(name, build)
    obj.select_set(True)
    export(name, selection=True, save_blend=False)
bpy.ops.wm.save_as_mainfile(filepath=os.path.join(BLEND, "weapons.blend"))
