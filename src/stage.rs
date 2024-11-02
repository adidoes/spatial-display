use bevy::{
    asset::RenderAssetUsages,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
};
use core_graphics2::display::CGDisplay;
use rand::Rng;

use crate::ScaleFactor;

#[derive(Resource)]
pub struct AssetHandles {
    pub screen: Handle<Image>,
}

pub struct StagePlugin;

impl Plugin for StagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, (spawn_stage, spawn_screen));
    }
}

fn spawn_stage(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    info!("Spawning stage");

    // Ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::WHITE,
            ..default()
        })),
        Transform::from_xyz(0.0, -4.0, 0.0),
    ));

    // let sphere_texture = Image::new(
    //     Extent3d {
    //         width: 256,
    //         height: 256,
    //         depth_or_array_layers: 1,
    //     },
    //     TextureDimension::D2,
    //     (0..256 * 256)
    //         .flat_map(|i| {
    //             let y = (i / 256) as f32 / 256.0;
    //             let r = 255;
    //             let g = ((1.0 - y) * 255.0) as u8;
    //             let b = 0;
    //             vec![r, g, b, 255]
    //         })
    //         .collect(),
    //     TextureFormat::Rgba8UnormSrgb,
    //     RenderAssetUsages::RENDER_WORLD,
    // );

    // // info!(
    // //     "All image handles BEFORE SPHERE INSERT: {:?}",
    // //     images.ids().collect::<Vec<_>>()
    // // );
    // let sphere_texture_handle = images.add(sphere_texture);
    // // info!("SPHERE texture handle: {:?}", sphere_texture_handle);
    // // info!(
    // //     "All image handles AFTER SPHERE INSERT: {:?}",
    // //     images.ids().collect::<Vec<_>>()
    // // );
    // let sphere_material = materials.add(StandardMaterial {
    //     base_color_texture: Some(sphere_texture_handle),
    //     ..default()
    // });

    // // Test spheres in different positions
    // commands.spawn((
    //     Mesh3d(meshes.add(Sphere::default().mesh())),
    //     MeshMaterial3d(sphere_material),
    //     Transform::from_xyz(0.0, 1.0, -8.0),
    // ));
}

fn spawn_screen(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    scale_factor: Res<ScaleFactor>,
) {
    info!("Spawning screen");
    // Create initial texture with RGBA format
    let mut rng = rand::thread_rng();

    let display = CGDisplay::main();
    let width = display.pixels_wide() as u32 * scale_factor.value as u32;
    let height = display.pixels_high() as u32 * scale_factor.value as u32;

    let mut screen_texture = Image::new(
        Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        (0..(width * height * 4))
            .map(|i| {
                if i % 4 == 3 {
                    255
                } else {
                    rng.gen_range(0..=255)
                }
            })
            .collect(),
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );

    screen_texture.texture_descriptor.usage =
        TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING | TextureUsages::STORAGE_BINDING;

    // info!(
    //     "All image handles BEFORE SCREEN INSERT: {:?}",
    //     images.ids().collect::<Vec<_>>()
    // );
    let texture_handle = images.add(screen_texture);
    // let strong_handle = images.get_strong_handle(texture_handle.id()).unwrap();
    // info!("SCREEN texture handle: {:?}", texture_handle);
    // info!("STRONG texture handle: {:?}", strong_handle);
    // info!("texture_handle.is_strong: {:?}", texture_handle.is_strong());
    // info!(
    //     "All image handles AFTER SCREEN INSERT: {:?}",
    //     images.ids().collect::<Vec<_>>()
    // );
    commands.insert_resource(AssetHandles {
        screen: texture_handle.clone(),
    });

    // Create material with the screen texture
    let screen_material = materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle),
        unlit: true,                   // Make sure lighting doesn't affect the texture
        alpha_mode: AlphaMode::Opaque, // Change from Blend to Opaque
        ..default()
    });

    // screen plane
    // Scale the plane to match the texture dimensions while maintaining aspect ratio
    let plane_width = 2.5; // Adjust as needed
    let plane_height = plane_width * (height as f32 / width as f32);
    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Plane3d::new(
            Vec3::Z,
            Vec2::new(plane_width, plane_height),
        )))),
        MeshMaterial3d(screen_material),
        Transform::from_xyz(0.0, 0.0, -6.0),
    ));
}
