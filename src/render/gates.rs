//! The bosses' passages in the world: the fogs (at the end of each corridor, and at the door of
//! each arena), the portcullis that bars the way once the boss is defeated, the torches in the
//! boss's colour (lit as long as it lives), and the final door with its medallions, which light up
//! one by one and open it once all the bosses are defeated, and the painted panel beyond it.
//!
//! The stone (gateways, torch poles, the door's frame and leaves, the panel) is in the scenery
//! (`tools/blender/arena.py`); what changes with the state is made here.

use bevy::prelude::*;

use super::models::{LightKind, SceneLights};
use super::ps1::{Ps1Material, TintMeshes};
use super::{AppState, LocalPlayer};
use crate::sim::data::Tuning;
use crate::sim::encounter::{self, Encounter};
use crate::sim::fighter::Body;
use crate::sim::player::Player;
use crate::sim::world::{self, Zone};

/// Height of the fogs, and of a torch's flame above the ground (`fx::TORCH_FLAME`).
const FOG_HEIGHT: f32 = 4.2;
/// The final door: height of its opening (`tools/blender/arena.py`), its medallions above it.
pub const DOOR_HEIGHT: f32 = 4.0;
const MEDALLION_HEIGHT: f32 = 4.75;
const MEDALLION_SPACING: f32 = 0.62;
/// How far in front of the door's plane: just in front of their sockets.
const MEDALLION_OUT: f32 = 0.56;
/// Opening angle of the leaves, and duration of the opening (seconds).
const DOOR_ANGLE: f32 = 1.75;
const DOOR_OPENING: f32 = 3.0;
/// The painted panel lights up (or goes out) over this long (seconds).
const FINALE_FADE: f32 = 1.5;

/// A fog veil: at the end of boss `arena`'s corridor (`door: false`), or at its arena's door.
#[derive(Component)]
struct FogVeil {
    arena: u8,
    door: bool,
}

/// Portcullis barring the fog of a defeated boss.
#[derive(Component)]
struct GateBars(u8);

/// Flame of a boss's torch.
#[derive(Component)]
struct TorchFlame(u8);

/// Medallion of the final door (boss), lit once it's defeated.
#[derive(Component)]
struct Medallion(u8);

/// Leaf of the final door (scenery objects `final_door_<0|1>`, their origin on the hinge), and
/// its orientation when closed; +1 / -1: the direction it opens.
#[derive(Component)]
pub struct DoorLeaf {
    pub closed: Quat,
    pub side: f32,
}

/// The painted panel beyond the final door (scenery object `finale_panel`): out of sight from the
/// rest of the level (its wall can't hide it from everywhere), it lights up once you step through
/// the open door. 0 (out, hidden) → 1 (lit).
#[derive(Component, Default)]
pub struct FinalePanel(f32);

/// Opening of the final door, 0 (closed) → 1.
#[derive(Resource, Default)]
struct DoorOpening(f32);

pub struct GatesPlugin;

impl Plugin for GatesPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DoorOpening>()
            .add_systems(OnExit(AppState::Loading), spawn_gates)
            .add_systems(Update, (fogs, torches, medallions, door, finale_panel).run_if(in_state(AppState::Playing)));
    }
}

/// Rotation that turns the local -Z (the front of a rectangle) towards `dir`.
fn facing(dir: Vec3) -> Quat {
    Quat::from_rotation_y(f32::atan2(dir.x, dir.z))
}

fn spawn_gates(
    mut commands: Commands,
    tuning: Res<Tuning>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut mats: ResMut<Assets<Ps1Material>>,
    mut lights: ResMut<SceneLights>,
) {
    let t = &*tuning;
    let mut fog = Ps1Material::unlit(Color::srgba(0.55, 0.58, 0.68, 0.3));
    fog.alpha_mode = AlphaMode::Blend;
    let iron = mats.add(Ps1Material::new(Color::srgb(0.16, 0.15, 0.16), None));
    let bar_v = meshes.add(Cuboid::new(0.09, FOG_HEIGHT, 0.09));
    // Flame: a bright cone in a translucent one, their base in the cup.
    let core = meshes.add(Cone { radius: 0.11, height: 0.42 }.mesh().resolution(5));
    let halo = meshes.add(Cone { radius: 0.24, height: 0.7 }.mesh().resolution(6));
    for (i, a) in t.arenas.iter().enumerate() {
        let i = i as u8;
        let (gate, gate_dir) = world::portal(t, &a.gate);
        let (door, door_dir) = world::portal(t, &a.door);
        // Two slightly offset veils, for a bit of thickness. The theatre's door is its gate.
        let mut veils = vec![(gate, gate_dir, a.gate.half_width, false)];
        if door.distance(gate) > 0.5 {
            veils.push((door, door_dir, a.door.half_width, true));
        }
        for (pos, dir, hw, is_door) in veils {
            let mesh = meshes.add(Rectangle::new(hw * 2.0 + 0.6, FOG_HEIGHT));
            for (dz, flip) in [(0.0, false), (0.25, true)] {
                let rot = facing(dir) * if flip { Quat::from_rotation_y(std::f32::consts::PI) } else { Quat::IDENTITY };
                commands.spawn((
                    FogVeil { arena: i, door: is_door },
                    Mesh3d(mesh.clone()),
                    MeshMaterial3d(mats.add(fog.clone())),
                    Transform::from_translation(pos + dir * dz + Vec3::Y * FOG_HEIGHT / 2.0).with_rotation(rot),
                    Visibility::Hidden,
                ));
            }
        }
        // Portcullis: bars across the opening, just in front of the fog.
        let hw = a.gate.half_width;
        let n = (hw * 2.0 / 0.4).round() as i32;
        let bar_h = meshes.add(Cuboid::new(hw * 2.0 + 0.2, 0.1, 0.1));
        commands
            .spawn((GateBars(i), Transform::from_translation(gate - gate_dir * 0.15).with_rotation(facing(gate_dir)), Visibility::Hidden))
            .with_children(|c| {
                for k in 0..=n {
                    let x = -hw + 2.0 * hw * k as f32 / n as f32;
                    c.spawn((Mesh3d(bar_v.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(x, FOG_HEIGHT / 2.0, 0.0)));
                }
                for y in [0.9, 2.4, 3.8] {
                    c.spawn((Mesh3d(bar_h.clone()), MeshMaterial3d(iron.clone()), Transform::from_xyz(0.0, y, -0.06)));
                }
            });
        // Torch flame, in the boss's colour: a bright core in a halo.
        let [r, g, b] = t.encounter_color(i as usize);
        let color = Vec3::new(r, g, b);
        let mut c = Ps1Material::unlit(Color::srgb(r, g, b).mix(&Color::WHITE, 0.45));
        c.params.emissive = (color * 0.8).extend(0.0);
        let core_mat = mats.add(c);
        let mut h = Ps1Material::unlit(Color::srgba(r, g, b, 0.35));
        h.alpha_mode = AlphaMode::Add;
        let halo_mat = mats.add(h);
        let at = encounter::torch_pos(t, i as usize) + Vec3::Y * crate::fx::TORCH_FLAME;
        commands.spawn((TorchFlame(i), Transform::from_translation(at), Visibility::Hidden)).with_children(|c| {
            c.spawn((Mesh3d(core.clone()), MeshMaterial3d(core_mat), Transform::from_xyz(0.0, 0.21, 0.0)));
            c.spawn((Mesh3d(halo.clone()), MeshMaterial3d(halo_mat), Transform::from_xyz(0.0, 0.3, 0.0)));
        });
        lights.0.push((at + Vec3::Y * 0.2, LightKind::Torch(i)));
    }
    // The final door's medallions: one per boss, in its colour, above the opening.
    let (door, dir) = world::portal(t, &t.level.final_door);
    let side = Vec3::new(-dir.z, 0.0, dir.x);
    let gem = meshes.add(Cuboid::new(0.3, 0.3, 0.08));
    let n = t.arenas.len();
    for i in 0..n {
        let x = (i as f32 - (n as f32 - 1.0) / 2.0) * MEDALLION_SPACING;
        let pos = door + side * x + Vec3::Y * MEDALLION_HEIGHT - dir * MEDALLION_OUT;
        let [r, g, b] = t.encounter_color(i);
        let mut m = Ps1Material::unlit(Color::srgb(r, g, b));
        m.params.emissive = Vec4::new(r, g, b, 0.0) * 0.6;
        commands.spawn((
            Medallion(i as u8),
            Mesh3d(gem.clone()),
            MeshMaterial3d(mats.add(m)),
            Transform::from_translation(pos).with_rotation(facing(dir) * Quat::from_rotation_z(std::f32::consts::FRAC_PI_4)),
        ));
    }
}

fn local(players: &Query<&Player, With<LocalPlayer>>) -> Zone {
    players.single().map_or(Zone::Level, |p| p.zone)
}

/// The fog of a living boss's corridor (and the door of the arena you're in); once it's defeated,
/// the portcullis (seen from the level).
fn fogs(
    time: Res<Time>,
    enc: Res<Encounter>,
    players: Query<&Player, With<LocalPlayer>>,
    mut veils: Query<(&FogVeil, &mut Visibility, &MeshMaterial3d<Ps1Material>), Without<GateBars>>,
    mut bars: Query<(&GateBars, &mut Visibility), Without<FogVeil>>,
    mut mats: ResMut<Assets<Ps1Material>>,
) {
    let zone = local(&players);
    let t = time.elapsed_secs();
    for (k, (v, mut vis, h)) in veils.iter_mut().enumerate() {
        let inside = zone == Zone::Arena(v.arena);
        let show = if v.door { inside } else { !enc.is_defeated(v.arena) || inside };
        let want = if show { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if show && let Some(mut m) = mats.get_mut(&h.0) {
            m.params.base_color.w = 0.26 + 0.08 * (t * 1.3 + k as f32 * 2.0).sin();
        }
    }
    for (b, mut vis) in &mut bars {
        let want = if enc.is_defeated(b.0) && zone != Zone::Arena(b.0) { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
    }
}

/// The torches burn as long as their boss lives; the flame dances.
fn torches(
    time: Res<Time>,
    enc: Res<Encounter>,
    mut q: Query<(&TorchFlame, &mut Visibility, &Children)>,
    mut parts: Query<&mut Transform, Without<TorchFlame>>,
) {
    let t = time.elapsed_secs();
    for (f, mut vis, children) in &mut q {
        let want = if enc.is_defeated(f.0) { Visibility::Hidden } else { Visibility::Inherited };
        if *vis != want {
            *vis = want;
        }
        let ph = t * 9.0 + f.0 as f32 * 1.7;
        for (k, c) in children.iter().enumerate() {
            if let Ok(mut ct) = parts.get_mut(c) {
                let s = 1.0 + 0.1 * ph.sin() + 0.06 * (ph * 2.3).sin();
                let k = k as f32;
                ct.scale = Vec3::new(s, s * (1.0 + 0.25 * (ph * 1.3 + k).cos()), s);
                ct.rotation = Quat::from_rotation_y(t * (1.5 + k)) * Quat::from_rotation_z(0.08 * (ph * 0.8 + k).sin());
            }
        }
    }
}

/// A defeated boss's medallion shines in its colour; the others are dull stone.
fn medallions(enc: Res<Encounter>, time: Res<Time>, q: Query<(&Medallion, &MeshMaterial3d<Ps1Material>)>, tuning: Res<Tuning>, mut mats: ResMut<Assets<Ps1Material>>) {
    let t = time.elapsed_secs();
    for (m, h) in &q {
        let Some(mut mat) = mats.get_mut(&h.0) else { continue };
        let [r, g, b] = tuning.encounter_color(m.0 as usize);
        let c = Vec3::new(r, g, b);
        let (base, glow) = if enc.is_defeated(m.0) {
            (c * 0.75, c * (0.3 + 0.1 * (t * 2.0 + m.0 as f32).sin()))
        } else {
            (c * 0.12 + Vec3::splat(0.08), Vec3::ZERO)
        };
        let want = (base.extend(1.0), glow.extend(0.0));
        if (mat.params.base_color, mat.params.emissive) != want {
            mat.params.base_color = want.0;
            mat.params.emissive = want.1;
        }
    }
}

/// The final door swings open once all the bosses are defeated (already open when loading a game).
fn door(
    time: Res<Time>,
    enc: Res<Encounter>,
    tuning: Res<Tuning>,
    fx: Res<crate::fx::FxState>,
    mut opening: ResMut<DoorOpening>,
    mut leaves: Query<(&DoorLeaf, &mut Transform)>,
) {
    let target = if encounter::door_open(&tuning, enc.defeated) { 1.0 } else { 0.0 };
    opening.0 = if target > opening.0 && !fx.last.contains(&crate::sim::SimEvent::Respawned) {
        (opening.0 + time.delta_secs() / DOOR_OPENING).min(1.0)
    } else {
        target
    };
    let k = opening.0 * opening.0 * (3.0 - 2.0 * opening.0);
    for (leaf, mut tf) in &mut leaves {
        tf.rotation = leaf.closed * Quat::from_rotation_y(leaf.side * DOOR_ANGLE * k);
    }
}

/// The painted panel fades in from the black once the local player is through the open final
/// door, and back out when they leave.
#[allow(clippy::too_many_arguments)]
fn finale_panel(
    mut commands: Commands,
    time: Res<Time>,
    enc: Res<Encounter>,
    tuning: Res<Tuning>,
    players: Query<&Body, With<LocalPlayer>>,
    mut panels: Query<(Entity, &mut FinalePanel, &mut Visibility)>,
    children: Query<&Children>,
    mut meshes: TintMeshes,
    mut mats: ResMut<Assets<Ps1Material>>,
) {
    let (door, dir) = world::portal(&tuning, &tuning.level.final_door);
    let through = encounter::door_open(&tuning, enc.defeated) && players.iter().any(|b| (b.pos - door).dot(dir) > 0.0);
    let step = time.delta_secs() / FINALE_FADE;
    for (e, mut panel, mut vis) in &mut panels {
        let k = if through { (panel.0 + step).min(1.0) } else { (panel.0 - step).max(0.0) };
        let want = if k > 0.0 { Visibility::Inherited } else { Visibility::Hidden };
        if *vis != want {
            *vis = want;
        }
        if k != panel.0 {
            panel.0 = k;
            // From the black (the night around it) to its own colours.
            let tint = if k >= 1.0 { Vec4::ZERO } else { Vec4::new(0.0, 0.0, 0.0, 1.0 - k * k * (3.0 - 2.0 * k)) };
            super::ps1::set_tint(&mut commands, e, tint, &children, &mut meshes, &mut mats);
        }
    }
}
