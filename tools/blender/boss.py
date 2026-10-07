"""Génère assets/models/boss.glb : l'Automate du Carrousel (~3,2 m) et sa hallebarde.

blender -b --factory-startup -P tools/blender/boss.py
"""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *  # noqa: E402,F403

reset_scene()
T = load_timings()["boss"]

RED = material("b_red", tex=tex_stripes((0.62, 0.12, 0.1), (0.85, 0.75, 0.55), n=6, seed=31))
CANOPY = material("b_canopy", tex=tex_stripes((0.7, 0.14, 0.12), (0.9, 0.85, 0.7), n=8, seed=32))
WOOD = material("b_wood", tex=tex_planks((0.45, 0.3, 0.18), seed=33))
PORCELAIN = material("b_mask", (0.92, 0.88, 0.8))
GOLD = material("b_gold", (0.82, 0.62, 0.25))
IRON = material("b_iron", tex=tex_noise((0.32, 0.31, 0.33), 0.4, seed=34))
DARK = material("b_dark", (0.08, 0.06, 0.06))
EYES = material("b_eyes", (1.0, 0.55, 0.2), emissive=(1.0, 0.45, 0.1))
STEEL = material("b_steel", (0.7, 0.72, 0.76))

rig = Rig("boss")
R, L = -1, 1


def hips_geo(mb):
    mb.box((0, 0, 1.66), (0.5, 0.36, 0.26), WOOD)
    # Jupe en lamelles de manège.
    mb.box((0, 0, 1.35), (0.8, 0.62, 0.5), RED, taper=(0.66, 0.62))
    mb.box((0, 0, 1.62), (0.56, 0.42, 0.06), GOLD)


def chest_geo(mb):
    mb.box((0, 0, 2.2), (0.58, 0.4, 0.72), RED, taper=(1.35, 1.1))
    mb.box((0, -0.2, 2.2), (0.3, 0.04, 0.5), GOLD, taper=(1.3, 1))
    mb.cylinder((0, -0.235, 2.25), 0.09, 0.03, IRON, sides=8, axis="Y")
    mb.box((0, 0, 2.62), (0.84, 0.46, 0.12), GOLD)
    mb.box((0, 0, 1.92), (0.42, 0.32, 0.16), IRON)


def head_geo(mb):
    mb.cylinder((0, 0, 2.72), 0.07, 0.14, IRON, sides=6)
    mb.box((0, -0.01, 2.95), (0.3, 0.3, 0.36), PORCELAIN, taper=(0.85, 0.9))
    mb.box((-0.07, -0.155, 2.98), (0.06, 0.01, 0.035), EYES)
    mb.box((0.07, -0.155, 2.98), (0.06, 0.01, 0.035), EYES)
    mb.box((0, -0.158, 2.86), (0.12, 0.01, 0.02), DARK)
    # Chapeau chapiteau de manège.
    mb.cylinder((0, 0, 3.16), 0.42, 0.08, GOLD, sides=10)
    mb.cylinder((0, 0, 3.36), 0.4, 0.34, CANOPY, sides=10, radius_top=0.0, caps=True)
    mb.box((0, 0, 3.57), (0.05, 0.05, 0.12), GOLD)
    for i in range(10):
        a = 2 * math.pi * (i + 0.5) / 10
        mb.box((math.cos(a) * 0.43, math.sin(a) * 0.43, 3.1), (0.07, 0.07, 0.07), RED)


def arm_geo(side):
    x = 0.42 * side

    def upper(mb):
        mb.cylinder((x, 0, 2.6), 0.14, 0.18, GOLD, sides=8)
        mb.box((x, 0, 2.35), (0.15, 0.16, 0.48), WOOD, taper=(1.15, 1.15))

    def fore(mb):
        mb.cylinder((x, 0, 2.1), 0.09, 0.1, IRON, sides=6)
        mb.box((x, 0, 1.88), (0.13, 0.14, 0.42), WOOD, taper=(1.1, 1.1))
        mb.box((x, 0, 1.72), (0.17, 0.18, 0.06), RED)

    def hand(mb):
        mb.box((x, -0.01, 1.58), (0.13, 0.15, 0.15), PORCELAIN)

    return upper, fore, hand


def leg_geo(side):
    x = 0.18 * side

    def thigh(mb):
        mb.box((x, 0, 1.22), (0.19, 0.2, 0.74), WOOD, taper=(1.2, 1.2))

    def shin(mb):
        mb.cylinder((x, 0, 0.85), 0.11, 0.1, IRON, sides=6)
        mb.box((x, 0, 0.47), (0.15, 0.16, 0.72), WOOD, taper=(1.15, 1.15))

    def foot(mb):
        mb.box((x, -0.08, 0.06), (0.2, 0.38, 0.12), IRON, taper=(0.9, 0.85))

    return thigh, shin, foot


def halberd_geo(mb):
    # Pivot à la poignée (main droite) ; hampe vers l'avant (-Y).
    mb.cylinder((-0.42, -0.5, 1.55), 0.035, 3.2, WOOD, sides=6, axis="Y")
    mb.box((-0.42, 1.1, 1.55), (0.08, 0.08, 0.1), GOLD)
    base_y = -2.1
    # Fer de hache + crochet + pointe.
    mb.box((-0.42, base_y, 1.73), (0.04, 0.42, 0.34), STEEL, taper=(1, 1.35))
    mb.box((-0.42, base_y, 1.4), (0.04, 0.18, 0.22), STEEL, taper=(1, 0.2))
    mb.seg((-0.42, base_y - 0.2, 1.55), (-0.42, base_y - 0.75, 1.55), 0.07, 0.04, STEEL, taper=0.1)
    mb.box((-0.42, base_y + 0.25, 1.55), (0.1, 0.1, 0.1), GOLD)


rig.part("hips", (0, 0, 1.65), build=hips_geo)
rig.part("chest", (0, 0, 1.85), "hips", build=chest_geo)
rig.part("head", (0, 0, 2.75), "chest", build=head_geo)
for side, s in (("R", R), ("L", L)):
    up, fo, ha = arm_geo(s)
    x = 0.42 * s
    rig.part(f"upper_arm_{side}", (x, 0, 2.6), "chest", build=up)
    rig.part(f"forearm_{side}", (x, 0, 2.1), f"upper_arm_{side}", build=fo)
    rig.part(f"hand_{side}", (x, 0, 1.65), f"forearm_{side}", build=ha)
    th, sh, ft = leg_geo(s)
    lx = 0.18 * s
    rig.part(f"thigh_{side}", (lx, 0, 1.6), "hips", build=th)
    rig.part(f"shin_{side}", (lx, 0, 0.85), f"thigh_{side}", build=sh)
    rig.part(f"foot_{side}", (lx, 0, 0.12), f"shin_{side}", build=ft)
rig.part("halberd", (-0.42, 0, 1.55), "hand_R", build=halberd_geo)

# ----------------------------------------------------------------------------- poses


def legs(tr=-15, sr=20, tl=10, sl=18, drop=-0.06, twist=0, fwd=0.0, lift=0.0):
    return {
        "thigh_R": (tr, 0, 0), "shin_R": (sr, 0, 0), "foot_R": (-(tr + sr), 0, 0),
        "thigh_L": (tl, 0, 0), "shin_L": (sl, 0, 0), "foot_L": (-(tl + sl), 0, 0),
        "hips": {"r": (0, 0, twist), "t": (0, -fwd, drop + lift)},
    }


def arm(side, upper, fore=(0, 0, 0), hand=(0, 0, 0)):
    return {f"upper_arm_{side}": upper, f"forearm_{side}": fore, f"hand_{side}": hand}


def with_hips(pose, r=(0, 0, 0), t=(0, 0, 0)):
    out = dict(pose)
    base = out.get("hips", {"r": (0, 0, 0), "t": (0, 0, 0)})
    out["hips"] = {"r": tuple(a + b for a, b in zip(base["r"], r)),
                   "t": tuple(a + b for a, b in zip(base["t"], t))}
    return out


STANCE = merge(legs(-14, 20, 12, 18, -0.08), {"chest": (8, 0, 10), "head": (6, 0, -8)},
               arm("R", (-32, 12, 0), (-46, 0, 0), (58, 0, 0)), arm("L", (-48, 28, 0), (-40, 0, 0), (40, 0, 0)))
FWD = merge(arm("R", (-80, 4, 0), (-10, 0, 0), (90, 0, 0)), arm("L", (-78, 30, 0), (-14, 0, 0), (90, 0, 0)))
OVER = merge(legs(-8, 18, 16, 22, -0.04), {"chest": (-14, 0, 0), "head": (-10, 0, 0)},
             arm("R", (-170, 6, 0), (-14, 0, 0), (45, 0, 0)), arm("L", (-165, 22, 0), (-14, 0, 0), (45, 0, 0)))
SLAM = merge(legs(-40, 40, 28, 14, -0.22, fwd=0.2), {"chest": (32, 0, 0), "head": (-14, 0, 0)},
             arm("R", (-62, 4, 0), (-4, 0, 0), (88, 0, 0)), arm("L", (-60, 28, 0), (-4, 0, 0), (88, 0, 0)))
PULL = merge(legs(-6, 30, 22, 30, -0.14, twist=-20), {"chest": (0, 0, -30), "head": (0, 0, 25)},
             arm("R", (-5, 30, 0), (-95, 0, 0), (98, 0, 0)), arm("L", (-60, 40, 0), (-60, 0, 0), (90, 0, 0)))
THRUST = merge(legs(-46, 40, 30, 10, -0.18, twist=5, fwd=0.25), {"chest": (18, 0, 6), "head": (-10, 0, -5)},
               arm("R", (-88, 2, 0), (-2, 0, 0), (74, 0, 0)), arm("L", (-80, 34, 0), (-8, 0, 0), (74, 0, 0)))
CROUCH = merge(legs(-60, 90, -45, 85, -0.55), {"chest": (30, 0, 0)},
               arm("R", (-150, 10, 0), (-20, 0, 0), (40, 0, 0)), arm("L", (-145, 22, 0), (-20, 0, 0), (40, 0, 0)))
KNEEL = merge(legs(-80, 85, 75, 100, -0.75), {"chest": (38, 0, 8), "head": (35, 0, 0)},
              arm("R", (-40, 15, 0), (-30, 0, 0), (40, 0, 0)), arm("L", (-10, -10, 0), (-20, 0, 0)))

markers = {}


def anim(name, keys, loop=False):
    info = T.get(name, {"total": 0, "hits": []})
    end = add_animation(rig, name, keys, info)
    markers[name] = {"frames": end, "marks": anim_markers(info) if name in T else [0, end], "loop": loop}


def swing(z_from, z_to, low=False):
    lean = 18 if low else 10
    wind = merge(STANCE, {"chest": (4, 0, z_from * 1.2), "head": (0, 0, -z_from * 0.5)},
                 arm("R", (-72, 10, z_from * 0.4), (-24, 0, 0), (92, 0, 0)),
                 arm("L", (-68, 30, z_from * 0.4), (-26, 0, 0), (92, 0, 0)))
    step = legs(-30, 30, 22, 14, -0.14, fwd=0.15)
    a = merge(step, {"chest": (lean, 0, z_from), "head": (0, 0, -z_from * 0.4)}, FWD)
    b = merge(step, {"chest": (lean, 0, z_to), "head": (0, 0, -z_to * 0.4)}, FWD)
    follow = merge(step, {"chest": (lean + 6, 0, z_to * 1.2)},
                   arm("R", (-55, 8, z_to * 0.3), (-20, 0, 0), (100, 0, 0)),
                   arm("L", (-55, 30, z_to * 0.3), (-20, 0, 0), (100, 0, 0)))
    return wind, a, b, follow


# ----------------------------------------------------------------------------- locomotion
anim("idle", [(0, STANCE), (70, merge(STANCE, {"chest": (11, 0, 10), "head": (9, 0, -6)})), (140, STANCE)], loop=True)
walk = []
for t, (tr, tl, lift) in ((0, (-25, 20, -0.08)), (20, (0, 0, 0.0)), (40, (20, -25, -0.08)), (60, (0, 0, 0.0)), (80, (-25, 20, -0.08))):
    sh_r = 50 if (t == 60) else 15
    sh_l = 50 if (t == 20) else 15
    walk.append((t, merge(STANCE, legs(tr, sh_r, tl, sh_l, lift), {"chest": (10, 0, 10 + (tl - tr) * 0.1)})))
anim("walk", walk, loop=True)

# ----------------------------------------------------------------------------- attaques
w, a, b, f = swing(-70, 70)
w2, a2, b2, f2 = swing(70, -70, low=True)
anim("double_swing", [
    (0, STANCE), ("h0-14", w), ("h0", a), ("h0e", b), ("h0e+10", f),
    ("h1-10", w2), ("h1", a2), ("h1e", b2), ("h1e+12", f2), ("T", STANCE),
])
anim("overhead_slam", [
    (0, STANCE), ("h0-26", OVER), ("h0-6", with_hips(OVER, t=(0, 0, 0.04))), ("h0", SLAM), ("h0e+30", SLAM),
    ("T", STANCE),
])
SPIN_WIND = merge(legs(-30, 50, 20, 45, -0.3, twist=-40), {"chest": (24, 0, -60), "head": (0, 0, 50)},
                  arm("R", (-70, 20, -20), (-10, 0, 0), (75, 0, 0)), arm("L", (-60, 40, -20), (-20, 0, 0), (75, 0, 0)))
SPIN = merge(legs(-20, 40, 20, 40, -0.3), {"chest": (24, 0, 0)},
             arm("R", (-72, 0, 0), (-4, 0, 0), (76, 0, 0)), arm("L", (-70, 30, 0), (-8, 0, 0), (76, 0, 0)))
anim("spin_sweep", [
    (0, STANCE), ("h0-20", SPIN_WIND), ("h0-3", with_hips(SPIN_WIND, r=(0, 0, -15))),
    ("h0", with_hips(SPIN, r=(0, 0, 0))), ("h0+10", with_hips(SPIN, r=(0, 0, 180))),
    ("h0e", with_hips(SPIN, r=(0, 0, 360))), ("h0e+20", with_hips(SPIN, r=(0, 0, 380))),
    ("T", with_hips(STANCE, r=(0, 0, 360))),
])
LANCE = merge({"chest": (24, 0, 0), "head": (-18, 0, 0)},
              arm("R", (-55, 8, 0), (-30, 0, 0), (85, 0, 0)), arm("L", (-70, 30, 0), (-20, 0, 0), (90, 0, 0)))
charge = [(0, STANCE), (20, merge(legs(-20, 40, 25, 40, -0.2), LANCE)), (38, merge(legs(-30, 50, 30, 45, -0.25), LANCE))]
for i, t in enumerate(range(42, 78, 8)):
    if i % 2 == 0:
        charge.append((t, merge(legs(-45, 30, 35, 70, -0.1), LANCE)))
    else:
        charge.append((t, merge(legs(35, 70, -45, 30, -0.1), LANCE)))
charge += [(84, merge(legs(-30, 40, 25, 30, -0.2), LANCE)), ("T", STANCE)]
anim("charge", charge)
anim("delayed_thrust", [
    (0, STANCE), (18, PULL), (36, with_hips(PULL, t=(0, 0.04, 0.02))), (54, with_hips(PULL, r=(0, 0, -6))),
    ("h0-2", PULL), ("h0", THRUST), ("h0e+16", THRUST), ("T", STANCE),
])
anim("fury_leap", [
    (0, STANCE), (24, CROUCH), (42, with_hips(CROUCH, t=(0, 0, -0.08))),
    (50, with_hips(merge(OVER, legs(-30, 60, 10, 40)), t=(0, 0, 1.4))),
    (60, with_hips(merge(OVER, legs(-40, 70, 0, 50)), t=(0, 0, 1.9))),
    (68, with_hips(merge(OVER, {"chest": (10, 0, 0)}), t=(0, 0, 0.8))),
    ("h0", merge(SLAM, legs(-60, 80, 40, 60, -0.5))), ("h0e+40", merge(SLAM, legs(-60, 80, 40, 60, -0.5))),
    ("T", STANCE),
])
w3, a3, b3, f3 = swing(-70, 70)
w4, a4, b4, f4 = swing(70, -70, low=True)
anim("triple_combo", [
    (0, STANCE), ("h0-12", w3), ("h0", a3), ("h0e", b3), ("h0e+6", f3),
    ("h1-8", w4), ("h1", a4), ("h1e", b4), ("h1e+6", f4),
    ("h2-24", OVER), ("h2-5", with_hips(OVER, t=(0, 0, 0.04))), ("h2", SLAM), ("h2e+30", SLAM), ("T", STANCE),
])
DEEP_PULL = merge(PULL, legs(-10, 50, 30, 50, -0.3, twist=-30), {"chest": (6, 0, -40)})
anim("fury_thrust", [
    (0, STANCE), (24, DEEP_PULL), ("h0-6", with_hips(DEEP_PULL, r=(0, 0, -6))),
    ("h0", merge(THRUST, legs(-60, 45, 40, 5, -0.25, fwd=0.4))), ("h0e+18", merge(THRUST, legs(-60, 45, 40, 5, -0.25, fwd=0.4))),
    ("T", STANCE),
])

# ----------------------------------------------------------------------------- états
anim("groggy", [
    (0, STANCE), (14, KNEEL), (100, merge(KNEEL, {"head": (45, 0, 10)})), (180, KNEEL),
    (196, merge(STANCE, legs(-40, 60, 30, 60, -0.35))), ("T", STANCE),
])
anim("fatal_received", [
    (0, KNEEL), (40, KNEEL), (50, merge(KNEEL, {"chest": (-20, 0, 0), "head": (-40, 0, 0)},
                                       arm("L", (-80, -40, 0), (-20, 0, 0)))),
    (80, merge(KNEEL, {"chest": (50, 0, 0), "head": (50, 0, 0)})),
    (104, merge(STANCE, legs(-40, 60, 30, 60, -0.35))), ("T", STANCE),
])
ROAR = merge(legs(-20, 25, 20, 25, -0.1), {"chest": (-25, 0, 0), "head": (-40, 0, 0)},
             arm("R", (-100, 70, 0), (-10, 0, 0), (40, 0, 0)), arm("L", (-100, -70, 0), (-10, 0, 0)))
anim("roar", [(0, STANCE), (20, merge(STANCE, {"chest": (30, 0, 0)})), (34, ROAR),
              (80, merge(ROAR, {"head": (-30, 0, 10)})), ("T", STANCE)])
FALLEN = merge(legs(-90, 0, -90, 10, 0), {"hips": {"r": (85, 0, 15), "t": (0, -0.9, -1.45)},
                                          "chest": (5, 0, 0), "head": (10, 0, 30)},
               arm("R", (-160, 30, 0)), arm("L", (-150, -40, 0)))
anim("death", [(0, STANCE), (30, merge(KNEEL, {"head": (-30, 0, 0), "chest": (-10, 0, 0)})), (70, KNEEL),
               (110, FALLEN), ("T", FALLEN)])

missing = sorted(set(T) - set(markers))
if missing:
    raise SystemExit(f"animations manquantes : {missing}")

rest_pose(rig)
export("boss", markers)
