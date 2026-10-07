"""Génère assets/models/arena.glb : cour circulaire d'un théâtre forain en ruine, et le
couloir d'accès (au sud) au bout duquel se trouve la lanterne du checkpoint.

Les piliers, le couloir et le checkpoint sont lus dans assets/config/arena.ron (mêmes
positions que les collisions). Les empties « light_* » indiquent au jeu où placer les
lumières (« light_checkpoint » : la lanterne).
"""

import math
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *  # noqa: E402,F403

reset_scene()
ron = open(os.path.join(ROOT, "assets", "config", "arena.ron")).read()
RADIUS = float(re.search(r"radius:\s*([\d.]+)", ron).group(1))
pillars_src = ron[ron.index("pillars"):]
pillars_src = pillars_src[:pillars_src.index("]")]
PILLARS = [tuple(float(v) for v in m) for m in re.findall(r"\(\s*(-?[\d.]+),\s*(-?[\d.]+),\s*(-?[\d.]+)\s*\)", pillars_src)]
CORRIDOR_HW = float(re.search(r"corridor_half_width:\s*([\d.]+)", ron).group(1))
# Le repère jeu (x, z) correspond à (x, -y) dans Blender : le couloir part vers +y.
CORRIDOR_END = -float(re.search(r"corridor_end:\s*(-?[\d.]+)", ron).group(1))
_cp = re.search(r"checkpoint:\s*\(\s*(-?[\d.]+),\s*(-?[\d.]+)\s*\)", ron)
CHECKPOINT = (float(_cp.group(1)), -float(_cp.group(2)))

FLOOR = material("a_floor", tex=tex_stone(seed=41))
BRICK = material("a_brick", tex=tex_brick(seed=42))
STONE = material("a_stone", tex=tex_stone(seed=43, base=(0.5, 0.48, 0.44)))
DARKSTONE = material("a_darkstone", tex=tex_noise((0.25, 0.24, 0.24), 0.4, seed=44))
WOOD = material("a_wood", tex=tex_planks((0.4, 0.28, 0.17), seed=45))
CANOPY = material("a_canopy", tex=tex_stripes((0.55, 0.12, 0.1), (0.8, 0.72, 0.55), n=8, seed=46))
GOLD = material("a_gold", (0.7, 0.52, 0.22))
FIRE = material("a_fire", (1.0, 0.6, 0.2), emissive=(1.0, 0.55, 0.15))
IRON = material("a_iron", (0.2, 0.19, 0.2))

sc = bpy.context.scene


def obj(name, build):
    mb = MeshBuilder()
    build(mb)
    o = bpy.data.objects.new(name, mb.finish(name + "_mesh"))
    sc.collection.objects.link(o)
    return o


def ring_quads(mb, r0, r1, z, mat, sides, uv=(1, 1)):
    for i in range(sides):
        a0, a1 = 2 * math.pi * i / sides, 2 * math.pi * (i + 1) / sides
        v = [mb.bm.verts.new((r * math.cos(a), r * math.sin(a), z)) for r, a in ((r0, a0), (r1, a0), (r1, a1), (r0, a1))]
        mb._face(v, mat, uv_scale=uv)


def floor(mb):
    # Sol en anneaux concentriques (dalles), légèrement subdivisé pour limiter la déformation affine.
    rings = [0, 1.5, 3, 4.5, 6, 7.5, 9, 10.5, 12, 13.5, 15, RADIUS + 1.5]
    for r0, r1 in zip(rings, rings[1:]):
        sides = 48  # même découpage partout : pas de fissure entre anneaux
        if r0 == 0:
            for i in range(sides):
                a0, a1 = 2 * math.pi * i / sides, 2 * math.pi * (i + 1) / sides
                v = [mb.bm.verts.new((0, 0, 0)), mb.bm.verts.new((r1 * math.cos(a0), r1 * math.sin(a0), 0)),
                     mb.bm.verts.new((r1 * math.cos(a1), r1 * math.sin(a1), 0))]
                mb._face(v, FLOOR, uvs=[(0.5, 0), (0, 1), (1, 1)])
        else:
            ring_quads(mb, r0, r1, 0, FLOOR, sides)
    # Rosace centrale.
    ring_quads(mb, 2.2, 2.6, 0.04, DARKSTONE, 24)


GATE_SEGMENT = 7  # segment de mur centré sur +y (ouverture vers le couloir)


def wall(mb):
    sides = 32
    r_in, r_out, h = RADIUS + 0.5, RADIUS + 1.3, 5.0
    for i in range(sides):
        # Décalé d'un demi-segment pour qu'un segment soit centré sur l'axe du couloir.
        a0, a1 = 2 * math.pi * (i + 0.5) / sides, 2 * math.pi * (i + 1.5) / sides
        if i == GATE_SEGMENT:
            continue
        c0, s0, c1, s1 = math.cos(a0), math.sin(a0), math.cos(a1), math.sin(a1)
        hh = h - (1.6 if i % 5 == 2 else 0) - (0.7 if i % 3 == 0 else 0)  # créneaux en ruine
        vi = [mb.bm.verts.new(p) for p in ((r_in * c1, r_in * s1, 0), (r_in * c0, r_in * s0, 0),
                                           (r_in * c0, r_in * s0, hh), (r_in * c1, r_in * s1, hh))]
        mb._face(vi, BRICK, uv_scale=(1, 2))
        vo = [mb.bm.verts.new(p) for p in ((r_out * c0, r_out * s0, 0), (r_out * c1, r_out * s1, 0),
                                           (r_out * c1, r_out * s1, hh), (r_out * c0, r_out * s0, hh))]
        mb._face(vo, BRICK, uv_scale=(1, 2))
        vt = [mb.bm.verts.new(p) for p in ((r_in * c0, r_in * s0, hh), (r_out * c0, r_out * s0, hh),
                                           (r_out * c1, r_out * s1, hh), (r_in * c1, r_in * s1, hh))]
        mb._face(vt, STONE)
        # Contreforts (pas contre l'ouverture).
        if i % 4 == 0 and i not in (GATE_SEGMENT, GATE_SEGMENT + 1):
            mb.box((r_in * c0 - 0.3 * c0, r_in * s0 - 0.3 * s0, h / 2 - 0.4), (0.7, 0.7, h - 0.8), STONE)


def pillars(mb):
    for x, y, r in PILLARS:
        # Le repère jeu (x, z) correspond à (x, -y) dans Blender.
        by = -y
        mb.cylinder((x, by, 0.25), r * 1.25, 0.5, STONE, sides=8)
        mb.cylinder((x, by, 3.6), r, 6.2, STONE, sides=8, uv_scale=(2, 4))
        mb.cylinder((x, by, 6.85), r * 1.3, 0.3, STONE, sides=8)


def braziers(mb):
    lights = []
    for x, y, r in PILLARS[:4]:
        d = math.hypot(x, y)
        bx, by = x - x / d * (r + 0.9), -(y - y / d * (r + 0.9))
        mb.cylinder((bx, by, 0.5), 0.08, 1.0, IRON, sides=6)
        mb.cylinder((bx, by, 1.1), 0.35, 0.25, IRON, sides=8, radius_top=0.45)
        mb.box((bx, by, 1.32), (0.35, 0.35, 0.25), FIRE, taper=(0.4, 0.4))
        lights.append((bx, by, 1.6))
    return lights


def carousel(mb):
    # Manège en ruine au-delà du mur, derrière le boss.
    cx, cy = 0.0, -(RADIUS + 9.0)
    mb.cylinder((cx, cy, 0.3), 6.0, 0.6, WOOD, sides=12)
    mb.cylinder((cx, cy, 4.5), 0.4, 9.0, GOLD, sides=8)
    for i in range(12):
        a = 2 * math.pi * i / 12
        if i in (3, 4):
            continue  # poteaux cassés
        mb.cylinder((cx + 5.2 * math.cos(a), cy + 5.2 * math.sin(a), 3.2), 0.1, 5.6, GOLD, sides=6)
    mb.cylinder((cx, cy, 6.3), 6.4, 0.5, GOLD, sides=12)
    mb.cylinder((cx, cy, 8.0), 6.6, 3.0, CANOPY, sides=12, radius_top=0.3, uv_scale=(3, 1))
    mb.box((cx + 2.5, cy + 1.0, 1.1), (0.5, 1.4, 1.0), WOOD)  # cheval de bois renversé
    mb.box((cx - 3.0, cy - 0.5, 1.3), (0.4, 1.2, 0.9), WOOD)


def corridor(mb):
    """Couloir sans faces coplanaires (le vertex snapping les ferait clignoter) et en panneaux
    subdivisés (sinon la texture affine se déforme sur les longs murs)."""
    hw, y1, h = CORRIDOR_HW, CORRIDOR_END, 4.6
    yp0, yp1 = RADIUS + 0.4, RADIUS + 1.5  # porche
    # Porche : deux piliers (en saillie de 15 cm dans le couloir) et un linteau.
    for sx in (-1, 1):
        mb.slab((sx * (hw + 0.25), (yp0 + yp1) / 2, 2.9), (0.8, yp1 - yp0, 5.8), STONE)
    mb.slab((0, (yp0 + yp1) / 2, 5.2), (2 * hw + 1.5, yp1 - yp0 + 0.1, 0.8), STONE)
    mb.box((0, (yp0 + yp1) / 2, 6.01), (2 * hw + 0.6, 0.9, 0.8), DARKSTONE, taper=(0.5, 1.0))
    # Sol : il reprend exactement le bord du sol de l'arène (polygone à 48 côtés, dont un
    # sommet est sur l'axe du couloir) au lieu de le chevaucher.
    r = RADIUS + 1.5
    a = 2 * math.pi * 11 / 48
    ex, ey = r * math.cos(a), r * math.sin(a)
    start = lambda x: r - (r - ey) * abs(x) / ex
    cols = [-hw + 2 * hw * i / 4 for i in range(5)]
    n = math.ceil((y1 - r))
    rows = [[mb.bm.verts.new((x, start(x) + (y1 - start(x)) * j / n, 0)) for x in cols] for j in range(n + 1)]
    for j in range(n):
        for i in range(4):
            q = (rows[j][i], rows[j][i + 1], rows[j + 1][i + 1], rows[j + 1][i])
            mb._face(q, FLOOR, uvs=[(v.co.x / 2, v.co.y / 2) for v in q])
    # Murs de brique (du porche au mur du fond), piliers engagés et poutres tous les 4 m.
    for sx in (-1, 1):
        mb.slab((sx * (hw + 0.3), (yp1 + y1) / 2, h / 2), (0.6, y1 - yp1, h), BRICK, tile=2.4,
                skip=("bottom", "-y", "+y"))
        for k in range(1, int((y1 - yp0) / 4) + 1):
            y = yp0 + k * 4.0 - 1.0
            mb.slab((sx * (hw - 0.05), y, (h + 0.1) / 2), (0.3, 0.5, h + 0.1), STONE, cell=0.8)
    for k in range(1, int((y1 - yp0) / 4) + 1):
        mb.slab((0, yp0 + k * 4.0 - 1.0, h - 0.2), (2 * hw + 0.6, 0.35, 0.35), WOOD, cell=0.8)
    mb.slab((0, y1 + 0.3, h / 2), (2 * hw + 1.2, 0.6, h), BRICK, tile=2.4)


def checkpoint(mb):
    """Lanterne sur un socle de pierre : le point de repos."""
    x, y = CHECKPOINT
    mb.cylinder((x, y, 0.15), 0.55, 0.3, STONE, sides=8)
    mb.cylinder((x, y, 0.45), 0.4, 0.3, DARKSTONE, sides=8, radius_top=0.3)
    mb.cylinder((x, y, 1.15), 0.07, 1.1, IRON, sides=6)
    mb.box((x, y, 1.72), (0.3, 0.3, 0.05), IRON)
    mb.box((x, y, 1.9), (0.22, 0.22, 0.32), FIRE)
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        mb.box((x + sx * 0.12, y + sy * 0.12, 1.9), (0.04, 0.04, 0.36), IRON)
    mb.box((x, y, 2.1), (0.34, 0.34, 0.12), IRON, taper=(0.3, 0.3))
    mb.cylinder((x, y, 2.23), 0.04, 0.12, IRON, sides=6)
    return (x, y, 1.95)


def skyline(mb):
    for i in range(20):
        a = 2 * math.pi * (i + 0.3) / 20
        d = RADIUS + 16 + (i * 7 % 5) * 2.5
        if abs(math.cos(a)) < 0.4 and math.sin(a) < 0:
            continue  # laisse voir le manège
        if abs(d * math.cos(a)) < 7 + CORRIDOR_HW and d * math.sin(a) > RADIUS:
            continue  # ne pas empiéter sur le couloir
        h = 8 + (i * 13 % 7) * 2.0
        mb.box((d * math.cos(a), d * math.sin(a), h / 2), (6, 5, h), DARKSTONE, taper=(0.9, 0.9), uv_scale=(2, 3))
        mb.box((d * math.cos(a), d * math.sin(a), h + 1.2), (6.2, 5.2, 2.4), DARKSTONE, taper=(0.1, 0.9))


obj("floor", floor)
obj("wall", wall)
obj("pillars", pillars)
mb = MeshBuilder()
lights = braziers(mb)
o = bpy.data.objects.new("braziers", mb.finish("braziers_mesh"))
sc.collection.objects.link(o)
obj("carousel", carousel)
obj("skyline", skyline)
obj("corridor", corridor)
mb = MeshBuilder()
cp_light = checkpoint(mb)
o = bpy.data.objects.new("checkpoint", mb.finish("checkpoint_mesh"))
sc.collection.objects.link(o)
for name, p in [(f"light_{i}", p) for i, p in enumerate(lights)] + [("light_checkpoint", cp_light)]:
    e = bpy.data.objects.new(name, None)
    e.location = p
    sc.collection.objects.link(e)
export("arena")
