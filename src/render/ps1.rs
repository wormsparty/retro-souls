//! Rendu « PS1 » : matériau dédié + rendu dans une petite texture agrandie sans filtrage.

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, Extent3d, ShaderType, TextureFormat};
use bevy::shader::ShaderRef;
use bevy::window::{PrimaryWindow, WindowResized};

use crate::settings::Settings;


#[derive(Clone, Copy, Debug, ShaderType)]
pub struct Ps1Params {
    pub base_color: Vec4,
    pub emissive: Vec4,
    pub tint: Vec4,
    pub sun_dir: Vec4,
    pub sun_color: Vec4,
    pub ambient: Vec4,
    pub fog_color: Vec4,
    pub fog: Vec4,
    pub misc: Vec4,
    pub lights: [Vec4; 8],
}

#[derive(Asset, TypePath, AsBindGroup, Clone, Debug)]
pub struct Ps1Material {
    #[uniform(0)]
    pub params: Ps1Params,
    #[texture(1)]
    #[sampler(2)]
    pub texture: Option<Handle<Image>>,
    pub alpha_mode: AlphaMode,
}

impl Material for Ps1Material {
    fn vertex_shader() -> ShaderRef {
        "shaders/ps1.wgsl".into()
    }
    fn fragment_shader() -> ShaderRef {
        "shaders/ps1.wgsl".into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
}

/// Lumière ponctuelle (brasero) prise en compte par le matériau PS1.
#[derive(Clone, Copy, Debug)]
pub struct PointLightPs1 {
    pub pos: Vec3,
    pub radius: f32,
    pub color: Vec3,
    pub intensity: f32,
}

/// Éclairage global, recopié dans tous les matériaux PS1 à chaque frame.
#[derive(Resource, Clone, Debug)]
pub struct Ps1Lighting {
    pub sun_dir: Vec3,
    pub sun_color: Vec3,
    pub sun_intensity: f32,
    pub ambient: Vec3,
    pub fog_color: Vec3,
    pub fog_start: f32,
    pub fog_end: f32,
    /// 1.0 = grille au pixel près ; plus petit = sommets plus « tremblants ».
    pub snap: f32,
    pub dither: f32,
    pub lights: Vec<PointLightPs1>,
}

impl Default for Ps1Lighting {
    fn default() -> Self {
        Self {
            sun_dir: Vec3::new(-0.35, 0.8, 0.45).normalize(),
            sun_color: Vec3::new(0.62, 0.62, 0.78),
            sun_intensity: 1.0,
            ambient: Vec3::new(0.24, 0.22, 0.3),
            fog_color: Vec3::new(0.05, 0.045, 0.07),
            fog_start: 12.0,
            fog_end: 46.0,
            snap: 0.5,
            dither: 1.0,
            lights: Vec::new(),
        }
    }
}

impl Ps1Material {
    pub fn new(color: Color, texture: Option<Handle<Image>>) -> Self {
        Self {
            params: Ps1Params {
                base_color: color.to_linear().to_vec4(),
                emissive: Vec4::ZERO,
                tint: Vec4::ZERO,
                sun_dir: Vec4::ZERO,
                sun_color: Vec4::ZERO,
                ambient: Vec4::ONE,
                fog_color: Vec4::ZERO,
                fog: Vec4::new(1000.0, 2000.0, 0.5, 1.0),
                misc: Vec4::new(if texture.is_some() { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0),
                lights: [Vec4::ZERO; 8],
            },
            texture,
            alpha_mode: AlphaMode::Opaque,
        }
    }

    pub fn unlit(color: Color) -> Self {
        let mut m = Self::new(color, None);
        m.params.misc.y = 1.0;
        m
    }

    pub fn from_standard(s: &StandardMaterial) -> Self {
        let mut m = Self::new(s.base_color, s.base_color_texture.clone());
        m.params.emissive = s.emissive.to_vec4();
        m.alpha_mode = s.alpha_mode;
        m
    }

    fn apply_lighting(&mut self, l: &Ps1Lighting) {
        let p = &mut self.params;
        p.sun_dir = l.sun_dir.extend(l.sun_intensity);
        p.sun_color = l.sun_color.extend(1.0);
        if p.misc.y < 0.5 {
            p.ambient = l.ambient.extend(1.0);
        }
        p.fog_color = l.fog_color.extend(1.0);
        p.fog = Vec4::new(l.fog_start, l.fog_end, l.snap, l.dither);
        p.lights = [Vec4::ZERO; 8];
        for (i, pl) in l.lights.iter().take(4).enumerate() {
            p.lights[i * 2] = pl.pos.extend(pl.radius);
            p.lights[i * 2 + 1] = pl.color.extend(pl.intensity);
        }
    }
}

/// Caméra 3D qui rend dans la texture basse résolution.
#[derive(Component)]
pub struct WorldCamera;

/// Image plein écran qui affiche la texture basse résolution.
#[derive(Component)]
pub struct LowResScreen;

#[derive(Resource)]
pub struct LowResTarget {
    pub image: Handle<Image>,
    pub size: UVec2,
}

pub struct Ps1Plugin;

impl Plugin for Ps1Plugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<Ps1Material>::default())
            .init_resource::<Ps1Lighting>()
            .add_systems(Startup, setup_low_res)
            .add_systems(Update, (resize_low_res, sync_lighting, swap_standard_materials));
    }
}

/// Taille de la résolution interne : hauteur choisie dans les options, largeur selon le ratio.
fn low_res_size(window: &Window, height: u32) -> UVec2 {
    let aspect = (window.width() / window.height().max(1.0)).clamp(1.0, 3.0);
    UVec2::new((height as f32 * aspect).round() as u32, height)
}

fn make_target(size: UVec2) -> Image {
    let mut image =
        Image::new_target_texture(size.x, size.y, TextureFormat::Rgba8UnormSrgb, None);
    image.sampler = ImageSampler::nearest();
    image.asset_usage = RenderAssetUsages::default();
    image
}

fn setup_low_res(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<Settings>,
) {
    let size = low_res_size(&window, settings.internal_height);
    let image = images.add(make_target(size));
    commands.insert_resource(LowResTarget { image: image.clone(), size });

    commands.spawn((
        WorldCamera,
        Camera3d::default(),
        Camera {
            order: -1,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.05, 0.045, 0.07)),
            ..default()
        },
        RenderTarget::Image(image.clone().into()),
        Msaa::Off,
        Tonemapping::None,
        DebandDither::Disabled,
        Projection::Perspective(PerspectiveProjection {
            fov: 60f32.to_radians(),
            near: 0.1,
            far: 200.0,
            ..default()
        }),
        Transform::from_xyz(0.0, 4.0, -16.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Caméra d'affichage : l'image basse résolution + le HUD en pleine résolution.
    commands.spawn((
        Camera2d,
        Camera { order: 0, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        Msaa::Off,
        IsDefaultUiCamera,
        bevy::ui_render::UiAntiAlias::Off,
        RenderLayers::layer(1),
    ));
    commands.spawn((
        LowResScreen,
        ImageNode::new(image),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            height: percent(100),
            ..default()
        },
        ZIndex(-10),
    ));
}

fn resize_low_res(
    mut resized: MessageReader<WindowResized>,
    window: Single<&Window, With<PrimaryWindow>>,
    settings: Res<Settings>,
    mut target: ResMut<LowResTarget>,
    mut images: ResMut<Assets<Image>>,
) {
    if resized.read().last().is_none() && !settings.is_changed() {
        return;
    }
    let size = low_res_size(&window, settings.internal_height);
    if size == target.size {
        return;
    }
    target.size = size;
    if let Some(mut img) = images.get_mut(&target.image) {
        img.resize(Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 });
    }
}

fn sync_lighting(lighting: Res<Ps1Lighting>, mut materials: ResMut<Assets<Ps1Material>>) {
    for (_, m) in materials.iter_mut() {
        m.apply_lighting(&lighting);
    }
}

/// Remplace les `StandardMaterial` (créés par le chargeur glTF) par des matériaux PS1.
/// Chaque entité reçoit sa propre instance pour pouvoir être teintée individuellement.
fn swap_standard_materials(
    mut commands: Commands,
    q: Query<(Entity, &MeshMaterial3d<StandardMaterial>)>,
    standard: Res<Assets<StandardMaterial>>,
    mut ps1: ResMut<Assets<Ps1Material>>,
    lighting: Res<Ps1Lighting>,
) {
    for (e, h) in &q {
        let Some(s) = standard.get(&h.0) else { continue };
        let mut m = Ps1Material::from_standard(s);
        m.apply_lighting(&lighting);
        commands
            .entity(e)
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .insert((MeshMaterial3d(ps1.add(m)), Ps1Swapped));
    }
}

/// Marqueur posé sur les meshes dont le matériau a été converti.
#[derive(Component)]
pub struct Ps1Swapped;
