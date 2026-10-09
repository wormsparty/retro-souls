"""Generates assets/models/hound.glb: a starving stray dog (~0.9 m at the withers) + animations.

blender -b --factory-startup -P tools/blender/hound.py

Attacks are timed on the timings in assets/config/enemies.ron (type "hound").
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from common import *  # noqa: E402,F403

reset_scene()
T = load_timings()["enemies"]["hound"]

FUR = material("h_fur", tex=tex_noise((0.24, 0.2, 0.17), 0.55, seed=61))
FUR_DARK = material("h_fur_dark", tex=tex_noise((0.13, 0.11, 0.1), 0.5, seed=62))
RIBS = material("h_ribs", tex=tex_stripes((0.3, 0.26, 0.22), (0.18, 0.15, 0.13), n=8, seed=63))
BONE = material("h_bone", (0.72, 0.66, 0.55))
MOUTH = material("h_mouth", (0.35, 0.06, 0.05))
EYES = material("h_eyes", (1.0, 0.45, 0.15), emissive=(1.0, 0.4, 0.1))
COLLAR = material("h_collar", (0.55, 0.42, 0.2))

rig = Rig("hound")
R, L = -1, 1


def body_geo(mb):
    # High, deep chest, hollow belly, lower rump: silhouette of a starving greyhound.
    mb.box((0, -0.2, 0.58), (0.26, 0.42, 0.34), RIBS, taper=(0.9, 1.0))
    mb.box((0, 0.2, 0.6), (0.22, 0.4, 0.22), FUR, taper=(1.0, 1.0), shift_top=(0, 0.02))
    mb.box((0, 0.42, 0.6), (0.24, 0.16, 0.24), FUR)
    # Protruding spine.
    for i in range(6):
        mb.box((0, -0.3 + i * 0.14, 0.77 - abs(i - 2) * 0.01), (0.05, 0.06, 0.05), BONE)
    mb.box((0, -0.36, 0.7), (0.29, 0.1, 0.12), COLLAR)


def head_geo(mb):
    mb.box((0, -0.62, 0.72), (0.2, 0.24, 0.2), FUR, taper=(0.9, 0.9))
    mb.box((0, -0.82, 0.68), (0.12, 0.22, 0.1), FUR_DARK, taper=(0.8, 0.9))
    mb.box((0, -0.93, 0.7), (0.06, 0.03, 0.05), FUR_DARK)
    for s in (R, L):
        mb.box((s * 0.055, -0.715, 0.775), (0.04, 0.02, 0.03), EYES)
        mb.box((s * 0.07, -0.56, 0.86), (0.05, 0.05, 0.12), FUR_DARK, taper=(0.3, 0.6))
    mb.box((0, -0.82, 0.63), (0.1, 0.18, 0.02), MOUTH)


def jaw_geo(mb):
    mb.box((0, -0.8, 0.6), (0.1, 0.22, 0.05), FUR_DARK, taper=(0.8, 0.9))
    for s in (R, L):
        mb.box((s * 0.035, -0.88, 0.635), (0.015, 0.015, 0.03), BONE)


def neck_geo(mb):
    mb.box((0, -0.47, 0.7), (0.15, 0.2, 0.16), FUR, taper=(0.9, 1.0))


def leg_geo(x, y, top, mid, front):
    def upper(mb):
        mb.box((x, y, (top + mid) / 2), (0.08, 0.1, top - mid), FUR, taper=(0.8, 0.8))

    def lower(mb):
        mb.box((x, y, (mid + 0.04) / 2), (0.05, 0.06, mid - 0.04), FUR_DARK)
        mb.box((x, y - 0.03, 0.025), (0.07, 0.1, 0.05), FUR_DARK)

    return upper, lower


def tail_geo(mb):
    mb.seg((0, 0.5, 0.64), (0, 0.78, 0.5), 0.05, 0.05, FUR, taper=0.4)


rig.part("body", (0, 0.0, 0.58), build=body_geo)
rig.part("neck", (0, -0.4, 0.68), "body", build=neck_geo)
rig.part("head", (0, -0.55, 0.72), "neck", build=head_geo)
rig.part("jaw", (0, -0.68, 0.62), "head", build=jaw_geo)
rig.part("tail", (0, 0.5, 0.64), "body", build=tail_geo)
for side, s in (("R", R), ("L", L)):
    for end, y, top, mid in (("front", -0.28, 0.5, 0.27), ("hind", 0.4, 0.52, 0.27)):
        up, lo = leg_geo(s * 0.1, y, top, mid, end == "front")
        rig.part(f"{end}_{side}", (s * 0.1, y, top), "body", build=up)
        rig.part(f"{end}_low_{side}", (s * 0.1, y, mid), f"{end}_{side}", build=lo)

# ----------------------------------------------------------------------------- poses
# Positive X rotation: a hanging leg swings backwards; the body and head dip forward.


def legs(fr=0, fl=0, hr=0, hl=0, bend=(0, 0, 0, 0)):
    """Angles of the four legs (front right, front left, back right, back left)."""
    br, bl, hbr, hbl = bend
    return {
        "front_R": (fr, 0, 0), "front_low_R": (br, 0, 0),
        "front_L": (fl, 0, 0), "front_low_L": (bl, 0, 0),
        "hind_R": (hr, 0, 0), "hind_low_R": (hbr, 0, 0),
        "hind_L": (hl, 0, 0), "hind_low_L": (hbl, 0, 0),
    }


def body(r=(0, 0, 0), t=(0, 0, 0)):
    return {"body": {"r": r, "t": t}}


STAND = merge(legs(-4, -4, 6, 6, (6, 6, -14, -14)), {"neck": (-8, 0, 0), "head": (14, 0, 0), "tail": (10, 0, 0)})
# Head low, lips curled back: it growls.
GROWL = merge(STAND, body((6, 0, 0), (0, 0.04, -0.04)), {"neck": (14, 0, 0), "head": (0, 0, 0), "jaw": (12, 0, 0)},
              legs(-14, -4, 18, 8, (14, 6, -24, -18)))
markers = {}


def anim(name, keys, loop=False):
    info = T.get(name, {"total": 0, "hits": []})
    end = add_animation(rig, name, keys, info)
    markers[name] = {"frames": end, "marks": anim_markers(info) if name in T else [0, end], "loop": loop}


# ----------------------------------------------------------------------------- locomotion
anim("idle", [(0, GROWL), (30, merge(GROWL, body((7, 0, 0), (0, 0.04, -0.05)), {"jaw": (16, 0, 0)})), (60, GROWL)], loop=True)

walk = []
for k in range(5):
    ph = [0, 1, 0, -1, 0][k]
    walk.append((k * 10, merge(STAND, legs(-18 * ph, 18 * ph, 18 * ph, -18 * ph, (10 + 15 * max(ph, 0), 10 + 15 * max(-ph, 0), -14, -14)),
                               body((0, 0, 4 * ph), (0, 0, -0.01 * abs(ph))))))
anim("walk", walk, loop=True)

# Gallop: front legs together, then the hind ones; the body arches and stretches.
GATHER = merge(STAND, legs(30, 24, -40, -34, (40, 36, -50, -44)), body((-6, 0, 0), (0, 0, -0.05)), {"head": (24, 0, 0), "tail": (-10, 0, 0)})
STRETCH = merge(STAND, legs(-50, -44, 46, 40, (10, 8, -10, -8)), body((6, 0, 0), (0, 0, 0.06)), {"head": (6, 0, 0), "tail": (30, 0, 0)})
anim("run", [(0, GATHER), (8, merge(STRETCH, body((2, 0, 0), (0, 0, 0.1)))), (16, STRETCH), (24, GATHER)], loop=True)

# Asleep: curled up in a ball, head on its paws; it breathes.
LYING = merge(legs(-80, -76, 70, 74, (100, 96, -120, -116)), body((0, 4, 0), (0, 0, -0.36)),
              {"neck": (14, 0, 0), "head": (20, 0, -8), "tail": (60, 0, 50)})
anim("sleep", [(0, LYING), (60, merge(LYING, body((-2, 4, 0), (0, 0, -0.345)))), (120, LYING)], loop=True)

# ----------------------------------------------------------------------------- actions
HOWL = merge(STAND, body((-14, 0, 0), (0, 0.06, 0.02)), legs(-14, -14, 18, 18, (8, 8, -30, -30)),
             {"neck": (-36, 0, 0), "head": (-30, 0, 0), "jaw": (34, 0, 0), "tail": (40, 0, 0)})
anim("howl", [(0, LYING), (10, GROWL), (20, HOWL), (38, merge(HOWL, {"head": (-36, 0, 6)})), ("T", GROWL)])

COIL = merge(GROWL, body((10, 0, 0), (0, 0.12, -0.1)), legs(-24, -10, 30, 20, (34, 20, -50, -40)), {"jaw": (6, 0, 0)})
STRIKE = merge(STAND, body((4, 0, 0), (0, -0.18, 0.0)), legs(-40, -20, 30, 24, (10, 10, -6, -6)),
               {"neck": (-6, 0, 0), "head": (2, 0, 0), "jaw": (2, 0, 0)})
OPEN = merge(STRIKE, {"jaw": (42, 0, 0), "head": (-6, 0, 0)})
anim("bite", [(0, GROWL), ("h0-9", COIL), ("h0-3", OPEN), ("h0+1", STRIKE), ("h0e+6", merge(STRIKE, {"jaw": (8, 0, 0)})), ("T", GROWL)])
anim("snap", [
    (0, GROWL), ("h0-8", COIL), ("h0-2", OPEN), ("h0+1", merge(STRIKE, {"head": (2, 0, 14)})),
    ("h1-8", merge(COIL, {"head": (8, 0, -12)})), ("h1-2", merge(OPEN, {"head": (-6, 0, -14)})), ("h1+1", merge(STRIKE, {"head": (2, 0, -14)})),
    ("h1e+8", STRIKE), ("T", GROWL),
])
CROUCH = merge(GROWL, body((-8, 0, 0), (0, 0.16, -0.2)), legs(-30, -24, 60, 56, (60, 56, -90, -86)), {"head": (-4, 0, 0)})
LEAP = merge(STRETCH, body((-12, 0, 0), (0, -0.1, 0.42)), {"neck": (-20, 0, 0), "head": (-10, 0, 0), "jaw": (46, 0, 0)},
             legs(-80, -74, 60, 56, (6, 6, -4, -4)))
LAND = merge(STAND, body((14, 0, 0), (0, -0.1, -0.1)), legs(-50, -40, 10, 6, (30, 26, -20, -16)), {"jaw": (6, 0, 0), "head": (16, 0, 0)})
anim("lunge", [(0, GROWL), (12, CROUCH), (20, merge(CROUCH, body((-10, 0, 0), (0, 0.18, -0.22)))), ("h0", LEAP),
               ("h0e-2", merge(LEAP, body((4, 0, 0), (0, -0.1, 0.15)))), ("h0e+4", LAND), ("T", GROWL)])

RECOIL = merge(STAND, body((-12, 0, 10), (0, 0.12, 0.02)), {"neck": (-20, 0, 0), "head": (-14, 0, 14), "jaw": (20, 0, 0)},
               legs(-30, -10, 24, 30, (30, 10, -20, -30)))
anim("hit", [(0, GROWL), (4, RECOIL), (12, merge(RECOIL, body((-6, 0, 6), (0, 0.08, 0)))), ("T", GROWL)])

SIDE = merge(legs(-30, -20, 30, 20, (10, 10, -10, -10)), body((0, 88, 0), (0.0, 0.05, -0.38)),
             {"neck": (10, 0, 0), "head": (24, 0, 10), "jaw": (30, 0, 0), "tail": (10, 0, 0)})
anim("death", [(0, GROWL), (6, RECOIL), (22, merge(SIDE, body((0, 50, 0), (0, 0.05, -0.2)))), (34, SIDE),
               (44, merge(SIDE, legs(-40, -10, 40, 10))), ("T", merge(SIDE, legs(-36, -14, 34, 14)))])

missing = sorted(set(T) - set(markers))
if missing:
    raise SystemExit(f"missing animations: {missing}")

rest_pose(rig)
export("hound", markers)
