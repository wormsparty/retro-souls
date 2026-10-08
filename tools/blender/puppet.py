"""Génère assets/models/puppet.glb : pantin de foire en bois (~1,9 m), fils coupés, armé du
maillet d'un stand de « tête de Turc ». Le colosse de la piste réutilise ce modèle, agrandi.

blender -b --factory-startup -P tools/blender/puppet.py

Les attaques sont calées sur les timings de assets/config/enemies.ron (type « puppet »).
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *  # noqa: E402,F403

reset_scene()
T = load_timings()["enemies"]["puppet"]

WOOD = material("p_wood", tex=tex_planks((0.55, 0.4, 0.26), seed=71))
WOOD_DARK = material("p_wood_dark", tex=tex_planks((0.3, 0.2, 0.13), seed=72))
VEST = material("p_vest", tex=tex_stripes((0.2, 0.32, 0.5), (0.8, 0.74, 0.58), n=6, seed=73))
RED = material("p_red", (0.62, 0.12, 0.1))
MASK = material("p_mask", (0.9, 0.86, 0.78))
PAINT = material("p_paint", (0.55, 0.06, 0.06))
EYES = material("p_eyes", (0.08, 0.06, 0.06))
GOLD = material("p_gold", (0.78, 0.6, 0.25))
STRING = material("p_string", (0.75, 0.72, 0.62))
IRON = material("p_iron", (0.25, 0.24, 0.25))

rig = Rig("puppet")
R, L = -1, 1


def joint(mb, x, y, z, r=0.055):
    mb.cylinder((x, y, z), r, r * 1.6, WOOD_DARK, sides=6)


def hips_geo(mb):
    mb.box((0, 0, 0.98), (0.3, 0.18, 0.14), WOOD_DARK)
    # Culotte bouffante à rayures.
    mb.box((0, 0, 0.88), (0.36, 0.24, 0.2), VEST, taper=(0.9, 0.9))


def chest_geo(mb):
    mb.box((0, 0, 1.32), (0.34, 0.2, 0.44), VEST, taper=(1.2, 1.05))
    mb.box((0, -0.11, 1.3), (0.12, 0.03, 0.34), RED, taper=(1.3, 1))
    for z in (1.2, 1.3, 1.4):
        mb.box((0, -0.13, z), (0.03, 0.02, 0.03), GOLD)
    mb.box((0, 0, 1.55), (0.42, 0.22, 0.06), WOOD)  # épaules
    # Fils coupés qui pendent encore des épaules.
    for s in (R, L):
        mb.seg((s * 0.18, 0, 1.58), (s * 0.24, 0.05, 2.05), 0.012, 0.012, STRING)


def head_geo(mb):
    mb.cylinder((0, 0, 1.62), 0.04, 0.08, WOOD_DARK, sides=6)
    mb.box((0, -0.01, 1.76), (0.2, 0.2, 0.22), MASK, taper=(0.88, 0.9))
    for s in (R, L):
        mb.box((s * 0.05, -0.112, 1.79), (0.04, 0.016, 0.03), EYES)
        mb.box((s * 0.07, -0.108, 1.71), (0.035, 0.016, 0.025), PAINT)  # joues fardées
    mb.box((0, -0.114, 1.69), (0.09, 0.016, 0.015), PAINT)  # sourire peint
    # Bonnet de bouffon à deux pointes.
    mb.box((0, 0, 1.9), (0.22, 0.22, 0.06), RED)
    for s in (R, L):
        mb.seg((s * 0.06, 0, 1.92), (s * 0.2, 0.04, 2.08), 0.08, 0.08, RED if s == R else VEST, taper=0.3)
        mb.box((s * 0.2, 0.04, 2.08), (0.04, 0.04, 0.04), GOLD)
    mb.seg((0, 0, 1.92), (0.02, 0.08, 2.3), 0.012, 0.012, STRING)


def arm_geo(side):
    x = 0.24 * side

    def upper(mb):
        joint(mb, x, 0, 1.5)
        mb.box((x, 0, 1.33), (0.08, 0.08, 0.28), WOOD)

    def fore(mb):
        joint(mb, x, 0, 1.17, 0.045)
        mb.box((x, 0, 1.03), (0.07, 0.07, 0.24), WOOD)

    def hand(mb):
        mb.box((x, -0.01, 0.86), (0.08, 0.09, 0.09), MASK)

    return upper, fore, hand


def leg_geo(side):
    x = 0.1 * side

    def thigh(mb):
        joint(mb, x, 0, 0.94)
        mb.box((x, 0, 0.76), (0.1, 0.1, 0.32), WOOD)

    def shin(mb):
        joint(mb, x, 0, 0.56, 0.05)
        mb.box((x, 0, 0.32), (0.08, 0.08, 0.42), WOOD)

    def foot(mb):
        mb.box((x, -0.05, 0.04), (0.11, 0.24, 0.08), WOOD_DARK, taper=(0.9, 0.85))

    return thigh, shin, foot


def mallet_geo(mb):
    # Pivot à la poignée (main droite) ; manche vers l'avant (-Y), masse au bout.
    x, z = -0.24, 0.87
    mb.cylinder((x, -0.4, z), 0.025, 1.0, WOOD_DARK, sides=6, axis="Y")
    mb.box((x, 0.12, z), (0.05, 0.05, 0.06), IRON)
    # Masse en travers du manche (axe X), cerclée de laiton.
    mb.box((x, -0.95, z), (0.4, 0.22, 0.22), RED)
    mb.box((x - 0.2, -0.95, z), (0.03, 0.24, 0.24), GOLD)
    mb.box((x + 0.2, -0.95, z), (0.03, 0.24, 0.24), GOLD)


rig.part("hips", (0, 0, 0.98), build=hips_geo)
rig.part("chest", (0, 0, 1.08), "hips", build=chest_geo)
rig.part("head", (0, 0, 1.6), "chest", build=head_geo)
for side, s in (("R", R), ("L", L)):
    up, fo, ha = arm_geo(s)
    x = 0.24 * s
    rig.part(f"upper_arm_{side}", (x, 0, 1.5), "chest", build=up)
    rig.part(f"forearm_{side}", (x, 0, 1.17), f"upper_arm_{side}", build=fo)
    rig.part(f"hand_{side}", (x, 0, 0.9), f"forearm_{side}", build=ha)
    th, sh, ft = leg_geo(s)
    lx = 0.1 * s
    rig.part(f"thigh_{side}", (lx, 0, 0.94), "hips", build=th)
    rig.part(f"shin_{side}", (lx, 0, 0.56), f"thigh_{side}", build=sh)
    rig.part(f"foot_{side}", (lx, 0, 0.08), f"shin_{side}", build=ft)
rig.part("mallet", (-0.24, 0, 0.87), "hand_R", build=mallet_geo)

# ----------------------------------------------------------------------------- poses (même logique que le boss)


def legs(tr=-15, sr=20, tl=10, sl=18, drop=-0.04, twist=0, fwd=0.0, lift=0.0):
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


# Pantin : un peu voûté, la tête penchée, le maillet traînant à moitié.
STANCE = merge(legs(-12, 18, 10, 16, -0.05), {"chest": (12, 0, 8), "head": (10, 0, -14)},
               arm("R", (-24, 10, 0), (-30, 0, 0), (40, 0, 0)), arm("L", (8, -8, 0), (-14, 0, 0), (0, 0, 0)))
FWD = merge(arm("R", (-80, 4, 0), (-10, 0, 0), (90, 0, 0)), arm("L", (-60, 24, 0), (-14, 0, 0), (60, 0, 0)))
OVER = merge(legs(-8, 18, 16, 22, -0.03), {"chest": (-16, 0, 0), "head": (-10, 0, 0)},
             arm("R", (-170, 6, 0), (-14, 0, 0), (45, 0, 0)), arm("L", (-150, 30, 0), (-14, 0, 0), (45, 0, 0)))
SLAM = merge(legs(-40, 40, 28, 14, -0.14, fwd=0.12), {"chest": (34, 0, 0), "head": (-14, 0, 0)},
             arm("R", (-62, 4, 0), (-4, 0, 0), (88, 0, 0)), arm("L", (-40, 28, 0), (-4, 0, 0), (30, 0, 0)))
PULL = merge(legs(-6, 30, 22, 30, -0.09, twist=-20), {"chest": (0, 0, -30), "head": (0, 0, 25)},
             arm("R", (-5, 30, 0), (-95, 0, 0), (98, 0, 0)), arm("L", (-60, 40, 0), (-60, 0, 0), (40, 0, 0)))
THRUST = merge(legs(-46, 40, 30, 10, -0.12, twist=5, fwd=0.16), {"chest": (18, 0, 6), "head": (-10, 0, -5)},
               arm("R", (-88, 2, 0), (-2, 0, 0), (74, 0, 0)), arm("L", (-20, 34, 0), (-8, 0, 0)))
# Fils coupés : affaissé, genoux pliés, bras ballants, tête sur la poitrine.
SLUMP = merge(legs(-50, 80, -40, 76, -0.36), {"chest": (40, 0, 10), "head": (50, 0, -20)},
              arm("R", (10, 20, 0), (-10, 0, 0), (20, 0, 0)), arm("L", (20, -14, 0), (-6, 0, 0)))
KNEEL = merge(legs(-80, 85, 75, 100, -0.48), {"chest": (38, 0, 8), "head": (35, 0, 0)},
              arm("R", (-40, 15, 0), (-30, 0, 0), (40, 0, 0)), arm("L", (-10, -10, 0), (-20, 0, 0)))

markers = {}


def anim(name, keys, loop=False):
    info = T.get(name, {"total": 0, "hits": []})
    end = add_animation(rig, name, keys, info)
    markers[name] = {"frames": end, "marks": anim_markers(info) if name in T else [0, end], "loop": loop}


def swing(z_from, z_to, low=False):
    lean = 18 if low else 10
    wind = merge(STANCE, {"chest": (4, 0, z_from * 1.2), "head": (0, 0, -z_from * 0.5)},
                 arm("R", (-72, 10, z_from * 0.4), (-24, 0, 0), (92, 0, 0)))
    step = legs(-30, 30, 22, 14, -0.09, fwd=0.1)
    a = merge(step, {"chest": (lean, 0, z_from), "head": (0, 0, -z_from * 0.4)}, FWD)
    b = merge(step, {"chest": (lean, 0, z_to), "head": (0, 0, -z_to * 0.4)}, FWD)
    follow = merge(step, {"chest": (lean + 6, 0, z_to * 1.2)}, arm("R", (-55, 8, z_to * 0.3), (-20, 0, 0), (100, 0, 0)))
    return wind, a, b, follow


# ----------------------------------------------------------------------------- locomotion
# Ballant, comme s'il était encore tiré par des fils.
anim("idle", [(0, STANCE), (50, merge(STANCE, {"chest": (16, 0, 4), "head": (16, 0, -20)})), (100, STANCE)], loop=True)
walk = []
for t, (tr, tl, lift) in ((0, (-25, 20, -0.05)), (20, (0, 0, 0.0)), (40, (20, -25, -0.05)), (60, (0, 0, 0.0)), (80, (-25, 20, -0.05))):
    sh_r = 50 if t == 60 else 15
    sh_l = 50 if t == 20 else 15
    sway = (tl - tr) * 0.25
    walk.append((t, merge(STANCE, legs(tr, sh_r, tl, sh_l, lift), {"chest": (14, sway * 0.4, 8 + sway), "head": (12, -sway, -14)})))
anim("walk", walk, loop=True)
anim("sleep", [(0, SLUMP), (90, merge(SLUMP, {"head": (54, 0, -16)})), (180, SLUMP)], loop=True)

# ----------------------------------------------------------------------------- actions
JERK = merge(STANCE, {"chest": (-10, 0, -10), "head": (-20, 0, 20)}, arm("L", (-30, 40, 0), (-20, 0, 0)))
anim("wake", [(0, SLUMP), (8, merge(SLUMP, {"head": (20, 0, 10)})), (16, with_hips(JERK, t=(0, 0, 0.04))),
              (26, merge(JERK, {"head": (-10, 0, -30)})), ("T", STANCE)])
anim("swing", [
    (0, STANCE), ("h0-28", OVER), ("h0-8", with_hips(OVER, t=(0, 0, 0.03))), ("h0", SLAM), ("h0e+24", SLAM),
    ("T", STANCE),
])
w, a, b, f = swing(-70, 70)
w2, a2, b2, f2 = swing(70, -70, low=True)
anim("combo", [
    (0, STANCE), ("h0-14", w), ("h0", a), ("h0e", b), ("h0e+8", f),
    ("h1-12", w2), ("h1", a2), ("h1e", b2), ("h1e+12", f2), ("T", STANCE),
])
anim("thrust", [
    (0, STANCE), (16, PULL), ("h0-12", with_hips(PULL, r=(0, 0, -6))), ("h0-2", PULL), ("h0", THRUST),
    ("h0e+14", THRUST), ("T", STANCE),
])
HIT = merge(STANCE, {"chest": (-18, 0, 16), "head": (-26, 0, 24)}, arm("L", (-40, 50, 0), (-30, 0, 0)), legs(-2, 22, 20, 30, -0.07))
anim("hit", [(0, STANCE), (4, HIT), (12, merge(HIT, {"chest": (-8, 0, 10)})), ("T", STANCE)])
FALLEN = merge(legs(-90, 0, -90, 10, 0), {"hips": {"r": (85, 0, 15), "t": (0, -0.55, -0.88)},
                                          "chest": (5, 0, 0), "head": (10, 0, 30)},
               arm("R", (-160, 30, 0)), arm("L", (-150, -40, 0)))
anim("death", [(0, STANCE), (8, HIT), (26, merge(KNEEL, {"head": (-30, 0, 0), "chest": (-10, 0, 0)})), (44, KNEEL),
               (62, FALLEN), ("T", FALLEN)])

missing = sorted(set(T) - set(markers))
if missing:
    raise SystemExit(f"animations manquantes : {missing}")

rest_pose(rig)
export("puppet", markers)
