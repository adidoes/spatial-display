use crate::screen_capture::ScreenTextureHandle;
use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};

pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_stage, spawn_screen));
    }
}

fn spawn_stage(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            ..default()
        })),
        Transform::from_xyz(0.0, -4.0, 0.0),
    ));

    // Test spheres in different positions
    commands.spawn((
        Mesh3d(meshes.add(Sphere::default().mesh())),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(1.0, 0.0, 0.0),
            ..default()
        })),
        Transform::from_xyz(0.0, 1.0, 1.0),
    ));
}

fn spawn_screen(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    // Create initial texture with RGBA format
    let mut screen_texture = Image::new(
        Extent3d {
            width: 1800,
            height: 1169,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; 1800 * 1169 * 4],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );

    screen_texture.texture_descriptor.usage =
        TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;

    let texture_handle = images.add(screen_texture);
    commands.insert_resource(ScreenTextureHandle {
        handle: texture_handle.clone(),
    });

    // Create material with the screen texture
    let screen_material = materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle),
        unlit: true,                   // Make sure lighting doesn't affect the texture
        alpha_mode: AlphaMode::Opaque, // Change from Blend to Opaque
        ..default()
    });

    // screen plane
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Plane3d::new(Vec3::Z, Vec2::splat(5.0))))),
        MeshMaterial3d(screen_material),
        Transform::from_xyz(0.0, 0.0, -10.0),
        // Transform::from_xyz(0.0, 0.0, -10.0)
        //     .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)), // Rotate 90 degrees around X axis
    ));
}
