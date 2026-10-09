# Giant's Flame — combat prototype

A combat prototype inspired by *Lies of P* / *Dark Souls*, rendered PlayStation 1 style, in
Rust + [Bevy 0.19](https://bevy.org) (native, and in the browser via WASM).

One player, two weapons (rapier and greatsword), a slow, telegraphed boss — the Carousel
Automaton — in a circular arena. Leaving the arena, a staircase leads down to the Lamplighters'
Square (first checkpoint), hanging above the void; from there, a long path of bridges and
platforms, guarded by dogs and puppets, leads to the second checkpoint.

## Running

```sh
# Native (fastest for iterating)
cargo run --release

# Native with hot reloading of the tuning files (assets/config/*.ron)
cargo run --features dev

# Browser: build into dist/ (~5 min the first time), then http://127.0.0.1:8080
tools/build_web.sh
tools/serve_web.sh
```

Web requirements: the `wasm32-unknown-unknown` target and `wasm-bindgen-cli` **of the same version
as `wasm-bindgen` in `Cargo.lock`** (currently `cargo install wasm-bindgen-cli --version 0.2.129`).
`wasm-opt` (binaryen) is used if installed. WASM: ~31 MB, ~8 MB compressed.
Blender 5.x and Python 3 are only needed to regenerate the assets.

## Controls

| Action | Gamepad | Keyboard / mouse |
|---|---|---|
| Light attack | RB / R1 | Left click |
| Heavy attack (hold = charge) | RT / R2 | Right click |
| Guard (perfect guard = right timing) | LB / L1 | Q or Left Shift |
| Weapon special attack | LT / L2 | E |
| Dodge (hold = sprint) | B / ○ | Space |
| Sprint (one click, as long as the stick is pushed) | L3 | — |
| Lock on | R3 | Tab or middle click |
| Switch target (locked on): next point to the left / right, higher / lower (a large boss's head) | right stick ← → ↑ ↓ | mouse ← → ↑ ↓ |
| Switch weapon | D-pad ↑ | R |
| Jump attack (more damage, the jump's stamina almost covers it) | RB / RT in the air | click in the air |
| Use the selected item | X / □ | F |
| Next item (quick slots) | D-pad ↓ | C |
| Interact (pick up, rest); otherwise jump | A / ✕ | G |
| Move / camera | sticks | WASD / mouse |
| Menu | Start | Esc |

Keys are read by physical position (QWERTY/QWERTZ layout): on an AZERTY keyboard, movement is
on ZQSD and guard on A.

Click in the window to capture the mouse, Esc to release it. The **Help** page of the pause
menu shows the controls of the last device used (gamepad, or keyboard and mouse).

### Menus

- **Title screen**: the centred title, on a background of embers and ash rising above the glow
  of a brazier. New game (asks for confirmation if a save exists), Load, Options, *Fork me*
  (opens the project page on GitHub), Quit.
- **Pause** (Esc / Start): two icons, Equipment (helm) and System (cogwheel: Options, Help,
  *Fork me*, Return to title screen, Quit). Esc / (B) closes the menu. While paused,
  characters and spells are frozen; particles keep going.
- **Checkpoint** (when resting there): Leave (selected by default), Travel (to another brazier
  already kindled, with a view of the place), Choose the boss, Level up (greyed out, coming
  soon), Equipment, Revive the boss (if it has been defeated).
- **Choose the boss**: who waits in the arena (it appears there at once, at full strength), as a
  grid of portraits (`assets/ui/boss_<n>.png`, rendered by `tools/blender/boss_icons.py`). The
  encounters are in `assets/config/bosses.ron` (in debug: `SOULS_BOSS=n`). Each boss has its
  own colour (`color`), that of its ground circles:
  - the Carousel Automaton (`boss.ron`);
  - the Ash Wyrm: huge, you lock onto its head (right stick ↑), its legs or its tail. It
    doesn't aim: it moves roughly towards its prey, and ends up facing it. In front of it, a
    headbutt down to the ground or a body slam (it lets itself fall); on its flank, it pivots a
    quarter turn whipping its tail; behind, the tail sweeps; from afar, a stream of fire down to
    the ground; it takes flight and lands on its target, or rises out of reach and spits three
    fireballs; ash rain in phase 2;
  - the Horned Butcher and his two dogs: diagonal cleavers (left then right), leaning whirlwind
    at head height, head-down charge, cleave (shockwave), thrown cleaver; he leaps far back
    then charges or throws a cleaver; frenzy in phase 2. The dogs only have a small health bar
    and collapse with their master;
  - the Lamplighter and the Anvil: the lamplighter is a magician who keeps his distance and
    never stays still: he regularly slides sideways (glide), then often shoots from there
    (homing glows, a volley of three glows one after the other, rockets bursting from the
    ground, back leap); the smith hits hard (hammer, backhand, charge, earthquake). When one
    falls, the other enters phase 2 (rings of light, rain of rockets; trail of embers);
  - the Great Marionette (violet): she knows she's vulnerable and never stops moving: hops to
    the side or backwards, throwing two needles (arms raised above her: they start from very
    high, you see them coming); slap, drop onto her target, pirouette, dance; hoisted straight
    up, she drops back down on the spot (big shockwave); ascent: out of reach, she throws three
    needles then drops onto her target; slashing strings in phase 2;
  - the Hollow-Spined Beast (the lizard, belly to the ground): you lock onto its head or its
    hindquarters; bite, dorsal arms, pivot and tail swipe, leap; it rears up and comes down:
    ice bursts out in front of it; frenzy and howl (circles of ice) in phase 2;
  - the Flame-Bearing Giant: a column that splits the ground sending out a trail of fire,
    pillars of fire under its target (which don't land on top of each other), three fireballs
    one after the other, rings of flames, the first one at his feet; stick to him and you take
    a stomp that splits the ground all around, or the column mowing at knee height; too close,
    he also leaps back to shoot from afar; brazier, meteors and a wave of fire in phase 2.

  Models and animations: `tools/blender/trial_bosses.py`. Spells (projectiles and eruptions
  announced on the ground) from the same attack only hit once. A projectile that crashes on the
  ground keeps burning there for a while (small area, half damage, neither guard nor parry).
  Spells, shockwaves and circles are all in the boss's colour (except the cleaver, in iron).
- **Equipment** (Dark Souls style): at the top the weapons (shown only, you switch them in
  game) and the talisman, below them the consumable cells, with the icon of the equipped item —
  4 quick slots for consumables (in game, D-pad ↓ / C moves to the next equipped slot) and a
  talisman slot (the first talisman picked up is worn automatically). Choosing a cell opens the
  list of what you own (with icons and quantities); from the pause menu or a brazier.

Esc / (B) goes back to the previous page. The game starts in full screen.

| Option | Values |
|---|---|
| Display | Fullscreen (borderless) · Exclusive fullscreen (except Wayland and browser) · Windowed |
| Resolution | window size, or video mode in exclusive (native only) |
| Refresh rate | rates offered by the monitor, in exclusive (native only) |
| Vertical sync | on / off (native only) |
| Internal resolution | 240p (PS1), 480p (modern) |
| Master volume, effects volume | 0 to 100% |
| Camera sensitivity, inverted vertical axis, camera shake | |

Under **Wayland**, exclusive full screen doesn't exist (the protocol doesn't allow changing video
modes; winit ignores it): the option isn't offered. To get it anyway, run the game through
XWayland with `WAYLAND_DISPLAY= cargo run --release` (the compositor then emulates the mode
change).

Language: English or French, asked on first launch (English by default) and changeable in the
options.

Graphics style: PS1 (240p internal resolution) or modern (480p), asked on first launch right
after the language, with a screenshot of each (`assets/ui/style_*.png`). Changeable in the
options ("Internal resolution").

Settings are saved as soon as they change and restored on the next launch:
`~/.config/giants-flame/settings.ron` (Linux), `%APPDATA%\giants-flame\settings.ron` (Windows),
`~/Library/Application Support/giants-flame/settings.ron` (macOS), `localStorage` in the browser.
In the browser, full screen turns on at the first click or key press (the browser requires it);
Esc exits it, an action in game brings it back.

### Tuning tools

- **F1**: state overlay (current action to the tick, perfect guard window, stagger…)
- **F2**: hitboxes (blue = body, orange = active hit, red = rage, yellow = imminent hit)
- **F3**: slow motion ×0.25 (timings in ticks stay exact)
- **F4**: passive boss (it walks but no longer attacks)
- **F5**: respawn the fighters (progress kept, the boss starts over)

## Mechanics

- **Perfect guard**: press guard at most 8 ticks (~133 ms) before the impact. No damage,
  stagger dealt to the boss, special gauge. Spamming guard shrinks the window.
- **Normal guard**: 60% of the damage, converted into **regain** (grey bar) that you recover by
  hitting the boss within 6 seconds. Out of stamina → guard broken.
- **Rage attacks** (the boss glows red): normal guard is useless, you need a perfect guard or a
  dodge. The boss also glows red before an unblockable hit that no circle announces (the
  wyrm's fire stream): you have to flee.
- **Area attacks** (a circle on the ground, in the boss's colour, filling up until the impact):
  neither guard nor perfect guard. The circle appears at least 40 ticks (⅔ s) before the impact
  and no longer moves: as soon as it's shown, the boss stops tracking its target (a jump that
  lands on it tracks it until take-off, its flight serves as the warning). The *shockwave* lasts
  longer than a roll's i-frames: you have to get out of the circle. The *back leap* is often
  followed by the *crushing jump*, which lands where the player was at take-off.
- **Roll**: ~3.5 m (backstep: ~1.7 m).
- **Stagger**: boss stagger gauge full (perfect guards, charged heavy attacks) → it falls to its
  knees; light attack from the front for the **fatal blow**. The gauge isn't shown (F1 shows
  it).
- **Jump attack**: attack (light or heavy) during a jump: plunging thrust (rapier) or
  downward cut (longsword), more damage and stagger than a normal attack for very little
  stamina (the jump already paid for it); in the air, you also reach higher.
- **Rapier**: combo of 4 quick thrusts; charged heavy as a lunge; special *Lightning Lunge*
  (dash + flurry).
- **Greatsword**: slow and heavy, long reach, hyper armour during swings; 3 wide cuts;
  vertical heavy; special *Stance* (high guard with hyper armour: a hit taken during the
  stance triggers a riposte).
- **Stamina**: at least 1 point is needed to attack, dodge or use the special; an action can
  make stamina go negative (down to -60), you then have to wait. An attack's cost = its
  damage × `stamina_per_damage`: for equal damage, the same cost for both weapons.
- **Healing flask** (inventory item): 3 charges, refilled at the checkpoint and on death, +40%
  HP; you can walk slowly meanwhile. Hit before the heal applies: the charge is lost.
- **Special gauge** (gold bar under stamina): fills up by hitting and perfect guarding; each
  special attack uses a third of it. Special attack hits don't recharge the gauge, and its
  stamina cost is fixed (not proportional to its heavy damage).
- **HUD**: bottom left, the equipped weapon (icon; D-pad ↑ / R to switch) above the selected
  quick item.
- **Boss**: 9 attacks in phase 1 (including one rage attack and two area attacks), 2 more in
  phase 2 (below 50% HP).

## The path

```
                 arena (boss)
                   │ stairs
       Lamplighters' Square ◆ checkpoint ── bridge ── fountain garden
                   │ bridge
          bandstand: 2 sleeping dogs
                   │ ramp
   ticket booths: 2 puppets, 1 dog ── narrow walkway ── ledge: dog ⤳ jump: flask shard
                   │ long bridge: a puppet in the way
   colossus's track (unique) ── narrow plank ── ledge: crest plume
                   │ stairs
        broken belvedere ◆ checkpoint ── collapsed bridge…
                   ⤳ running jump: isolated rock, iron brooch
```

- **Jump** (A / G when there's nothing to pick up and no brazier in range): ~0.8 m high,
  ~3.5 m cleared at a run (`jump` in `player.ron`), 12 stamina. Coming down above the void
  means falling.
- **Respawning at a brazier**: next to the fire, facing the way forward (`look` in
  `level.ron`); a game quit at the foot of a brazier resumes the same way.
- **The void**: outside the arena and the stairs, there are no railings. Walking, rolling or
  being pushed past an edge means falling, and death. Below there is only darkness and, far
  off, a few street lamps lost on floating rocks.
- **Checkpoints** (braziers): an iron bowl on a pedestal, an old sword planted in the embers.
  Unlit, it only lets out a trickle of cinders; resting there kindles it ("BRAZIER KINDLED"):
  a column of embers and ash then rises above it, visible from afar. It becomes the respawn
  point and a travel destination. Resting restores HP, stamina and flasks, but **brings all
  enemies back**; impossible while an enemy is on your heels.
- **Death**: you respawn at the last brazier **without your embers**: they stay on the spot,
  with your corpse bathed in green from which green glows rise, visible from afar (at the edge
  you fell from, after a fall). Interacting near the corpse ("Recover") gets them back; dying
  before reaching it loses them for good.
- **Enemies**: asleep (you can get close, but they sense everything around them) or watching
  (they see far, in front of them). An alert cry wakes their whole group. Too far from their
  post, they give up, go back and heal. When defeated, they give embers; they come back on rest
  and on death, except the colossus.
  - *Stray dog*: fast, bites and pounce; staggered by every hit.
  - *Fairground puppet*: "high striker" mallet (vertical blow with hyper armour, backhand,
    advancing thrust); it takes two rapier hits to stagger it.
  - *Colossus of the track*: a giant puppet, unique, that hardly ever flinches.
  The player's hits lower down to opponents smaller than them (a thrust at chest height hits a
  dog).
- **Items on the ground**: white glows surrounded by sparks swirling upwards (visible through
  the fog); you only find out what they are by picking them up ("Pick up", item obtained
  popup). Each can only be picked up once.
  - consumables (quick slots, don't refill): *faded ember* / *lively ember* (crush them to gain
    embers), *golden moss* (regenerates HP), *ember resin* (+20% damage for a minute, the
    blade glows red);
  - talismans: *iron brooch* (damage taken −15%), *crest plume* (dodges −30% stamina);
  - *flask shard*: one more healing charge, permanently.

## Progress and saving

- You start at the square's **checkpoint**, at the foot of the arena stairs. Resting there
  restores HP, stamina and items.
- Entering the arena wakes the boss and a **fog** closes the stairs until the end of the fight.
- Victory: **+1000 embers** (`embers` in `boss.ron`, flame icon), shown at the bottom right above
  the counter, which absorbs them with a little sound; the boss stays dead, you can revive it
  from the checkpoint.
- Death: "YOU DIED" (~4 s, the screen darkens then goes black), then back to the last
  checkpoint you rested at (embers kept), items refilled, boss and enemies reset.
- **Autosave** Dark Souls style (a single slot): on every important event (rest, entering the
  arena, victory, enemy defeated, item picked up or used, fall, death, menu opened/closed),
  every 5 seconds and when quitting; the file is only rewritten if it changed. You resume where
  you quit, except mid boss fight (you come back in front of the fog, the boss starts over) or
  mid-fall (at the last checkpoint). Picked-up items, kindled braziers, defeated unique enemies
  and embers dropped on death are saved. File `save.ron` next to `settings.ron`
  (`localStorage` in the browser).

## Tuning the feel

All gameplay values are in `assets/config/` and expressed in **ticks** (60/s):

- `player.ron`: HP, stamina, speeds, dodge (i-frames), perfect guard, regain…
- `weapons.ron`: each attack (startup/active/recovery, hitbox, damage, stagger, root motion)
- `boss.ron`: HP, phases, stagger, pauses between attacks, each attack and its reach
- `bosses.ron`: the other bosses (same format, plus model and scale, lockable parts, spells)
  and the encounters offered at the checkpoint
- `arena.ron`: arena size, pillars, opening, boss spawn
- `level.ron`: the rest of the level — floors (square, bridges, ramps, stairs; walled edges or
  open onto the void), checkpoints (name, travel menu view), decor, enemies, items
- `enemies.ron`: enemy types (sight, chase, poise, embers, attacks)

With `cargo run --features dev`, saving a file restarts the fight with the new values.
Animations are retimed automatically if you change the hit windows. The scenery is generated
from `arena.ron` and `level.ron`: after changing the geometry, rerun `tools/build_assets.sh`
so the visuals match the collisions.

## Architecture

```
src/sim/      deterministic simulation at 60 Hz (no dependency on rendering)
  data.rs       tuning types (frame data)
  input.rs      PlayerInput: 7 bytes per player per tick (what will go over the network)
  player.rs     player state machine (combos, charge, guard, dodge, sprint…)
  boss.rs       boss AI (multiplayer aggro, weighted choice, cooldowns, phases, duos,
                lockable points of large bosses)
  spell.rs      boss spells: projectiles, eruptions announced on the ground
  enemy.rs      path enemy AI (post, group alert, chase, giving up)
  world.rs      walkable floors and heights, walled or open edges (falling), obstacles
  combat.rs     collisions, hitboxes (capsules, arc sweeps), guard / perfect / regain
  encounter.rs  checkpoints, travel, fog, victory, death/respawn, progress, menus
  items.rs      items (consumables, talismans, key items), inventory, quick slots
src/input.rs  keyboard/mouse/gamepad → PlayerInput (presses latched between two ticks)
src/render/   PS1 rendering, camera, models and animations driven by the sim state,
              checkpoint previews (travel menu)
src/fx.rs     sounds, sparks, shakes triggered by sim events
src/hud.rs    in-game interface
src/menu.rs   title screen, pause, checkpoint, equipment, options, help
src/save.rs   autosave (storage.rs: files / localStorage)
tools/blender script-generated assets (low-poly models, animations timed on the frame data)
tools/sfx.py  synthesised sound effects
tools/pixel_font.py  the interface's bitmap font
tests/sim.rs  simulation tests, including determinism
```

**Designed for online multiplayer (co-op against the boss).** The simulation only reads
`PlayerInput`s and a tick counter, uses `libm` maths and an internal RNG: replaying the same
inputs gives exactly the same state (checked by `cargo test`). That's the prerequisite for
rollback with `bevy_ggrs` + `matchbox` (peer-to-peer WebRTC in the browser) — the Bevy version
was chosen to be compatible with these two crates. The boss already picks its target among
several players.

## Regenerating the assets

```sh
tools/build_assets.sh   # timings and level → Blender (player, boss, weapons, dog, puppet, scenery) → sound effects → font
```

The `.blend` files are written to `tools/blender/blend/` for manual touch-ups (not versioned:
they're regenerated by `tools/build_assets.sh`). The produced `.glb` files are versioned.
`tools/blender/preview.py` renders contact sheets of poses to check an animation:

```sh
PREVIEW_WEAPON=rapier blender -b tools/blender/blend/player.blend -P tools/blender/preview.py -- out.png rapier_light1:0 rapier_light1:8
```

## Credits

Everything is generated by the repository's scripts: models, textures, animations, sounds, and
the interface's bitmap font (`tools/pixel_font.py`, 12-pixel em, French accents). The interface
is drawn on a grid of big pixels (≈ 360 rows, a whole number of screen pixels per dot) like the
game; the key icons (keyboard, mouse, Xbox gamepad) and item icons are drawn as pixel art by
`src/ui.rs` and `src/hud.rs`.
