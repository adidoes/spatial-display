mod ar_drivers {
    pub mod lib;
}
use ar_drivers::lib::{any_glasses, GlassesEvent};

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::diagnostic::LogDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::{
    mesh::Indices, render_asset::RenderAssetUsages, render_resource::PrimitiveTopology,
};
use bevy::window::{
    Monitor, MonitorSelection, PresentMode, Window, WindowLevel, WindowMode, WindowPlugin,
    WindowPosition,
};

use bevy::{
    render::camera::RenderTarget,
    window::{ExitCondition, WindowRef},
};

#[derive(Component)]
struct MonitorRef(Entity);

fn main() {
    App::new()
        .add_plugins(
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    // https://docs.rs/bevy_window/latest/bevy_window/enum.PresentMode.html
                    present_mode: PresentMode::AutoNoVsync, // AutoVsync, AutoNoVsync
                    // when using AutoVsync, add the bevy_framepace plugin and uncomment
                    // the framespace_settings lines in setup()
                    resizable: true,
                    focused: false,
                    visible: false,
                    // mode: WindowMode::Fullscreen,
                    // mode: WindowMode::Windowed,
                    window_level: WindowLevel::AlwaysOnTop,
                    mode: WindowMode::Fullscreen(MonitorSelection::Index(1)),
                    position: WindowPosition::Centered(MonitorSelection::Index(1)), // 0 is primary, 1 is secondary
                    ..default()
                }),
                ..default()
            }), // .build()
                // .disable::<bevy::input::InputPlugin>(),
        )
        // .add_plugins(DefaultPlugins.set(WindowPlugin {
        //     primary_window: None,
        //     exit_condition: ExitCondition::DontExit,
        //     ..default()
        // }))
        .insert_resource(SharedGlassesStore::new())
        // https://bevy-cheatbook.github.io/programming/schedules.html
        .add_systems(Startup, setup)
        .add_systems(Startup, create_glasses_thread)
        // .add_systems(Update, handle_keyboard_input)
        // .add_systems(Update, (update, close_on_esc))
        // You can do First/PreUpdate/Update or FixedFirst/FixedPreUpdate/FixedUpdate
        .add_systems(FixedPreUpdate, glasses_event_system)
        .insert_resource(Time::<Fixed>::from_hz(500.0)) // when using Fixed schedule
        .add_plugins((
            FrameTimeDiagnosticsPlugin,
            LogDiagnosticsPlugin::default(),
            // bevy_framepace::FramepacePlugin, // when disabling VSYNC also comment out this line
        ))
        .run();
}

fn handle_keyboard_input(
    mut commands: Commands,
    focused_windows: Query<(Entity, &Window)>,
    input: Res<ButtonInput<KeyCode>>,
) {
    for (window, focus) in focused_windows.iter() {
        if !focus.focused {
            continue;
        }

        if input.just_pressed(KeyCode::Escape) {
            commands.entity(window).despawn();
        }
    }
}

fn update(
    mut commands: Commands,
    monitors_added: Query<(Entity, &Monitor), Added<Monitor>>,
    mut monitors_removed: RemovedComponents<Monitor>,
    monitor_refs: Query<(Entity, &MonitorRef)>,
) {
    for (entity, monitor) in monitors_added.iter() {
        // Spawn a new window on each monitor
        let name = monitor.name.clone().unwrap_or_else(|| "<no name>".into());
        let size = format!("{}x{}px", monitor.physical_height, monitor.physical_width);
        let hz = monitor
            .refresh_rate_millihertz
            .map(|x| format!("{}Hz", x as f32 / 1000.0))
            .unwrap_or_else(|| "<unknown>".into());
        let position = format!(
            "x={} y={}",
            monitor.physical_position.x, monitor.physical_position.y
        );
        let scale = format!("{:.2}", monitor.scale_factor);

        let window = commands
            .spawn((
                Window {
                    title: name.clone(),
                    mode: WindowMode::Fullscreen(MonitorSelection::Entity(entity)),
                    position: WindowPosition::Centered(MonitorSelection::Entity(entity)),
                    ..default()
                },
                MonitorRef(entity),
            ))
            .id();

        let camera = commands
            .spawn((
                Camera2d,
                Camera {
                    target: RenderTarget::Window(WindowRef::Entity(window)),
                    ..default()
                },
            ))
            .id();

        let info_text = format!(
            "Monitor: {name}\nSize: {size}\nRefresh rate: {hz}\nPosition: {position}\nScale: {scale}\n\n",
        );
        commands.spawn((
            Text(info_text),
            Node {
                position_type: PositionType::Relative,
                height: Val::Percent(100.0),
                width: Val::Percent(100.0),
                ..default()
            },
            TargetCamera(camera),
            MonitorRef(entity),
        ));
    }

    // Remove windows for removed monitors
    for monitor_entity in monitors_removed.read() {
        for (ref_entity, monitor_ref) in monitor_refs.iter() {
            if monitor_ref.0 == monitor_entity {
                commands.entity(ref_entity).despawn_recursive();
            }
        }
    }
}

use dcmimu::DCMIMU;
use std::sync::Arc;
use std::sync::Mutex;
struct SharedGlassesStore {
    dcmimu: Arc<Mutex<DCMIMU>>,
}

impl SharedGlassesStore {
    pub fn new() -> Self {
        Self {
            dcmimu: Arc::new(Mutex::new(DCMIMU::new())),
        }
    }
}

impl Resource for SharedGlassesStore {}

fn create_glasses_thread(shared_glasses_store: Res<SharedGlassesStore>) {
    let shared_dcmimu_clone = Arc::clone(&shared_glasses_store.dcmimu);

    std::thread::spawn(move || {
        let mut glasses = any_glasses().unwrap();
        let mut last_timestamp: Option<u64> = None;

        use std::time::{Duration, Instant};
        let mut last_print_time = Instant::now();
        let mut loop_counter = 0;

        loop {
            if let GlassesEvent::AccGyro {
                accelerometer,
                gyroscope,
                timestamp,
            } = glasses.read_event().unwrap()
            {
                if let Some(last_timestamp) = last_timestamp {
                    let dt = (timestamp - last_timestamp) as f32 / 1_000_000.0; // in seconds

                    shared_dcmimu_clone.lock().unwrap().update(
                        (gyroscope.x, gyroscope.y, gyroscope.z),
                        (accelerometer.x, accelerometer.y, accelerometer.z),
                        // (0., 0., 0.), // set accel to 0 to disable prediction
                        dt,
                    );
                }

                last_timestamp = Some(timestamp);
            }

            loop_counter += 1;

            if last_print_time.elapsed() > Duration::from_secs(1) {
                println!("Loop has run {} times in the last second", loop_counter);
                loop_counter = 0;
                last_print_time = Instant::now();
            }
        }
    });
}

fn glasses_event_system(
    mut query: Query<&mut Transform, With<Camera>>,
    state: Res<SharedGlassesStore>,
) {
    let dcm = state.dcmimu.lock().unwrap().all();

    // println!("DCM: {:?}", dcm);

    let rot = Transform::from_rotation(Quat::from_euler(
        EulerRot::YXZ,
        dcm.yaw,
        -dcm.roll,
        dcm.pitch,
    ));

    for mut transform in query.iter_mut() {
        transform.rotation = rot.rotation;
    }
}

#[derive(Component)]
struct CustomUV;

fn setup(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    // mut framespace_settings: ResMut<bevy_framepace::FramepaceSettings>
) {
    // framespace_settings.limiter = bevy_framepace::Limiter::from_framerate(120.0);

    // Import the custom texture.
    let custom_texture_handle: Handle<Image> = asset_server.load("array_texture.png");
    // Create and save a handle to the mesh.
    let cube_mesh_handle: Handle<Mesh> = meshes.add(create_cube_mesh());

    let camera_vec = Vec3::new(0.0, 0.0, 0.0);
    let plane_vec = Vec3::new(0.0, -4.0, 0.0);
    let cube_vec = Vec3::new(0.0, 0.0, -10.0);

    // Spawn the mesh with the custom texture using Mesh3d and MeshMaterial3d
    commands.spawn((
        Mesh3d(cube_mesh_handle),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color_texture: Some(custom_texture_handle),
            ..default()
        })),
        Transform::from_translation(cube_vec),
        CustomUV,
    ));

    // Ground plane
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
        MeshMaterial3d(materials.add(Color::WHITE)),
        Transform::from_translation(plane_vec),
    ));

    // Transform for the camera and lighting.
    let camera_and_light_transform = Transform::from_translation(camera_vec);

    // Camera in 3D space
    commands.spawn((
        Camera3d::default(),
        Camera { ..default() },
        Projection::from(PerspectiveProjection {
            fov: 21.70f32.to_radians(),
            ..default()
        }),
        camera_and_light_transform,
    ));

    // Light
    commands.spawn((PointLight::default(), camera_and_light_transform));
    // Text to describe the controls.
    commands.spawn((
        Text::new("Spatial Display"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        // Set the justification of the Text
        TextLayout::new_with_justify(JustifyText::Center),
        // Set the style of the Node itself.
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

#[rustfmt::skip]
fn create_cube_mesh() -> Mesh {
    // Keep the mesh data accessible in future frames to be able to mutate it in toggle_texture.
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        // Each array is an [x, y, z] coordinate in local space.
        // Meshes always rotate around their local [0, 0, 0] when a rotation is applied to their Transform.
        // By centering our mesh around the origin, rotating the mesh preserves its center of mass.
        vec![
            // top (facing towards +y)
            [-0.5, 0.5, -0.5], // vertex with index 0
            [0.5, 0.5, -0.5], // vertex with index 1
            [0.5, 0.5, 0.5], // etc. until 23
            [-0.5, 0.5, 0.5],
            // bottom   (-y)
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, -0.5, 0.5],
            [-0.5, -0.5, 0.5],
            // right    (+x)
            [0.5, -0.5, -0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5], // This vertex is at the same position as vertex with index 2, but they'll have different UV and normal
            [0.5, 0.5, -0.5],
            // left     (-x)
            [-0.5, -0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [-0.5, 0.5, 0.5],
            [-0.5, 0.5, -0.5],
            // back     (+z)
            [-0.5, -0.5, 0.5],
            [-0.5, 0.5, 0.5],
            [0.5, 0.5, 0.5],
            [0.5, -0.5, 0.5],
            // forward  (-z)
            [-0.5, -0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [0.5, 0.5, -0.5],
            [0.5, -0.5, -0.5],
        ],
    )
    // Set-up UV coordinates to point to the upper (V < 0.5), "dirt+grass" part of the texture.
    // Take a look at the custom image (assets/textures/array_texture.png)
    // so the UV coords will make more sense
    // Note: (0.0, 0.0) = Top-Left in UV mapping, (1.0, 1.0) = Bottom-Right in UV mapping
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![
            // Assigning the UV coords for the top side.
            [0.0, 0.2], [0.0, 0.0], [1.0, 0.0], [1.0, 0.25],
            // Assigning the UV coords for the bottom side.
            [0.0, 0.45], [0.0, 0.25], [1.0, 0.25], [1.0, 0.45],
            // Assigning the UV coords for the right side.
            [1.0, 0.45], [0.0, 0.45], [0.0, 0.2], [1.0, 0.2],
            // Assigning the UV coords for the left side.
            [1.0, 0.45], [0.0, 0.45], [0.0, 0.2], [1.0, 0.2],
            // Assigning the UV coords for the back side.
            [0.0, 0.45], [0.0, 0.2], [1.0, 0.2], [1.0, 0.45],
            // Assigning the UV coords for the forward side.
            [0.0, 0.45], [0.0, 0.2], [1.0, 0.2], [1.0, 0.45],
        ],
    )
    // For meshes with flat shading, normals are orthogonal (pointing out) from the direction of
    // the surface.
    // Normals are required for correct lighting calculations.
    // Each array represents a normalized vector, which length should be equal to 1.0.
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![
            // Normals for the top side (towards +y)
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            // Normals for the bottom side (towards -y)
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            // Normals for the right side (towards +x)
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            // Normals for the left side (towards -x)
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            // Normals for the back side (towards +z)
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            // Normals for the forward side (towards -z)
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
        ],
    )
    // Create the triangles out of the 24 vertices we created.
    // To construct a square, we need 2 triangles, therefore 12 triangles in total.
    // To construct a triangle, we need the indices of its 3 defined vertices, adding them one
    // by one, in a counter-clockwise order (relative to the position of the viewer, the order
    // should appear counter-clockwise from the front of the triangle, in this case from outside the cube).
    // Read more about how to correctly build a mesh manually in the Bevy documentation of a Mesh,
    // further examples and the implementation of the built-in shapes.
    .with_inserted_indices(Indices::U32(vec![
        0,3,1 , 1,3,2, // triangles making up the top (+y) facing side.
        4,5,7 , 5,6,7, // bottom (-y)
        8,11,9 , 9,11,10, // right (+x)
        12,13,15 , 13,14,15, // left (-x)
        16,19,17 , 17,19,18, // back (+z)
        20,21,23 , 21,22,23, // forward (-z)
    ]))
}
