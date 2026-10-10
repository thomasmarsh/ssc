//! A bounded, zoomable chart built from vector UI shapes and remembered sector data.
//! The adapter owns selection and view scale; discovery and travel remain simulation rules.
use crate::{
    Session,
    presentation::{CYAN, ChartSpan, MUTED},
};
use bevy::{input::mouse::MouseWheel, prelude::*, ui::FocusPolicy};
use ssc::{
    backdrop,
    simulation::{ChartEntry, ChartGeometry, ChartGeometryKind},
    world::{SECTOR_SIZE, SectorId},
};
use std::collections::BTreeMap;

const SCALES: [(i32, i32); 6] = [(1, 1), (3, 3), (7, 5), (11, 9), (17, 13), (25, 19)];
const CELLS: usize = 25 * 19;
const INK: Color = Color::srgb(0.82, 0.88, 0.95);
const BLUE: Color = CYAN;
const GREEN: Color = Color::srgb(0.29, 0.87, 0.50);
const GOLD: Color = Color::srgb(0.96, 0.77, 0.26);
const PURPLE: Color = Color::srgb(0.72, 0.58, 0.96);
const RED: Color = Color::srgb(0.95, 0.36, 0.40);

#[derive(Component)]
pub(crate) struct ChartRoot;
#[derive(Component)]
pub(crate) struct Cell(usize);
#[derive(Component)]
pub(crate) struct Marker {
    cell: usize,
    kind: usize,
}
#[derive(Component)]
pub(crate) struct Zoom(i32);
#[derive(Component)]
pub(crate) struct ScaleLabel;
#[derive(Component)]
pub(crate) struct ChartScroll;
#[derive(Component)]
pub(crate) struct CellLabel(usize);
#[derive(Component)]
pub(crate) struct MapViewport;
#[derive(Component)]
pub(crate) struct MapArea;
#[derive(Component)]
pub(crate) struct GeometryNode(ChartGeometry);
#[derive(Default)]
pub(crate) struct GeometryCache {
    snapshot: Option<(u64, SectorId, usize, Vec<ChartEntry>)>,
}

fn sector(center: SectorId, index: usize, cols: i32, rows: i32) -> SectorId {
    SectorId {
        x: center.x + index as i32 % cols - cols / 2,
        y: center.y + rows / 2 - index as i32 / cols,
    }
}

/// The same shapes appear in the legend and the map. Their size is in UI pixels, so they
/// remain readable as the sector scale changes. Offset markers never obscure the ship.
fn marker(kind: usize) -> (Node, BackgroundColor, BorderColor, UiTransform, FocusPolicy) {
    let (x, y, size, color, round, hollow, turn) = match kind {
        0 => (
            25.0,
            35.0,
            12.0,
            Color::srgb(0.76, 0.65, 0.54),
            true,
            false,
            false,
        ),
        1 => (72.0, 35.0, 13.0, PURPLE, true, true, false),
        2 => (50.0, 30.0, 12.0, GOLD, false, true, false),
        3 => (25.0, 72.0, 9.0, GREEN, false, true, false),
        4 => (75.0, 72.0, 10.0, BLUE, true, true, false),
        5 => (85.0, 16.0, 6.0, GOLD, false, false, true),
        6 => (50.0, 72.0, 8.0, RED, false, true, true),
        7 => (50.0, 53.0, 11.0, INK, false, false, true),
        8 => (14.0, 16.0, 5.0, RED, true, false, false),
        _ => (
            72.0,
            52.0,
            7.0,
            Color::srgb(0.88, 0.25, 0.60),
            false,
            true,
            true,
        ),
    };
    (
        Node {
            position_type: PositionType::Absolute,
            left: percent(x),
            top: percent(y),
            width: px(size),
            height: px(size),
            margin: UiRect {
                left: px(-size / 2.0),
                top: px(-size / 2.0),
                ..default()
            },
            border: UiRect::all(px(if hollow { 2.0 } else { 0.0 })),
            border_radius: if round {
                BorderRadius::MAX
            } else {
                BorderRadius::ZERO
            },
            ..default()
        },
        BackgroundColor(if hollow { Color::NONE } else { color }),
        BorderColor::all(color),
        UiTransform::from_rotation(Rot2::radians(if turn {
            std::f32::consts::FRAC_PI_4
        } else {
            0.0
        })),
        FocusPolicy::Pass,
    )
}

pub fn setup(mut commands: Commands) {
    commands.spawn((
        ChartRoot,
        GlobalZIndex(20),
        Node {
            position_type: PositionType::Absolute,
            left: percent(3), right: percent(3), top: percent(8), bottom: percent(5),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(px(16)), row_gap: px(12),
            border: UiRect::all(px(1)), display: Display::None,
            ..default()
        },
        BackgroundColor(Color::srgb(0.012, 0.022, 0.045)),
        BorderColor::all(CYAN.with_alpha(0.45)),
    )).with_children(|root| {
        root.spawn(Node { align_items: AlignItems::Center, flex_wrap: FlexWrap::Wrap, row_gap: px(6), column_gap: px(12), ..default() })
            .with_children(|bar| {
                bar.spawn((Text::new("STAR MAP"), TextFont::from_font_size(14.0), TextColor(CYAN)));
                bar.spawn((ScaleLabel, Text::new(""), TextFont::from_font_size(14.0), TextColor(BLUE)));
                for (by, label) in [(-1, "+"), (1, "-")] {
                    bar.spawn((Button, Zoom(by), Node { padding: UiRect::axes(px(12), px(4)), border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::srgb(0.08, 0.10, 0.15)), BorderColor::all(BLUE)))
                        .with_children(|b| { b.spawn((Text::new(label), TextFont::from_font_size(14.0), TextColor(INK), FocusPolicy::Pass)); });
                }
                bar.spawn((Text::new("Scroll / + - zoom   |   Click selects   |   Arrows pan   |   Z ship   |   G close"), TextFont::from_font_size(13.0), TextColor(MUTED)));
            });
        root.spawn(Node { flex_grow: 1.0, min_height: px(0), column_gap: px(20), ..default() })
            .with_children(|body| {
                body.spawn((MapArea, Node { width: percent(68), height: percent(100), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() }))
                    .with_children(|area| {
                    area.spawn((MapViewport, Node { overflow: Overflow::clip(), ..default() }, BackgroundColor(Color::srgb(0.027, 0.035, 0.051))))
                    .with_children(|map| {
                        for index in 0..CELLS {
                            map.spawn((Button, Cell(index), Node { position_type: PositionType::Absolute, border: UiRect::all(px(1)), ..default() }, BackgroundColor(Color::NONE), BorderColor::all(Color::NONE)))
                                .with_children(|cell| {
                                    for kind in 0..10 { if kind != 0 && kind != 2 { cell.spawn((Marker { cell: index, kind }, marker(kind))); } }
                                    cell.spawn((CellLabel(index), Text::new(""), TextFont::from_font_size(10.0), TextColor(INK.with_alpha(0.55)), Node { position_type: PositionType::Absolute, left: px(5), bottom: px(3), ..default() }, FocusPolicy::Pass));
                                });
                        }
                    });
                    });
                body.spawn((Node { flex_grow: 1.0, flex_basis: px(0), min_width: px(0), height: percent(100), overflow: Overflow::scroll_y(), padding: UiRect::right(px(8)), ..default() }, ScrollPosition::default(), ChartScroll))
                    .with_children(|side| {
                        side.spawn((Text::new(""), TextFont::from_font_size(14.0), TextColor(INK)))
                            .with_children(|text| {
                                for i in 0..16 { text.spawn((ChartSpan(i), TextSpan::new(""), TextFont::from_font_size(14.0), TextColor(INK))); }
                            });
                    });
            });
        root.spawn(Node { flex_wrap: FlexWrap::Wrap, column_gap: px(18), row_gap: px(8), ..default() })
            .with_children(|legend| {
                for (kind, label) in ["Planetoid / lode", "Well", "Civilization", "Pad", "Beacon", "Pin", "Wreck", "Ship", "Danger", "Relic"].iter().enumerate() {
                    legend.spawn(Node { align_items: AlignItems::Center, column_gap: px(5), ..default() })
                        .with_children(|item| {
                            item.spawn(Node { width: px(20), height: px(20), ..default() }).with_children(|icon| { icon.spawn(marker(kind)); });
                            item.spawn((Text::new(*label), TextFont::from_font_size(13.0), TextColor(INK)));
                        });
                }
                legend.spawn((Text::new("Green planet rim: renewable   |   Color: local nebula   |   Dark: uncharted   |   North is up"), TextFont::from_font_size(13.0), TextColor(MUTED)));
            });
    });
}

type ScaleFilter = (With<ScaleLabel>, Without<CellLabel>);
type RootFilter = (
    With<ChartRoot>,
    Without<Cell>,
    Without<Marker>,
    Without<MapViewport>,
    Without<GeometryNode>,
);
type ViewFilter = (
    With<MapViewport>,
    Without<Cell>,
    Without<Marker>,
    Without<ChartRoot>,
    Without<GeometryNode>,
);

type CellQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Cell,
        &'static Interaction,
        &'static mut Node,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (Without<Marker>, Without<MapViewport>, Without<GeometryNode>),
>;
type MarkerQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static Marker,
        &'static mut Node,
        &'static mut BackgroundColor,
        &'static mut BorderColor,
    ),
    (Without<Cell>, Without<MapViewport>, Without<GeometryNode>),
>;

#[allow(clippy::too_many_arguments)]
pub fn update(
    area: Single<&ComputedNode, With<MapArea>>,
    mut session: ResMut<Session>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    mut wheel: MessageReader<MouseWheel>,
    mut root: Single<&mut Node, RootFilter>,
    mut cells: CellQuery,
    mut viewport: Single<(Entity, &ComputedNode, &mut Node), ViewFilter>,
    mut markers: MarkerQuery,
    zoom_buttons: Query<(&Zoom, &Interaction), Changed<Interaction>>,
    mut label: Single<&mut Text, ScaleFilter>,
    mut cell_labels: Query<(&CellLabel, &mut Text), Without<ScaleLabel>>,
    mut scroll: Query<&mut ScrollPosition, With<ChartScroll>>,
) {
    let delta: f32 = wheel.read().map(|e| e.y).sum();
    root.display = if session.chart.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    let Some(mut cursor) = session.chart else {
        return;
    };
    let mut zoom = 0;
    for pad in &gamepads {
        if pad.just_pressed(GamepadButton::RightTrigger2) {
            zoom -= 1;
        }
        if pad.just_pressed(GamepadButton::LeftTrigger2) {
            zoom += 1;
        }
    }
    if keys.just_pressed(KeyCode::Equal) || keys.just_pressed(KeyCode::NumpadAdd) {
        zoom -= 1;
    }
    if keys.just_pressed(KeyCode::Minus) || keys.just_pressed(KeyCode::NumpadSubtract) {
        zoom += 1;
    }
    let map_hovered = cells.iter().any(|(_, interaction, node, _, _)| {
        node.display != Display::None && *interaction != Interaction::None
    });
    if delta != 0.0 {
        if map_hovered {
            zoom -= delta.signum() as i32;
        } else {
            for mut pos in &mut scroll {
                pos.0.y = (pos.0.y - delta * 24.0).max(0.0);
            }
        }
    }
    for (button, interaction) in &zoom_buttons {
        if *interaction == Interaction::Pressed {
            zoom += button.0;
        }
    }
    // Clicks use the layout from the previous frame, before applying a new zoom.
    let (cols, rows) = SCALES[cursor.zoom];
    for (cell, interaction, node, _, _) in &cells {
        if mouse.just_pressed(MouseButton::Left)
            && node.display != Display::None
            && *interaction == Interaction::Pressed
        {
            cursor.sector = sector(cursor.center, cell.0, cols, rows);
        }
    }
    cursor.zoom = (cursor.zoom as i32 + zoom).clamp(0, SCALES.len() as i32 - 1) as usize;
    let (cols, rows) = SCALES[cursor.zoom];
    label.0 = format!("{} x {} sectors", cols, rows);
    // Use one world-to-pixel scale on both axes: planets stay circular and distances agree.
    let available = area.size() * area.inverse_scale_factor;
    let tile_size = (available / Vec2::new(cols as f32, rows as f32)).min_element();
    let map_size = Vec2::new(cols as f32, rows as f32) * tile_size;
    viewport.2.width = px(map_size.x);
    viewport.2.height = px(map_size.y);
    let icon_scale = (tile_size / 52.0).clamp(0.3, 1.0);
    let entries: BTreeMap<_, _> = session
        .game
        .chart_entries()
        .into_iter()
        .map(|e| (e.sector, e))
        .collect();
    let here = session.game.sector();
    for (cell, interaction, mut node, mut fill, mut border) in &mut cells {
        if cell.0 >= (cols * rows) as usize {
            node.display = Display::None;
            continue;
        }
        node.display = Display::Flex;
        node.left = percent((cell.0 as i32 % cols) as f32 * 100.0 / cols as f32);
        node.top = percent((cell.0 as i32 / cols) as f32 * 100.0 / rows as f32);
        node.width = percent(100.0 / cols as f32);
        node.height = percent(100.0 / rows as f32);
        let id = sector(cursor.center, cell.0, cols, rows);
        let entry = entries.get(&id);
        fill.0 = if let Some(e) = entry.filter(|e| {
            e.visited || (0..10).any(|kind| kind != 5 && kind != 6 && visible(kind, Some(e), false))
        }) {
            let sky = backdrop::backdrop_at(session.game.seed(), id.center());
            let strength = if e.visited { 0.44 } else { 0.24 };
            let [r, g, b] = sky.tint;
            Color::srgb(
                0.035 + r * strength,
                0.043 + g * strength,
                0.065 + b * strength,
            )
        } else {
            Color::srgb(0.027, 0.035, 0.051)
        };
        border.set_all(if id == cursor.sector {
            BLUE
        } else if *interaction == Interaction::Hovered {
            INK.with_alpha(0.5)
        } else if let Some(civ) = entry.and_then(|e| e.civ).filter(|c| !c.fallen) {
            let [r, g, b] = civ.tint;
            Color::srgba(r, g, b, 0.42)
        } else {
            Color::srgb(0.075, 0.085, 0.11)
        });
        node.border = UiRect::all(px(if id == cursor.sector { 2.0 } else { 1.0 }));
    }
    for (mark, mut node, _, _) in &mut markers {
        let id = sector(cursor.center, mark.cell, cols, rows);
        let entry = entries.get(&id);
        let show = mark.cell < (cols * rows) as usize
            && mark.kind != 0
            && mark.kind != 2
            && visible(mark.kind, entry, id == here);
        node.display = if show { Display::Flex } else { Display::None };
        let base = marker(mark.kind).0;
        let size = match base.width {
            Val::Px(size) => size * icon_scale,
            _ => 8.0,
        };
        node.left = base.left;
        node.top = base.top;
        let position = match mark.kind {
            7 => session.game.player().map(|p| p.position),
            3 => session
                .game
                .pads()
                .map(|p| session.game.pad_position(p))
                .find(|p| SectorId::containing(*p) == id),
            4 => session
                .game
                .beacons()
                .iter()
                .find(|b| SectorId::containing(b.position) == id)
                .map(|b| b.position),
            _ => None,
        };
        if let Some(position) = position {
            let offset = (position - id.center()) / SECTOR_SIZE;
            node.left = percent(50.0 + offset.x * 100.0);
            node.top = percent(50.0 - offset.y * 100.0);
        }
        node.width = px(size);
        node.height = px(size);
        node.border = UiRect::all(px(match base.border.left {
            Val::Px(width) if width > 0.0 => (width * icon_scale).max(1.0),
            _ => 0.0,
        }));
        node.margin = UiRect {
            left: px(-size / 2.0),
            top: px(-size / 2.0),
            ..default()
        };
    }
    for (cell, mut text) in &mut cell_labels {
        let id = sector(cursor.center, cell.0, cols, rows);
        text.0 = if cell.0 >= (cols * rows) as usize || tile_size < 36.0 {
            String::new()
        } else if id == SectorId::ORIGIN {
            "HOME".into()
        } else if id == here {
            "YOU".into()
        } else if cursor.zoom < 4 && (entries.contains_key(&id) || id == cursor.sector) {
            format!("{},{}", id.x, id.y)
        } else {
            String::new()
        };
    }
    session.chart = Some(cursor);
}

type GeometryFilter = (
    With<GeometryNode>,
    Without<Cell>,
    Without<Marker>,
    Without<ChartRoot>,
    Without<MapViewport>,
);
type GeometryQuery<'w, 's> =
    Query<'w, 's, (Entity, &'static GeometryNode, &'static mut Node), GeometryFilter>;

pub fn update_geometry(
    mut commands: Commands,
    session: Res<Session>,
    mut cache: Local<GeometryCache>,
    mut geometry: GeometryQuery,
    viewport: Single<(Entity, &ComputedNode), With<MapViewport>>,
) {
    let Some(cursor) = session.chart else {
        cache.snapshot = None;
        return;
    };
    let (cols, rows) = SCALES[cursor.zoom];
    let tile_size = viewport.1.size().x * viewport.1.inverse_scale_factor / cols as f32;
    let entries: BTreeMap<_, _> = session
        .game
        .chart_entries()
        .into_iter()
        .filter(|e| {
            e.sector.x.abs_diff(cursor.center.x) <= (cols / 2 + 1) as u32
                && e.sector.y.abs_diff(cursor.center.y) <= (rows / 2 + 1) as u32
        })
        .map(|e| (e.sector, e))
        .collect();
    let snapshot = (
        session.game.seed(),
        cursor.center,
        cursor.zoom,
        entries.values().cloned().collect::<Vec<_>>(),
    );
    if cache.snapshot.as_ref() != Some(&snapshot) {
        for (entity, _, _) in &geometry {
            commands.entity(entity).despawn();
        }
        for id in entries.keys() {
            for site in session.game.chart_geometry(*id) {
                let color = Color::srgb(site.tint[0], site.tint[1], site.tint[2]);
                let planet = matches!(
                    site.kind,
                    ChartGeometryKind::Planetoid | ChartGeometryKind::Lode
                );
                let node = geometry_node(site, cursor.center, cols, rows, tile_size);
                commands.entity(viewport.0).with_children(|map| {
                    map.spawn((
                        GeometryNode(site),
                        node,
                        BackgroundColor(if planet {
                            color.with_alpha(0.35)
                        } else {
                            Color::NONE
                        }),
                        BorderColor::all(if site.renewable { GREEN } else { color }),
                        FocusPolicy::Pass,
                    ));
                });
            }
        }
        cache.snapshot = Some(snapshot);
    }
    for (_, site, mut node) in &mut geometry {
        *node = geometry_node(site.0, cursor.center, cols, rows, tile_size);
    }
}

/// A literal disc or structural footprint: no minimum radius or artificial sector offset.
fn geometry_node(site: ChartGeometry, center: SectorId, cols: i32, rows: i32, tile: f32) -> Node {
    let delta = (site.position - center.center()) / SECTOR_SIZE;
    let radius = site.radius / SECTOR_SIZE * tile;
    let round = matches!(
        site.kind,
        ChartGeometryKind::Planetoid
            | ChartGeometryKind::Lode
            | ChartGeometryKind::Wall
            | ChartGeometryKind::Turret
    );
    Node {
        position_type: PositionType::Absolute,
        left: px((cols as f32 * 0.5 + delta.x) * tile - radius),
        top: px((rows as f32 * 0.5 - delta.y) * tile - radius),
        width: px(radius * 2.0),
        height: px(radius * 2.0),
        border: UiRect::all(px((radius * 0.18).clamp(0.25, 1.0))),
        border_radius: if round {
            BorderRadius::MAX
        } else {
            BorderRadius::ZERO
        },
        ..default()
    }
}

fn visible(kind: usize, entry: Option<&ChartEntry>, ship: bool) -> bool {
    if kind == 7 {
        return ship;
    }
    let Some(e) = entry else {
        return false;
    };
    match kind {
        0 => e.planetoids > 0 || e.lodes > 0,
        1 => e.dynamic_wells > 0,
        2 => e.civ.is_some(),
        3 => e.pads > 0,
        4 => e.beacons > 0,
        5 => e.pin.is_some(),
        6 => e.wreck,
        8 => e.predators.is_some_and(|n| n > 0) || e.nests > 0,
        9 => e.relics > 0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ChartCursor;
    use ssc::simulation::PinLabel;

    fn chart_app() -> App {
        let mut app = App::new();
        let session = Session {
            chart: Some(ChartCursor {
                sector: SectorId::ORIGIN,
                center: SectorId::ORIGIN,
                label: PinLabel::Camp,
                zoom: 3,
            }),
            ..default()
        };
        app.insert_resource(session)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .add_message::<MouseWheel>()
            .add_systems(Startup, setup)
            .add_systems(Update, (update, update_geometry).chain());
        app.update();
        app
    }

    #[test]
    fn selection_keeps_view_fixed_and_zoom_is_bounded() {
        let mut app = chart_app();
        assert!(
            app.world_mut()
                .query::<(&ChartSpan, &TextFont)>()
                .iter(app.world())
                .all(|(_, font)| font.font_size == bevy::text::FontSize::Px(14.0))
        );
        let cell = app
            .world_mut()
            .query::<(Entity, &Cell)>()
            .iter(app.world())
            .find(|(_, cell)| cell.0 == 0)
            .unwrap()
            .0;
        *app.world_mut().get_mut::<Interaction>(cell).unwrap() = Interaction::Pressed;
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .press(MouseButton::Left);
        app.update();
        let cursor = app.world().resource::<Session>().chart.unwrap();
        assert_eq!(cursor.sector, SectorId { x: -5, y: 4 });
        assert_eq!(cursor.center, SectorId::ORIGIN);
        app.world_mut()
            .resource_mut::<ButtonInput<MouseButton>>()
            .clear();
        // A held interaction must not reselect a different sector after zooming.
        for _ in 0..8 {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::Minus);
            app.update();
        }
        let cursor = app.world().resource::<Session>().chart.unwrap();
        assert_eq!(cursor.zoom, SCALES.len() - 1);
        assert_eq!(cursor.sector, SectorId { x: -5, y: 4 });
        for _ in 0..8 {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.reset_all();
            keys.press(KeyCode::Equal);
            app.update();
        }
        assert_eq!(app.world().resource::<Session>().chart.unwrap().zoom, 0);
        let shown = app
            .world_mut()
            .query::<(&Cell, &Node)>()
            .iter(app.world())
            .filter(|(_, node)| node.display != Display::None)
            .count();
        assert_eq!(shown, 1);
    }

    #[test]
    fn literal_geometry_preserves_positions_and_dimensions_at_each_scale() {
        let site = ChartGeometry {
            position: Vec2::new(1200.0, -900.0),
            radius: 350.0,
            kind: ChartGeometryKind::Planetoid,
            tint: backdrop::ROCK,
            renewable: true,
        };
        for (cols, rows) in SCALES {
            let tile = 90.0;
            let node = geometry_node(site, SectorId::ORIGIN, cols, rows, tile);
            let Val::Px(width) = node.width else {
                panic!("pixel width");
            };
            let Val::Px(height) = node.height else {
                panic!("pixel height");
            };
            let Val::Px(left) = node.left else {
                panic!("pixel x");
            };
            let Val::Px(top) = node.top else {
                panic!("pixel y");
            };
            assert_eq!(width, height);
            assert!((width / tile * SECTOR_SIZE - 2.0 * site.radius).abs() < 0.001);
            let world = Vec2::new(
                (left + width / 2.0) / tile - cols as f32 / 2.0,
                rows as f32 / 2.0 - (top + height / 2.0) / tile,
            ) * SECTOR_SIZE;
            assert!(world.distance(site.position) < 0.01);
        }
    }

    #[test]
    fn uncharted_space_has_no_site_markers_and_closing_hides_map() {
        let mut app = chart_app();
        app.world_mut()
            .resource_mut::<Session>()
            .chart
            .as_mut()
            .unwrap()
            .center = SectorId { x: 100, y: 100 };
        app.update();
        assert!(
            app.world_mut()
                .query::<(&Marker, &Node)>()
                .iter(app.world())
                .all(|(_, node)| node.display == Display::None)
        );
        app.world_mut().resource_mut::<Session>().chart = None;
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<&Node, With<ChartRoot>>()
                .single(app.world())
                .unwrap()
                .display,
            Display::None
        );
    }
}
