//! Camera, offscreen target and HUD node spawning.
use super::*;

pub fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
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
    // The on-demand details (hold Tab or F3): the situation, the ship's gear and the rig, in
    // three columns over a dim backdrop. Nothing here is needed to fly; it is for looking up.
    commands
        .spawn((
            DetailsPanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                top: px(DETAILS_TOP),
                max_width: percent(96),
                flex_direction: FlexDirection::Row,
                flex_wrap: FlexWrap::Wrap,
                align_items: AlignItems::FlexStart,
                align_content: AlignContent::FlexStart,
                column_gap: px(26),
                padding: UiRect::axes(px(16), px(12)),
                border: UiRect::all(px(1)),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.9)),
            BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.35)),
            ScrollPosition::default(),
            Scrollable,
            GlobalZIndex(10),
        ))
        .with_children(|panel| {
            panel.spawn((
                Hud,
                Text::new(""),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.82, 0.88, 0.95)),
                Node {
                    width: px(300),
                    ..default()
                },
            ));
            for (first, last) in [(0, RIG_SPLIT), (RIG_SPLIT, RIG_LINES)] {
                panel
                    .spawn((
                        Text::new(""),
                        TextFont::from_font_size(13.0),
                        Node {
                            width: px(300),
                            ..default()
                        },
                    ))
                    .with_children(|column| {
                        for line in first..last {
                            column.spawn((
                                RigLine(line),
                                TextSpan::new(""),
                                TextFont::from_font_size(13.0),
                                TextColor(MUTED),
                            ));
                        }
                    });
            }
        });
    // The full key list (F1): every binding and the color legend, on a dim backdrop.
    commands
        .spawn((
            HelpPanel,
            Node {
                position_type: PositionType::Absolute,
                left: px(0),
                right: px(0),
                top: px(DETAILS_TOP),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::FlexStart,
                display: Display::None,
                ..default()
            },
            GlobalZIndex(30),
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(26), px(16)),
                    border: UiRect::all(px(1)),
                    max_width: percent(96),
                    overflow: Overflow::scroll_y(),
                    ..default()
                },
                BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                ScrollPosition::default(),
                Scrollable,
                HelpBody,
                Text::new(HELP_TEXT),
                TextFont::from_font_size(13.0),
                TextColor(Color::srgb(0.82, 0.88, 0.95)),
            ));
        });
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
    commands
        .spawn((
            Text::new(""),
            TextFont::from_font_size(14.0),
            TextLayout::justify(Justify::Center),
            Node {
                position_type: PositionType::Absolute,
                left: percent(4),
                width: percent(92),
                bottom: px(172),
                ..default()
            },
        ))
        .with_children(|feed| {
            for line in 0..FEED_LINES {
                feed.spawn((
                    FeedLine(line),
                    TextSpan::new(""),
                    TextFont::from_font_size(14.0),
                    TextColor(MUTED),
                ));
            }
        });
    // The landing prompt and the hidden/exposed banner: above the ship, clear of the HUD.
    commands.spawn((
        PadBanner,
        Text::new(""),
        TextFont::from_font_size(20.0),
        TextColor(CYAN),
        TextLayout::justify(Justify::Center),
        Node {
            position_type: PositionType::Absolute,
            width: percent(100),
            top: percent(30),
            ..default()
        },
    ));
    // The bench: a panel on the right, shown while it is open.
    commands
        .spawn((
            BenchPanelNode,
            Text::new(""),
            TextFont::from_font_size(14.0),
            Node {
                position_type: PositionType::Absolute,
                right: px(16),
                top: px(DETAILS_TOP),
                max_width: percent(94),
                padding: UiRect::axes(px(16), px(12)),
                border: UiRect::all(px(1)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.92)),
            BorderColor::all(Color::srgba(0.4, 1.0, 0.65, 0.4)),
            GlobalZIndex(12),
        ))
        .with_children(|panel| {
            for line in 0..BENCH_LINES {
                panel.spawn((
                    BenchLine(line),
                    TextSpan::new(""),
                    TextFont::from_font_size(14.0),
                    TextColor(MUTED),
                ));
            }
        });
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(250),
                right: px(28),
                top: px(150),
                justify_content: JustifyContent::Center,
                display: Display::None,
                ..default()
            },
            SummaryPanel,
        ))
        .with_children(|row| {
            row.spawn((
                Node {
                    padding: UiRect::axes(px(26), px(18)),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.012, 0.022, 0.045, 0.9)),
                BorderColor::all(Color::srgba(0.28, 0.94, 0.92, 0.45)),
                Text::new(""),
                TextFont::from_font_size(14.0),
                TextLayout::justify(Justify::Center),
            ))
            .with_children(|panel| {
                for line in 0..SUMMARY_LINES {
                    panel.spawn((
                        SummaryLine(line),
                        TextSpan::new(""),
                        TextFont::from_font_size(14.0),
                        TextColor(MUTED),
                    ));
                }
            });
        });
}
