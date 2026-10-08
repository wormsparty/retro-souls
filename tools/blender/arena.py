"""Génère assets/models/arena.glb : le décor complet.

- la cour circulaire d'un théâtre forain en ruine (l'arène du boss), posée sur un socle de
  roche qui s'enfonce dans le vide ;
- autour, des ruines suspendues au-dessus du noir : l'escalier, la place des Allumeurs, les
  ponts, plates-formes et passerelles du chemin, leur décor, et au loin dans le vide quelques
  réverbères perdus sur des rochers flottants.

Tout est lu dans tools/blender/timings.json (exporté de assets/config/arena.ron et
level.ron par `cargo run --bin export_timings`) : mêmes positions que les collisions.
Les empties « light_* » indiquent au jeu où placer les lumières (« light_checkpoint_<i> » :
lanternes, « light_lamp_<i> » : réverbères, les autres : braseros).

Repère : le jeu (x, y, z) correspond à (x, -z, y) dans Blender.
"""

import math
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *  # noqa: E402,F403

reset_scene()
DATA = load_timings()
ARENA, LEVEL = DATA["arena"], DATA["level"]
RADIUS = ARENA["radius"]
PILLARS = [tuple(p) for p in ARENA["pillars"]]
GATE_HW = ARENA["gate_half_width"]
rng = random.Random(1234)

FLOOR = material("a_floor", tex=tex_stone(seed=41))
BRICK = material("a_brick", tex=tex_brick(seed=42))
STONE = material("a_stone", tex=tex_stone(seed=43, base=(0.5, 0.48, 0.44)))
DARKSTONE = material("a_darkstone", tex=tex_noise((0.25, 0.24, 0.24), 0.4, seed=44))
ROCK = material("a_rock", tex=tex_noise((0.2, 0.18, 0.17), 0.5, seed=47, cells=(16, 8, 4)))
WOOD = material("a_wood", tex=tex_planks((0.4, 0.28, 0.17), seed=45))
PLANKS = material("a_planks", tex=tex_planks((0.33, 0.24, 0.16), seed=48))
CANOPY = material("a_canopy", tex=tex_stripes((0.55, 0.12, 0.1), (0.8, 0.72, 0.55), n=8, seed=46))
BOOTH = material("a_booth", tex=tex_stripes((0.2, 0.3, 0.45), (0.78, 0.72, 0.58), n=6, seed=49))
GOLD = material("a_gold", (0.7, 0.52, 0.22))
RED = material("a_red", (0.55, 0.1, 0.08))
CREAM = material("a_cream", (0.78, 0.72, 0.58))
FIRE = material("a_fire", (1.0, 0.6, 0.2), emissive=(1.0, 0.55, 0.15))
GLASS = material("a_lampglass", (1.0, 0.92, 0.65), emissive=(0.95, 0.85, 0.55))
IRON = material("a_iron", (0.2, 0.19, 0.2))
WATER = material("a_water", (0.16, 0.26, 0.32), emissive=(0.02, 0.04, 0.05))

sc = bpy.context.scene


def B(x, z, y=0.0):
    """Point du jeu (x, z) à la hauteur y → coordonnées Blender."""
    return (x, -z, y)


def obj(name, build, loc=(0, 0, 0), yaw=0.0):
    mb = MeshBuilder()
    build(mb)
    o = bpy.data.objects.new(name, mb.finish(name + "_mesh"))
    o.location = loc
    o.rotation_euler = (0, 0, yaw)
    sc.collection.objects.link(o)
    return o


LIGHTS = []


def light(name, p):
    LIGHTS.append((name, p))


# ----------------------------------------------------------------------------- sols (même logique que src/sim/world.rs)

def strip_geom(f):
    s = f["shape"]["Strip"]
    (x0, z0, y0), (x1, z1, y1) = s["from"], s["to"]
    return x0, z0, y0, x1, z1, y1, s["half_width"]


def floor_at(x, z, near=0.0):
    best = None
    pieces = [{"shape": {"Ellipse": {"center": [0, 0], "radii": [RADIUS, RADIUS], "y": 0.0}}}] + LEVEL["floors"]
    for f in pieces:
        sh = f["shape"]
        if "Ellipse" in sh:
            e = sh["Ellipse"]
            (cx, cz), (rx, rz) = e["center"], e["radii"]
            if ((x - cx) / rx) ** 2 + ((z - cz) / rz) ** 2 <= 1.0:
                y = e["y"]
            else:
                continue
        else:
            x0, z0, y0, x1, z1, y1, hw = strip_geom(f)
            dx, dz = x1 - x0, z1 - z0
            ln = math.hypot(dx, dz)
            ux, uz = dx / ln, dz / ln
            rx, rz = x - x0, z - z0
            a = rx * ux + rz * uz
            c = rx * uz - rz * ux
            if not (0 <= a <= ln and abs(c) <= hw):
                continue
            y = y0 + (y1 - y0) * a / ln
        if best is None or abs(y - near) < abs(best - near):
            best = y
    return best


def ellipses():
    for f in LEVEL["floors"]:
        if "Ellipse" in f["shape"]:
            e = f["shape"]["Ellipse"]
            yield e["center"], e["radii"], e["y"]


def inside_ellipse(x, z, margin=0.0):
    for (cx, cz), (rx, rz), y in ellipses():
        if ((x - cx) / (rx - margin)) ** 2 + ((z - cz) / (rz - margin)) ** 2 <= 1.0:
            return y
    return None


# ----------------------------------------------------------------------------- arène

def ring_quads(mb, r0, r1, z, mat, sides, uv=(1, 1), cx=0.0, cy=0.0, ry=None):
    k = 1.0 if ry is None else ry
    for i in range(sides):
        a0, a1 = 2 * math.pi * i / sides, 2 * math.pi * (i + 1) / sides
        v = [mb.bm.verts.new((cx + r * math.cos(a), cy + r * k * math.sin(a), z)) for r, a in ((r0, a0), (r1, a0), (r1, a1), (r0, a1))]
        mb._face(v, mat, uv_scale=uv)


def arena_floor(mb):
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


GATE_SEGMENT = 7  # segment de mur centré sur +y (ouverture vers l'escalier)


def wall(mb):
    sides = 32
    r_in, r_out, h = RADIUS + 0.5, RADIUS + 1.3, 5.0
    for i in range(sides):
        # Décalé d'un demi-segment pour qu'un segment soit centré sur l'axe de l'escalier.
        a0, a1 = 2 * math.pi * (i + 0.5) / sides, 2 * math.pi * (i + 1.5) / sides
        if i == GATE_SEGMENT:
            continue
        c0, s0, c1, s1 = math.cos(a0), math.sin(a0), math.cos(a1), math.sin(a1)
        hh = h - (1.6 if i % 5 == 2 else 0) - (0.7 if i % 3 == 0 else 0)  # créneaux en ruine
        vi = [mb.bm.verts.new(p) for p in ((r_in * c1, r_in * s1, 0), (r_in * c0, r_in * s0, 0),
                                           (r_in * c0, r_in * s0, hh), (r_in * c1, r_in * s1, hh))]
        mb._face(vi, BRICK, uv_scale=(1, 2))
        # Le parement extérieur descend jusque sous le sol (on le voit depuis la place).
        vo = [mb.bm.verts.new(p) for p in ((r_out * c0, r_out * s0, -1.5), (r_out * c1, r_out * s1, -1.5),
                                           (r_out * c1, r_out * s1, hh), (r_out * c0, r_out * s0, hh))]
        mb._face(vo, BRICK, uv_scale=(1, 2.6))
        vt = [mb.bm.verts.new(p) for p in ((r_in * c0, r_in * s0, hh), (r_out * c0, r_out * s0, hh),
                                           (r_out * c1, r_out * s1, hh), (r_in * c1, r_in * s1, hh))]
        mb._face(vt, STONE)
        # Contreforts (pas contre l'ouverture).
        if i % 4 == 0 and i not in (GATE_SEGMENT, GATE_SEGMENT + 1):
            mb.box((r_in * c0 - 0.3 * c0, r_in * s0 - 0.3 * s0, h / 2 - 0.4), (0.7, 0.7, h - 0.8), STONE)


def rock_cone(mb, cx, cy, top, rx, ry, depth, seed, sides=14, rings=4):
    """Socle de roche irrégulier sous une plate-forme : il se resserre et se perd dans le vide."""
    r = random.Random(seed)
    jag = [[0.82 + 0.3 * r.random() for _ in range(sides)] for _ in range(rings + 1)]
    levels = []
    for k in range(rings + 1):
        t = k / rings
        z = top - depth * t ** 0.8
        s = (1.0 - t) ** 1.3 * 0.95 + 0.04
        ring = []
        for i in range(sides):
            a = 2 * math.pi * (i + 0.5 * (k % 2)) / sides
            j = 1.0 if k == 0 else jag[k][i]
            ring.append(mb.bm.verts.new((cx + rx * s * j * math.cos(a), cy + ry * s * j * math.sin(a), z)))
        levels.append(ring)
    for k in range(rings):
        a, b = levels[k], levels[k + 1]
        for i in range(sides):
            j = (i + 1) % sides
            mb._face((a[j], a[i], b[i], b[j]), ROCK, uv_scale=(1.5, 2))
    mb._face(list(reversed(levels[-1])), ROCK)


def arena_base(mb):
    rock_cone(mb, 0, 0, -1.5, RADIUS + 1.3, RADIUS + 1.3, 26, seed=7, sides=24, rings=5)


def pillars(mb):
    for x, z, r in PILLARS:
        x, y, _ = B(x, z)
        mb.cylinder((x, y, 0.25), r * 1.25, 0.5, STONE, sides=8)
        mb.cylinder((x, y, 3.6), r, 6.2, STONE, sides=8, uv_scale=(2, 4))
        mb.cylinder((x, y, 6.85), r * 1.3, 0.3, STONE, sides=8)


def braziers(mb):
    for i, (x, z, r) in enumerate(PILLARS[:4]):
        d = math.hypot(x, z)
        bx, by, _ = B(x - x / d * (r + 0.9), z - z / d * (r + 0.9))
        mb.cylinder((bx, by, 0.5), 0.08, 1.0, IRON, sides=6)
        mb.cylinder((bx, by, 1.1), 0.35, 0.25, IRON, sides=8, radius_top=0.45)
        mb.box((bx, by, 1.32), (0.35, 0.35, 0.25), FIRE, taper=(0.4, 0.4))
        light(f"light_{i}", (bx, by, 1.6))


def carousel(mb):
    # Manège en ruine au-delà du mur, derrière le boss (au nord), sur son propre rocher.
    cx, cy = 0.0, -(RADIUS + 9.0)
    rock_cone(mb, cx, cy, 0.0, 7.0, 7.0, 16, seed=8)
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


def skyline(mb):
    """Silhouettes de bâtisses, au nord seulement (au sud, il n'y a que le vide), chacune sur
    une aiguille de roche."""
    for i in range(20):
        a = 2 * math.pi * (i + 0.3) / 20
        d = RADIUS + 16 + (i * 7 % 5) * 2.5
        x, y = d * math.cos(a), d * math.sin(a)
        if y > -6 or (abs(x) < 9 and y < 0):
            continue  # pas au sud, et laisse voir le manège
        h = 8 + (i * 13 % 7) * 2.0
        rock_cone(mb, x, y, -0.2, 4.2, 3.6, 18 + (i % 3) * 6, seed=100 + i, sides=8, rings=3)
        mb.box((x, y, h / 2), (6, 5, h), DARKSTONE, taper=(0.9, 0.9), uv_scale=(2, 3))
        mb.box((x, y, h + 1.2), (6.2, 5.2, 2.4), DARKSTONE, taper=(0.1, 0.9))


def porch_and_landing(mb):
    """Porche dans l'ouverture du mur, palier et départ de l'escalier."""
    hw = GATE_HW
    yp0, yp1 = RADIUS + 0.4, RADIUS + 1.5
    for sx in (-1, 1):
        mb.slab((sx * (hw + 0.25), (yp0 + yp1) / 2, 2.9), (0.8, yp1 - yp0, 5.8), STONE)
    mb.slab((0, (yp0 + yp1) / 2, 5.2), (2 * hw + 1.5, yp1 - yp0 + 0.1, 0.8), STONE)
    mb.box((0, (yp0 + yp1) / 2, 6.01), (2 * hw + 0.6, 0.9, 0.8), DARKSTONE, taper=(0.5, 1.0))
    # Sol du palier : il reprend exactement le bord du sol de l'arène (polygone à 48 côtés,
    # dont un sommet est sur l'axe) au lieu de le chevaucher.
    land = next(f for f in LEVEL["floors"] if f.get("arena"))
    y1 = -strip_geom(land)[4]
    r = RADIUS + 1.5
    a = 2 * math.pi * 11 / 48
    ex, ey = r * math.cos(a), r * math.sin(a)
    start = lambda x: r - (r - ey) * abs(x) / ex
    cols = [-hw + 2 * hw * i / 4 for i in range(5)]
    n = 2
    rows = [[mb.bm.verts.new((x, start(x) + (y1 - start(x)) * j / n, 0)) for x in cols] for j in range(n + 1)]
    for j in range(n):
        for i in range(4):
            q = (rows[j][i], rows[j][i + 1], rows[j + 1][i + 1], rows[j + 1][i])
            mb._face(q, FLOOR, uvs=[(v.co.x / 2, v.co.y / 2) for v in q])
    # Balustrades du palier.
    for sx in (-1, 1):
        mb.slab((sx * (hw + 0.2), (yp1 + y1) / 2, 0.5), (0.4, y1 - yp1, 1.0), STONE, skip=("bottom", "-y"))
    # Dessous du palier.
    mb.box((0, (yp1 + y1) / 2, -0.8), (2 * hw + 0.8, y1 - yp1, 1.6), ROCK)


# ----------------------------------------------------------------------------- chemins et plates-formes

def platform(mb, f, seed):
    e = f["shape"]["Ellipse"]
    (cx, cz), (rx, rz), y = e["center"], e["radii"], e["y"]
    bx, by, _ = B(cx, cz)
    k = rz / rx
    # Dessus en anneaux (pas de face trop grande : la texture affine se déformerait).
    sides = 40
    step = 1.6
    rings = [0.0]
    while rings[-1] + step < rx - 0.3:
        rings.append(rings[-1] + step)
    rings.append(rx)
    for r0, r1 in zip(rings, rings[1:]):
        if r0 == 0:
            for i in range(sides):
                a0, a1 = 2 * math.pi * i / sides, 2 * math.pi * (i + 1) / sides
                v = [mb.bm.verts.new((bx, by, y)),
                     mb.bm.verts.new((bx + r1 * math.cos(a0), by + r1 * k * math.sin(a0), y)),
                     mb.bm.verts.new((bx + r1 * math.cos(a1), by + r1 * k * math.sin(a1), y))]
                mb._face(v, FLOOR, uvs=[(0.5, 0), (0, 1), (1, 1)])
        else:
            ring_quads(mb, r0, r1, y, FLOOR, sides, cx=bx, cy=by, ry=k)
    # Tranche du dallage, puis le rocher dessous.
    for i in range(sides):
        a0, a1 = 2 * math.pi * i / sides, 2 * math.pi * (i + 1) / sides
        p = [(bx + rx * math.cos(a), by + rx * k * math.sin(a)) for a in (a0, a1)]
        v = [mb.bm.verts.new((p[1][0], p[1][1], y)), mb.bm.verts.new((p[0][0], p[0][1], y)),
             mb.bm.verts.new((p[0][0], p[0][1], y - 0.6)), mb.bm.verts.new((p[1][0], p[1][1], y - 0.6))]
        mb._face(v, STONE, uv_scale=(0.5, 0.3))
    rock_cone(mb, bx, by, y - 0.6, rx, rx * k, 6 + rx * 1.6, seed=seed, sides=16)
    # Bordure de pierres basses (pas une barrière : quelques pierres, avec des manques).
    r = random.Random(seed)
    for i in range(sides):
        if r.random() < 0.35:
            continue
        a = 2 * math.pi * (i + 0.5) / sides
        px, py = bx + (rx - 0.18) * math.cos(a), by + (rx - 0.18) * k * math.sin(a)
        # Pas de pierre là où un chemin part.
        gx, gz = px, -py
        if any(near_strip_end(g, gx, gz) for g in LEVEL["floors"] if "Strip" in g["shape"]):
            continue
        mb.box((px, py, y + 0.06), (0.32, 0.32, 0.12), STONE, taper=(0.8, 0.8))


def near_strip_end(f, x, z):
    x0, z0, _, x1, z1, _, hw = strip_geom(f)
    return min(math.hypot(x - x0, z - z0), math.hypot(x - x1, z - z1)) < hw + 1.2


def strip(mb, f, idx):
    x0, z0, y0, x1, z1, y1, hw = strip_geom(f)
    style, steps, walled = f.get("style", "Paved"), f.get("steps", 0), f.get("walled", False)
    dx, dz = x1 - x0, z1 - z0
    ln = math.hypot(dx, dz)
    ux, uz = dx / ln, dz / ln
    # Les bouts qui entrent dans une plate-forme sont rognés (on ne superpose pas deux sols)
    # et posés 2 cm plus bas que la plate-forme.
    def inside(t):
        return inside_ellipse(x0 + ux * t, z0 + uz * t) is not None
    a, b = 0.0, ln
    while a < ln and inside(a + 0.8):
        a += 0.1
    while b > 0 and inside(b - 0.8):
        b -= 0.1
    if b - a < 0.2:
        a, b = 0.0, ln
    sx, sz = uz, -ux  # côté (droite en regardant vers `to`)
    deck = 0.12 if style == "Planks" else 0.5
    mat_top = PLANKS if style == "Planks" else FLOOR
    mat_side = WOOD if style == "Planks" else STONE

    def P(t, side, dy=0.0):
        y = y0 + (y1 - y0) * t / ln
        return B(x0 + ux * t + sx * side, z0 + uz * t + sz * side, y + dy)

    if steps:
        # Marches : le dessus de chaque marche est à la hauteur de la pente en son milieu.
        n = steps
        for k in range(n):
            ta, tb = a + (b - a) * k / n, a + (b - a) * (k + 1) / n
            tm = (ta + tb) / 2
            ytop = y0 + (y1 - y0) * tm / ln
            lo = min(y0, y1) - 0.4
            corners = [B(x0 + ux * t + sx * s, z0 + uz * t + sz * s) for t, s in ((ta, -hw), (tb, -hw), (tb, hw), (ta, hw))]
            v_top = [mb.bm.verts.new((c[0], c[1], ytop)) for c in corners]
            v_bot = [mb.bm.verts.new((c[0], c[1], lo)) for c in corners]
            mb._face(v_top if is_ccw(corners) else list(reversed(v_top)), FLOOR, uv_scale=(1, 0.5))
            for i in range(4):
                j = (i + 1) % 4
                q = (v_bot[i], v_bot[j], v_top[j], v_top[i])
                mb._face(q if is_ccw(corners) else tuple(reversed(q)), STONE, uv_scale=(1, 0.3))
    else:
        # Dalles du tablier en bandes d'environ 1 m (texture affine contenue).
        n = max(1, math.ceil((b - a) / 1.0))
        cols = [-hw + 2 * hw * i / 3 for i in range(4)]
        rows = [[mb.bm.verts.new(P(a + (b - a) * j / n, c, -0.02)) for c in cols] for j in range(n + 1)]
        for j in range(n):
            for i in range(3):
                q = [rows[j][i], rows[j + 1][i], rows[j + 1][i + 1], rows[j][i + 1]]
                if not is_ccw([v.co for v in q]):
                    q.reverse()
                mb._face(q, mat_top, uvs=[(v.co.x / 2, v.co.y / 2) for v in q])
        # Tranches du tablier.
        for side in (-hw, hw):
            for j in range(n):
                ta, tb = a + (b - a) * j / n, a + (b - a) * (j + 1) / n
                q = [mb.bm.verts.new(P(ta, side, -0.02)), mb.bm.verts.new(P(tb, side, -0.02)),
                     mb.bm.verts.new(P(tb, side, -deck)), mb.bm.verts.new(P(ta, side, -deck))]
                if side < 0:
                    q.reverse()
                mb._face(q, mat_side, uv_scale=(1, 0.4))
        # Dessous.
        q = [mb.bm.verts.new(P(a, -hw, -deck)), mb.bm.verts.new(P(a, hw, -deck)),
             mb.bm.verts.new(P(b, hw, -deck)), mb.bm.verts.new(P(b, -hw, -deck))]
        if is_ccw([v.co for v in q]):
            q.reverse()
        mb._face(q, mat_side)
        if style == "Planks":
            # Planches transversales mal jointes, et quelques poutres qui pendent dessous.
            for k in range(int((b - a) / 0.9)):
                t = a + 0.45 + k * 0.9
                mb.seg(P(t, -hw - 0.1, -0.05), P(t, hw + 0.1, -0.05), 0.18, 0.06, WOOD)
            for t in (a + 0.6, b - 0.6):
                for side in (-hw, hw):
                    mb.seg(P(t, side, -0.1), P(t, side * 1.4, -3.0), 0.12, 0.12, WOOD)
        else:
            # Piles qui plongent dans le vide.
            k = max(1, int((b - a) / 6.0))
            for i in range(k):
                t = a + (b - a) * (i + 0.5) / k
                c = P(t, 0, -deck)
                mb.box((c[0], c[1], c[2] - 7.0), (2 * hw * 0.7, 1.2, 14.0), ROCK if style != "Bridge" else STONE,
                       taper=(0.5, 0.6))
                # Encorbellement sous le tablier.
                mb.box((c[0], c[1], c[2] - 0.5), (2 * hw + 0.2, 1.6, 1.0), STONE, taper=(1.15, 1.4))
            # Pierres d'arête, avec des manques.
            r = random.Random(idx * 31)
            for side in (-hw + 0.15, hw - 0.15):
                t = a + 0.5
                while t < b - 0.3:
                    if r.random() > 0.4:
                        c = P(t, side)
                        mb.box((c[0], c[1], c[2] + 0.04), (0.28, 0.28, 0.1), STONE, taper=(0.8, 0.8))
                    t += 0.9 + r.random() * 0.8
    if walled:
        # Balustrades de part et d'autre (escalier de l'arène).
        for side in (-hw - 0.2, hw + 0.2):
            n = max(1, steps or int(ln))
            for k in range(n):
                ta, tb = a + (b - a) * k / n, a + (b - a) * (k + 1) / n
                c0, c1 = P(ta, side), P(tb, side)
                bot, top = min(c0[2], c1[2]) - 0.1, max(c0[2], c1[2]) + 0.9
                mid = ((c0[0] + c1[0]) / 2, (c0[1] + c1[1]) / 2, (bot + top) / 2)
                length = math.hypot(c1[0] - c0[0], c1[1] - c0[1]) + 0.02
                obj(f"rail_{idx}_{k}_{side}", lambda m, l=length, h=top - bot: m.slab((0, 0, 0), (0.4, l, h), STONE),
                    loc=mid, yaw=math.atan2(-(c1[0] - c0[0]), c1[1] - c0[1]))
        # Dessous de l'escalier.
        lo = min(y0, y1)
        for t in (a + (b - a) * 0.3, a + (b - a) * 0.8):
            c = P(t, 0)
            mb.box((c[0], c[1], lo - 4.0), (2 * hw + 0.6, 1.4, 8.0 + (c[2] - lo)), ROCK, taper=(0.6, 0.8))


def is_ccw(pts):
    """Vrai si le polygone (vu de dessus, axe Z) tourne dans le sens direct."""
    s = 0.0
    for i in range(len(pts)):
        x0, y0 = pts[i][0], pts[i][1]
        x1, y1 = pts[(i + 1) % len(pts)][0], pts[(i + 1) % len(pts)][1]
        s += x0 * y1 - x1 * y0
    return s > 0


# ----------------------------------------------------------------------------- décor
# Repère local d'un décor : avant = -Y, droite = -X (comme les personnages), origine au sol.

def lamp(mb):
    mb.box((0, 0, 0.15), (0.32, 0.32, 0.3), STONE, taper=(0.8, 0.8))
    mb.cylinder((0, 0, 1.8), 0.06, 3.0, IRON, sides=6)
    mb.cylinder((0, 0, 3.32), 0.1, 0.06, IRON, sides=6)
    mb.box((0, 0, 3.52), (0.26, 0.26, 0.34), GLASS, taper=(1.15, 1.15))
    for sx, sy in ((-1, -1), (1, -1), (1, 1), (-1, 1)):
        mb.box((sx * 0.14, sy * 0.14, 3.52), (0.03, 0.03, 0.38), IRON)
    mb.box((0, 0, 3.76), (0.4, 0.4, 0.14), IRON, taper=(0.2, 0.2))


def dead_lamp(mb):
    mb.box((0, 0, 0.15), (0.32, 0.32, 0.3), STONE, taper=(0.8, 0.8))
    mb.seg((0, 0, 0.3), (0.0, -0.5, 2.2), 0.1, 0.1, IRON)
    mb.seg((0.0, -0.5, 2.2), (0.3, -1.1, 2.6), 0.09, 0.09, IRON)
    mb.box((0.35, -1.2, 2.45), (0.24, 0.24, 0.3), IRON, taper=(1.1, 1.1))


def tube_inside(mb, r, z0, z1, mat, sides):
    """Paroi intérieure d'un cylindre (faces tournées vers l'axe)."""
    for i in range(sides):
        a0, a1 = 2 * math.pi * i / sides, 2 * math.pi * (i + 1) / sides
        p = lambda a, z: mb.bm.verts.new((r * math.cos(a), r * math.sin(a), z))
        mb._face([p(a1, z0), p(a0, z0), p(a0, z1), p(a1, z1)], mat, uv_scale=(1, 0.3))


def disc(mb, r, z, mat, sides):
    """Disque horizontal tourné vers le haut, sans tranche (surface de l'eau : il entre un peu
    dans la paroi qui l'entoure, aucun bord ne la double)."""
    v = [mb.bm.verts.new((r * math.cos(2 * math.pi * i / sides), r * math.sin(2 * math.pi * i / sides), z)) for i in range(sides)]
    mb._face(v, mat)


# Hauteurs de la fontaine (le jeu y fait jaillir l'eau : voir FOUNTAIN_* dans src/fx.rs).
FOUNTAIN_WATER = 0.42
FOUNTAIN_BOWL = 1.93
FOUNTAIN_SPOUT = 2.2


def fountain(mb):
    # Bassin creux : paroi extérieure, margelle, paroi intérieure, et l'eau sous le bord (deux
    # surfaces à la même hauteur scintillaient).
    sides = 16
    mb.cylinder((0, 0, 0.28), 1.7, 0.56, STONE, sides=sides, caps=False)
    ring_quads(mb, 1.45, 1.7, 0.56, STONE, sides)
    tube_inside(mb, 1.45, 0.2, 0.56, STONE, sides)
    disc(mb, 1.5, FOUNTAIN_WATER, WATER, sides)
    # Colonne et vasque haute, creuse elle aussi : l'eau est bien sous la margelle et le haut de
    # la colonne bien sous l'eau (des faces à 1-2 cm l'une de l'autre se chevauchaient une fois
    # les sommets accrochés à la grille de l'écran, façon PS1). Un bec au milieu.
    mb.cylinder((0, 0, 1.1), 0.28, 1.3, STONE, sides=8, radius_top=0.22)
    mb.cylinder((0, 0, FOUNTAIN_BOWL - 0.13), 0.38, 0.26, STONE, sides=12, radius_top=0.8, caps=False)
    mb.cylinder((0, 0, FOUNTAIN_BOWL - 0.27), 0.38, 0.02, STONE, sides=12)  # dessous
    ring_quads(mb, 0.64, 0.8, FOUNTAIN_BOWL, STONE, 12)
    tube_inside(mb, 0.64, FOUNTAIN_BOWL - 0.2, FOUNTAIN_BOWL, STONE, 12)
    disc(mb, 0.68, FOUNTAIN_BOWL - 0.09, WATER, 12)
    mb.cylinder((0, 0, (FOUNTAIN_BOWL - 0.1 + FOUNTAIN_SPOUT) / 2), 0.09, FOUNTAIN_SPOUT - FOUNTAIN_BOWL + 0.1, STONE, sides=6, radius_top=0.06)
    mb.box((0.0, -0.9, FOUNTAIN_WATER + 0.06), (0.7, 0.35, 0.22), DARKSTONE)  # éclat tombé dans le bassin


def bench(mb):
    for sx in (-0.6, 0.6):
        mb.box((sx, 0, 0.2), (0.08, 0.4, 0.4), IRON)
    mb.box((0, 0, 0.44), (1.5, 0.42, 0.06), WOOD)
    mb.box((0, 0.2, 0.78), (1.5, 0.05, 0.36), WOOD)


def horse(mb):
    # Cheval de manège tombé sur le flanc.
    mb.box((0, 0, 0.3), (0.42, 1.25, 0.5), WOOD)
    mb.box((0, -0.75, 0.5), (0.3, 0.45, 0.32), WOOD, taper=(0.9, 0.8))
    mb.box((0, -1.0, 0.42), (0.24, 0.3, 0.22), WOOD)
    for sy in (-0.4, 0.4):
        mb.seg((0.12, sy, 0.2), (0.75, sy - 0.1, 0.12), 0.1, 0.1, WOOD)
        mb.seg((-0.12, sy, 0.2), (0.6, sy + 0.15, 0.05), 0.1, 0.1, WOOD)
    mb.box((0, 0, 0.58), (0.3, 0.5, 0.08), CANOPY)
    mb.seg((0.0, 0.3, 0.3), (-1.4, 0.9, 0.08), 0.07, 0.07, GOLD)


def booth(mb):
    mb.box((0, 0.1, 1.0), (1.5, 1.3, 2.0), WOOD)
    mb.box((0, -0.56, 1.15), (1.1, 0.06, 0.55), DARKSTONE)  # guichet
    mb.box((0, -0.62, 0.88), (1.2, 0.25, 0.06), WOOD)
    mb.box((0, -0.05, 2.25), (1.8, 1.8, 0.5), BOOTH, taper=(0.15, 0.15))
    mb.box((0, -0.86, 2.1), (1.7, 0.08, 0.25), BOOTH)


def column(h):
    def build(mb):
        mb.cylinder((0, 0, 0.12), 0.38, 0.24, STONE, sides=8)
        mb.cylinder((0, 0, 0.24 + h / 2), 0.26, h, STONE, sides=8, uv_scale=(1, 2))
    return build


def crates(mb):
    mb.box((0, 0, 0.3), (0.6, 0.6, 0.6), WOOD)
    mb.box((0.55, 0.25, 0.25), (0.5, 0.5, 0.5), WOOD)
    mb.box((0.1, 0.1, 0.82), (0.45, 0.45, 0.45), WOOD)


PROPS = {"Lamp": lamp, "DeadLamp": dead_lamp, "Fountain": fountain, "Bench": bench, "Horse": horse,
         "Booth": booth, "Crates": crates}


def props():
    lamp_i = 0
    for i, p in enumerate(LEVEL["props"]):
        x, z = p["pos"]
        y = floor_at(x, z) or 0.0
        yaw = math.radians(p.get("yaw", 0.0))
        kind = p["kind"]
        build = column(1.0 + (i * 7 % 5) * 0.55) if kind == "Column" else PROPS[kind]
        obj(f"prop_{i}_{kind}", build, loc=B(x, z, y), yaw=yaw)
        if kind == "Lamp":
            light(f"light_lamp_{lamp_i}", B(x, z, y + 3.5))
            lamp_i += 1


# Hauteur des braises du brasier (le jeu en fait monter braises et cendres : voir src/fx.rs).
COALS = 1.0


def checkpoint(mb):
    """Brasier du point de repos : socle de pierre, vasque de fer à griffes, et une vieille
    épée plantée dans les braises (en coordonnées locales ; les braises sont à part)."""
    mb.cylinder((0, 0, 0.12), 0.62, 0.24, STONE, sides=8)
    mb.cylinder((0, 0, 0.36), 0.46, 0.24, DARKSTONE, sides=8, radius_top=0.36)
    mb.cylinder((0, 0, 0.56), 0.16, 0.16, IRON, sides=6)
    mb.cylinder((0, 0, 0.79), 0.3, 0.3, IRON, sides=8, radius_top=0.62)
    for k in range(6):
        a = 2 * math.pi * (k + 0.5) / 6
        c, s = math.cos(a), math.sin(a)
        mb.seg((0.56 * c, 0.56 * s, 0.9), (0.74 * c, 0.74 * s, 1.22), 0.06, 0.06, IRON, taper=0.4)
    # Épée plantée, légèrement penchée : lame, garde, poignée, pommeau.
    hilt = (0.13, 0.05, 1.72)
    mb.seg((0.03, 0.0, 0.85), hilt, 0.11, 0.03, IRON, taper=1.3)
    mb.seg((hilt[0], hilt[1] - 0.2, hilt[2]), (hilt[0], hilt[1] + 0.2, hilt[2]), 0.06, 0.06, IRON)
    top = (hilt[0] + 0.03, hilt[1] + 0.01, hilt[2] + 0.28)
    mb.seg(hilt, top, 0.05, 0.05, WOOD)
    mb.box(top, (0.09, 0.09, 0.09), IRON)


def checkpoint_coals(mb):
    """Tas de braises dans la vasque (objet à part : le jeu l'assombrit tant que le brasier
    n'est pas ranimé)."""
    mb.cylinder((0, 0, COALS - 0.06), 0.56, 0.12, FIRE, sides=8, radius_top=0.32)
    for k in range(5):
        a = 2 * math.pi * k / 5 + 0.4
        mb.box((0.22 * math.cos(a), 0.22 * math.sin(a), COALS + 0.02), (0.14, 0.12, 0.1), FIRE, taper=(0.6, 0.6))


def checkpoints():
    for i, c in enumerate(LEVEL["checkpoints"]):
        x, z = c["pos"]
        y = floor_at(x, z) or 0.0
        obj(f"checkpoint_{i}", checkpoint, loc=B(x, z, y))
        obj(f"checkpoint_{i}_coals", checkpoint_coals, loc=B(x, z, y))
        light(f"light_checkpoint_{i}", B(x, z, y + COALS + 0.35))


def ring_marks(mb):
    """La piste du colosse : un cercle peint rouge et blanc sur les dalles."""
    for (cx, cz), (rx, rz), y in ellipses():
        if rx >= 6.0 and abs(rx - rz) < 1e-3:
            bx, by, _ = B(cx, cz)
            sides = 32
            for i in range(sides):
                a0, a1 = 2 * math.pi * i / sides, 2 * math.pi * (i + 1) / sides
                v = [mb.bm.verts.new((bx + r * math.cos(a), by + r * math.sin(a), y + 0.025))
                     for r, a in ((rx - 1.2, a0), (rx - 0.8, a0), (rx - 0.8, a1), (rx - 1.2, a1))]
                mb._face(v, RED if i % 2 else CREAM)


def far_lamps():
    """Au loin dans le vide : des réverbères sur des rochers flottants, seuls points de lumière."""
    zs = [f["shape"]["Ellipse"]["center"][1] for f in LEVEL["floors"] if "Ellipse" in f["shape"]]
    cz = (min(zs) + max(zs)) / 2
    for i in range(18):
        a = 2 * math.pi * (i + rng.random() * 0.6) / 18
        d = 40 + rng.random() * 60
        x, z = 5 + d * math.cos(a), cz + d * 1.3 * math.sin(a)
        if abs(z) < 30 and abs(x) < 30:
            continue  # pas dans l'arène
        y = -14 - rng.random() * 30
        s = 1.4 + rng.random() * 1.2  # plus grands que nature : on les voit de loin

        def build(mb, s=s, seed=i):
            rock_cone(mb, 0, 0, 0, 1.6 * s, 1.3 * s, 4 * s, seed=500 + seed, sides=7, rings=2)
            mb.cylinder((0, 0, 1.5 * s), 0.07 * s, 3 * s, IRON, sides=5)
            mb.box((0, 0, 3.2 * s), (0.4 * s, 0.4 * s, 0.5 * s), GLASS)
        obj(f"far_lamp_{i}", build, loc=B(x, z, y))


# ----------------------------------------------------------------------------- assemblage

obj("floor", arena_floor)
obj("wall", wall)
obj("arena_base", arena_base)
obj("pillars", pillars)
obj("braziers", braziers)
obj("carousel", carousel)
obj("skyline", skyline)
obj("porch", porch_and_landing)
for i, f in enumerate(LEVEL["floors"]):
    if f.get("arena"):
        continue  # le palier est dessiné avec le porche
    if "Ellipse" in f["shape"]:
        obj(f"platform_{i}", lambda mb, f=f, i=i: platform(mb, f, seed=200 + i))
    else:
        obj(f"strip_{i}", lambda mb, f=f, i=i: strip(mb, f, i))
obj("ring_marks", ring_marks)
props()
checkpoints()
far_lamps()
for name, p in LIGHTS:
    e = bpy.data.objects.new(name, None)
    e.location = p
    sc.collection.objects.link(e)
export("arena")
