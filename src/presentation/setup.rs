//! Camera, offscreen target and HUD node spawning.
use super::{CYAN, Offscreen, Overlay, VIEW_HEIGHT};
use super::{banners, bench, help};
use bevy::camera::{Hdr, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;

pub(crate) fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut camera = commands.spawn((
        Camera2d,
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::FixedVertical {
                viewport_height: VIEW_HEIGHT,
            },
            ..OrthographicProjection::default_2d()
        }),
        // HDR with no tonemapping keeps colors exact and lets bloom be toggled safely.
        Hdr,
        Tonemapping::None,
        Msaa::Sample4,
    ));
    if std::env::var_os("SSC_OFFSCREEN").is_some() {
        // SSC_OFFSCREEN_SIZE=1280x720 picks the image size (default 1800x1200).
        let (width, height) = std::env::var("SSC_OFFSCREEN_SIZE")
            .ok()
            .and_then(|v| {
                let (w, h) = v.split_once('x')?;
                Some((w.parse().ok()?, h.parse().ok()?))
            })
            .unwrap_or((1800, 1200));
        let image = Image::new_target_texture(
            width,
            height,
            bevy::render::render_resource::TextureFormat::Rgba8UnormSrgb,
            None,
        );
        let handle = images.add(image);
        // The UI follows the default UI camera, which must be told when it has no window.
        camera.insert((
            bevy::camera::RenderTarget::Image(handle.clone().into()),
            bevy::ui::IsDefaultUiCamera,
        ));
        commands.insert_resource(Offscreen(handle));
    }
    help::spawn(&mut commands);
    commands.spawn((
        Overlay,
        Text::new(""),
        TextFont::from_font_size(30.0),
        TextColor(CYAN),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(43),
            ..default()
        },
    ));
    banners::spawn_feed(&mut commands);
    banners::spawn_banner(&mut commands);
    bench::spawn(&mut commands);
}
