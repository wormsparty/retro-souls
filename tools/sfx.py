"""Génère des bruitages simples (WAV mono 22 kHz) dans assets/audio/. Remplaçables à volonté.

python3 tools/sfx.py
"""
import math
import os
import random
import struct
import wave

SR = 22050
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "assets", "audio")
rng = random.Random(7)


def write(name, samples):
    os.makedirs(OUT, exist_ok=True)
    peak = max(1e-6, max(abs(s) for s in samples))
    with wave.open(os.path.join(OUT, name + ".wav"), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(struct.pack("<h", int(s / peak * 0.9 * 32767)) for s in samples))


def env(t, attack, decay):
    return min(1.0, t / attack) * math.exp(-t / decay) if attack > 0 else math.exp(-t / decay)


def noise_lp(n, alpha):
    y, out = 0.0, []
    for _ in range(n):
        y += alpha * (rng.uniform(-1, 1) - y)
        out.append(y)
    return out


def metal(dur, partials, decay, noise=0.3, pitch=1.0):
    n = int(SR * dur)
    nz = noise_lp(n, 0.6)
    out = []
    for i in range(n):
        t = i / SR
        s = sum(a * math.sin(2 * math.pi * f * pitch * t) * math.exp(-t / (decay * d)) for f, a, d in partials)
        s += nz[i] * noise * math.exp(-t / 0.02)
        out.append(s)
    return out


def thud(dur, f0, f1, decay, noise=0.5, alpha=0.08):
    n = int(SR * dur)
    nz = noise_lp(n, alpha)
    out, ph = [], 0.0
    for i in range(n):
        t = i / SR
        f = f1 + (f0 - f1) * math.exp(-t / 0.05)
        ph += 2 * math.pi * f / SR
        out.append((math.sin(ph) + nz[i] * noise) * env(t, 0.002, decay))
    return out


def whoosh(dur, a0, a1, peak=0.4):
    n = int(SR * dur)
    y, out = 0.0, []
    for i in range(n):
        t = i / n
        alpha = a0 + (a1 - a0) * t
        y += alpha * (rng.uniform(-1, 1) - y)
        out.append(y * math.sin(math.pi * min(1, t / peak if t < peak else 1 - (t - peak) / (1 - peak))))
    return out


CLANG = [(1180, 1.0, 1.0), (2630, 0.6, 0.6), (3950, 0.4, 0.4), (5510, 0.25, 0.3), (830, 0.3, 1.4)]
write("perfect_guard", metal(0.9, CLANG, 0.35, noise=0.5, pitch=1.1))
write("guard", [a * 0.7 + b for a, b in zip(metal(0.5, CLANG, 0.12, noise=0.6, pitch=0.7), thud(0.5, 160, 70, 0.08))])
write("guard_break", [a + b for a, b in zip(metal(0.8, CLANG, 0.2, noise=0.8, pitch=0.55), thud(0.8, 120, 50, 0.2))])
write("hit", thud(0.35, 180, 60, 0.07, noise=1.4, alpha=0.25))
write("hit_heavy", thud(0.7, 120, 40, 0.18, noise=1.2, alpha=0.15))
write("slam", thud(1.2, 90, 30, 0.35, noise=1.0, alpha=0.05))
write("swing", whoosh(0.28, 0.05, 0.35))
write("swing_heavy", whoosh(0.45, 0.03, 0.2, peak=0.5))
write("dodge", whoosh(0.3, 0.02, 0.08, peak=0.3))
write("fury", [s * (0.6 + 0.4 * math.sin(i / SR * 2 * math.pi * 9)) for i, s in enumerate(metal(1.2, [(220, 1, 1), (331, 0.7, 1), (440, 0.5, 1)], 0.8, noise=0.1))])
write("fatal", [a + b for a, b in zip(thud(1.2, 70, 30, 0.4, noise=1.5, alpha=0.1), metal(1.2, CLANG, 0.3, noise=0.2, pitch=0.5))])
write("roar", [s * (0.5 + 0.5 * math.sin(i / SR * 2 * math.pi * 23)) for i, s in enumerate(thud(1.8, 90, 60, 0.9, noise=2.0, alpha=0.03))])
write("switch", metal(0.25, [(2400, 0.6, 1), (3100, 0.4, 1)], 0.05, noise=0.4))
# Soin : arpège doux montant.
heal = []
for k, f in enumerate((523, 659, 784, 1046)):
    n = int(SR * 0.5)
    for i in range(n):
        t = i / SR
        v = math.sin(2 * math.pi * f * t) * env(t, 0.01, 0.25) * 0.5
        idx = int(k * 0.07 * SR) + i
        while len(heal) <= idx:
            heal.append(0.0)
        heal[idx] += v
write("heal", heal)
# Braises absorbées : souffle qui monte et carillon cristallin.
embers = whoosh(1.0, 0.01, 0.12, peak=0.35)
for i in range(len(embers)):
    t = i / SR
    bell = sum(a * math.sin(2 * math.pi * f * t + 3 * math.sin(2 * math.pi * 5.5 * t)) for f, a in ((1568, 0.5), (2093, 0.35), (2637, 0.25), (3136, 0.15)))
    embers[i] = embers[i] * 0.9 + bell * env(max(t - 0.12, 0.0), 0.05, 0.3) * (0.0 if t < 0.12 else 0.45)
write("embers", embers)
print("sfx ok")
