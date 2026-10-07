//! Draws the nebula backdrop: a few big meshes behind everything, each tiling one generated noise
//! texture and tinted per vertex with what `ssc::backdrop::backdrop_at` says about the place
//! under it. All the choices (colour, amount, grain) are made in the headless library; this
//! file only builds meshes and moves them. Cost: a handful of overdrawn low-alpha quads and
//! about a hundred cached lookups per frame.

use crate::Session;
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageSampler, ImageSamplerDescriptor},
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
    sprite_render::{AlphaMode2d, ColorMaterial},
};
use ssc::backdrop::{self, Backdrop, Grain, TEXTURE_SIZE};

/// Vertices across and down each mesh; colours are sampled at every vertex.
const COLS: usize = 15;
const ROWS: usize = 9;
/// The meshes are a little larger than the view so edges never show.
const MARGIN: f32 = 1.06;

/// One cloud layer: which texture, how fast it drifts against the camera (1 is as fast as the
/// world), the world size of one texture tile, the texture's turn and its draw order.
struct Spec {
    kind: Kind,
    parallax: f32,
    tile: f32,
    turn: f32,
    z: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    FarWisp,
    MidWisp,
    NearGrit,
}

const SPECS: [Spec; 3] = [
    Spec {
        kind: Kind::FarWisp,
        parallax: 0.2,
        tile: 3600.0,
        turn: 0.0,
        z: -30.0,
    },
    Spec {
        kind: Kind::MidWisp,
        parallax: 0.45,
        tile: 2300.0,
        turn: 0.9,
        z: -29.0,
    },
    Spec {
        kind: Kind::NearGrit,
        parallax: 0.8,
        tile: 700.0,
        turn: 0.4,
        z: -28.0,
    },
];

#[derive(Component)]
pub struct Cloud(usize);

#[derive(Resource)]
pub struct Clouds {
    meshes: Vec<Handle<Mesh>>,
}

fn grain_image(grain: Grain) -> Image {
    let size = TEXTURE_SIZE;
    let coverage = backdrop::texture(grain, size);
    let mut data = Vec::with_capacity(size * size * 4);
    for a in coverage {
        data.extend_from_slice(&[255, 255, 255, a]);
    }
    let mut image = Image::new(
        Extent3d {
            width: size as u32,
            height: size as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        ..ImageSamplerDescriptor::linear()
    });
    image
}

fn grid_mesh() -> Mesh {
    let mut positions = Vec::with_capacity(COLS * ROWS);
    for r in 0..ROWS {
        for c in 0..COLS {
            positions.push([
                c as f32 / (COLS - 1) as f32 * 2.0 - 1.0,
                r as f32 / (ROWS - 1) as f32 * 2.0 - 1.0,
                0.0,
            ]);
        }
    }
    let mut indices = Vec::new();
    for r in 0..ROWS - 1 {
        for c in 0..COLS - 1 {
            let i = (r * COLS + c) as u32;
            let (right, up) = (i + 1, i + COLS as u32);
            indices.extend_from_slice(&[i, right, up, right, up + 1, up]);
        }
    }
    let n = positions.len();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32; 2]; n])
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0f32; 4]; n])
    .with_inserted_indices(Indices::U32(indices))
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut handles = Vec::new();
    for (index, spec) in SPECS.iter().enumerate() {
        let grain = match spec.kind {
            Kind::FarWisp | Kind::MidWisp => Grain::Wisp,
            Kind::NearGrit => Grain::Grit,
        };
        let texture = images.add(grain_image(grain));
        let material = materials.add(ColorMaterial {
            color: Color::WHITE,
            alpha_mode: AlphaMode2d::Blend,
            texture: Some(texture),
            ..default()
        });
        let mesh = meshes.add(grid_mesh());
        handles.push(mesh.clone());
        commands.spawn((
            Cloud(index),
            Mesh2d(mesh),
            MeshMaterial2d(material),
            Transform::from_xyz(0.0, 0.0, spec.z),
        ));
    }
    commands.insert_resource(Clouds { meshes: handles });
}

type ViewParam<'w, 's> =
    Single<'w, 's, (&'static Transform, &'static Projection), (With<Camera2d>, Without<Cloud>)>;

/// Moves the clouds with the camera and refreshes their colours and texture coordinates.
pub fn update(
    session: Res<Session>,
    clouds: Res<Clouds>,
    view: ViewParam,
    mut layers: Query<(&Cloud, &mut Transform, &mut Visibility), Without<Camera2d>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let camera = view.0.translation.truncate();
    let half = match view.1 {
        Projection::Orthographic(p) => p.area.half_size(),
        _ => Vec2::new(900.0, 450.0),
    } * MARGIN;
    let seed = session.game.seed();
    for (Cloud(index), mut transform, mut visibility) in &mut layers {
        *visibility = if session.reduce_effects {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
        if session.reduce_effects {
            continue;
        }
        let spec = &SPECS[*index];
        transform.translation = camera.extend(spec.z);
        transform.scale = half.extend(1.0);
        let Some(mut mesh) = meshes.get_mut(&clouds.meshes[*index]) else {
            continue;
        };
        let turn = Vec2::from_angle(spec.turn);
        let rotate = |v: Vec2| Vec2::new(turn.x * v.x - turn.y * v.y, turn.y * v.x + turn.x * v.y);
        // The layer drifts at `parallax` of the camera's speed. The big offset is reduced in
        // f64 so far-away worlds do not lose texture precision.
        let shifted = rotate(camera * spec.parallax / spec.tile);
        let offset = Vec2::new(
            (shifted.x as f64).rem_euclid(1.0) as f32,
            (shifted.y as f64).rem_euclid(1.0) as f32,
        );
        let mut uvs = Vec::with_capacity(COLS * ROWS);
        let mut colors = Vec::with_capacity(COLS * ROWS);
        for r in 0..ROWS {
            for c in 0..COLS {
                let local = Vec2::new(
                    c as f32 / (COLS - 1) as f32 * 2.0 - 1.0,
                    r as f32 / (ROWS - 1) as f32 * 2.0 - 1.0,
                );
                let world = camera + local * half;
                uvs.push((offset + rotate(local * half / spec.tile)).to_array());
                let look: Backdrop = backdrop::backdrop_at(seed, world);
                let l = look.layers();
                let rgba = match spec.kind {
                    Kind::FarWisp => l.far_wisp,
                    Kind::MidWisp => l.mid_wisp,
                    Kind::NearGrit => l.near_grit,
                };
                let linear = Color::srgb(rgba[0], rgba[1], rgba[2]).to_linear();
                colors.push([linear.red, linear.green, linear.blue, rgba[3]]);
            }
        }
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    }
}
