//! Checkpoint view for the travel menu: a second camera films only the scenery
//! (layer `PREVIEW_LAYER`, without the fighters) into a small image, with the PS1 rendering.

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;

use crate::sim::data::Tuning;

/// Render layer of the scenery seen by the preview camera (the scenery is also on layer 0).
pub const PREVIEW_LAYER: usize = 2;
/// Preview image size, in pixels.
pub const PREVIEW_SIZE: UVec2 = UVec2::new(160, 120);

#[derive(Resource)]
pub struct CheckpointPreview {
    pub image: Handle<Image>,
    /// Preview shown: the camera only renders in that case.
    pub shown: bool,
    /// Checkpoint shown.
    pub index: usize,
}

#[derive(Component)]
struct PreviewCamera;

pub struct PreviewPlugin;

impl Plugin for PreviewPlugin {
    fn build(&self, app: &mut App) {
        let mut image = Image::new_target_texture(PREVIEW_SIZE.x, PREVIEW_SIZE.y, TextureFormat::Rgba8UnormSrgb, None);
        image.sampler = ImageSampler::nearest();
        image.asset_usage = RenderAssetUsages::default();
        let image = app.world_mut().resource_mut::<Assets<Image>>().add(image);
        app.insert_resource(CheckpointPreview { image, shown: false, index: 0 })
            .add_systems(Startup, spawn_camera)
            .add_systems(PostUpdate, update_camera);
    }
}

fn spawn_camera(mut commands: Commands, preview: Res<CheckpointPreview>) {
    commands.spawn((
        PreviewCamera,
        Camera3d::default(),
        Camera {
            order: -2,
            is_active: false,
            clear_color: ClearColorConfig::Custom(Color::srgb(0.05, 0.045, 0.07)),
            ..default()
        },
        RenderTarget::Image(preview.image.clone().into()),
        RenderLayers::layer(PREVIEW_LAYER),
        Msaa::Off,
        Tonemapping::None,
        DebandDither::Disabled,
        Projection::Perspective(PerspectiveProjection { fov: 55f32.to_radians(), near: 0.1, far: 200.0, ..default() }),
        bevy::light::cluster::ClusterConfig::Single,
    ));
}

fn update_camera(
    preview: Res<CheckpointPreview>,
    tuning: Res<Tuning>,
    mut cam: Single<(&mut Camera, &mut Transform), With<PreviewCamera>>,
) {
    if cam.0.is_active != preview.shown {
        cam.0.is_active = preview.shown;
    }
    if preview.shown {
        let cps = &tuning.level.checkpoints;
        let [eye, target] = cps[preview.index.min(cps.len() - 1)].view;
        *cam.1 = Transform::from_translation(Vec3::from(eye)).looking_at(Vec3::from(target), Vec3::Y);
    }
}
