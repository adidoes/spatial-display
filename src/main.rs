mod ar_drivers {
    pub mod lib;
}
use ar_drivers::lib::{any_glasses, GlassesEvent};

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::diagnostic::LogDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
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

use bevy::render::render_resource::{TextureFormat, TextureUsages};
use core_foundation::base::TCFType;
use core_media::sample_buffer::{CMSampleBuffer, CMSampleBufferRef};
use core_video::pixel_buffer::kCVPixelBufferLock_ReadOnly;
use core_video::pixel_buffer::kCVPixelFormatType_32BGRA;
use core_video::pixel_buffer::CVPixelBuffer;
use core_video::pixel_buffer::CVPixelBufferLockFlags;
use dispatch2::{Queue, QueueAttribute};
use objc2::{
    declare_class, extern_methods, msg_send_id, mutability,
    rc::{Allocated, Id},
    runtime::ProtocolObject,
    ClassType, DeclaredClass,
};
use objc2_foundation::NSArray;
use objc2_foundation::NSError;
use objc2_foundation::NSObject;
use objc2_foundation::NSObjectProtocol;
use screen_capture_kit::stream::SCContentFilter;
use screen_capture_kit::stream::SCStreamDelegate;
use screen_capture_kit::stream::SCStreamOutput;
use screen_capture_kit::stream::SCStreamOutputType;
use screen_capture_kit::{
    shareable_content::SCShareableContent,
    stream::{SCFrameStatus, SCStream, SCStreamConfiguration},
};

use std::sync::{Arc, Mutex};

// Shared state between capture thread and bevy
#[derive(Debug)]
struct SharedFrameData {
    width: u32,
    height: u32,
    buffer: Vec<u8>,
    new_frame: bool,
}

// Resource to hold the texture handle
#[derive(Resource)]
struct ScreenTexture {
    handle: Handle<Image>,
}

// Resource to hold shared frame data
#[derive(Resource)]
struct FrameDataResource {
    shared_data: Arc<Mutex<SharedFrameData>>,
}

fn main() {
    // Initialize shared frame data
    let shared_data = Arc::new(Mutex::new(SharedFrameData {
        width: 0,
        height: 0,
        buffer: Vec::new(),
        new_frame: false,
    }));

    // Spawn capture thread
    let capture_data = shared_data.clone();
    std::thread::spawn(move || {
        capture_screen(capture_data);
    });

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
        .insert_resource(FrameDataResource { shared_data })
        .insert_resource(SharedGlassesStore::new())
        // https://bevy-cheatbook.github.io/programming/schedules.html
        .add_systems(Startup, setup)
        .add_systems(Startup, create_glasses_thread)
        .add_systems(Update, update_texture)
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
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    // mut framespace_settings: ResMut<bevy_framepace::FramepaceSettings>
) {
    // framespace_settings.limiter = bevy_framepace::Limiter::from_framerate(120.0);

    // Create initial texture
    let mut initial_texture = Image::new_fill(
        Extent3d {
            width: 1920,  // Set to your capture resolution
            height: 1080, // Set to your capture resolution
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255], // Initial black pixel
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::RENDER_WORLD,
    );

    // Important: Set the texture as filterable in sampler
    initial_texture.texture_descriptor.usage =
        TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;

    // Store the texture handle
    let texture_handle = images.add(initial_texture);
    commands.insert_resource(ScreenTexture {
        handle: texture_handle.clone(),
    });

    let camera_vec = Vec3::new(0.0, 0.0, 0.0);
    let plane_vec = Vec3::new(0.0, -4.0, 0.0);
    let screen_vec = Vec3::new(0.0, 0.0, -10.0);

    // Create a quad mesh for the screen
    let quad_handle = meshes.add(Mesh::from(Plane3d::new(Vec3::Z, Vec2::splat(0.5))));

    // Create material with the screen texture
    let screen_material = materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle),
        // unlit: true, // Make the material unlit so it's not affected by lighting
        alpha_mode: AlphaMode::Blend,
        ..default()
    });

    // Spawn the screen quad
    // commands.spawn((MaterialMeshBundle {
    //     mesh: quad_handle,
    //     material: screen_material,
    //     transform: Transform::from_translation(screen_vec).with_scale(Vec3::new(16.0, 9.0, 1.0)), // Adjust scale to match aspect ratio
    //     ..default()
    // },));
    commands.spawn((
        Mesh3d(quad_handle.clone()),
        MeshMaterial3d(screen_material),
        Transform::from_translation(screen_vec).with_scale(Vec3::new(16.0, 9.0, 1.0)),
    ));

    // Ground plane (optional - you can remove if not needed)
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(30.0, 30.0))),
        MeshMaterial3d(materials.add(Color::WHITE)),
        Transform::from_translation(plane_vec),
    ));

    // Camera in 3D space
    commands.spawn((
        Camera3d::default(),
        Camera { ..default() },
        Projection::from(PerspectiveProjection {
            fov: 21.70f32.to_radians(),
            ..default()
        }),
        Transform::from_translation(camera_vec),
    ));

    // Light (optional - since we're using unlit material)
    commands.spawn(PointLight::default());

    // Text overlay
    commands.spawn((
        Text::new("Spatial Display"),
        TextFont {
            font_size: 20.0,
            ..default()
        },
        TextLayout::new_with_justify(JustifyText::Center),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(12.0),
            left: Val::Px(12.0),
            ..default()
        },
    ));
}

fn update_texture(
    frame_data: Res<FrameDataResource>,
    screen_texture: Res<ScreenTexture>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut shared = frame_data.shared_data.lock().unwrap();
    if shared.new_frame {
        println!(
            "📥 Received new frame: {}x{} with buffer size: {}",
            shared.width,
            shared.height,
            shared.buffer.len()
        );

        if let Some(texture) = images.get_mut(&screen_texture.handle) {
            if texture.width() != shared.width || texture.height() != shared.height {
                println!(
                    "🔄 Resizing texture from {}x{} to {}x{}",
                    texture.width(),
                    texture.height(),
                    shared.width,
                    shared.height
                );
                *texture = Image::new(
                    Extent3d {
                        width: shared.width,
                        height: shared.height,
                        depth_or_array_layers: 1,
                    },
                    TextureDimension::D2,
                    shared.buffer.clone(),
                    TextureFormat::Rgba8UnormSrgb,
                    RenderAssetUsages::RENDER_WORLD,
                );
                texture.texture_descriptor.usage =
                    TextureUsages::COPY_DST | TextureUsages::TEXTURE_BINDING;
            } else {
                texture.data = shared.buffer.clone();
                println!("✅ Updated texture data");
            }
        } else {
            println!("❌ Failed to get texture from handle");
        }
        shared.new_frame = false;
    }
}

// Define the ivars struct first
#[derive(Debug)]
pub struct DelegateIvars {
    shared_data: Arc<Mutex<SharedFrameData>>,
}

declare_class!(
    struct Delegate;

    unsafe impl ClassType for Delegate {
        type Super = NSObject;
        type Mutability = mutability::Mutable;
        const NAME: &'static str = "StreamOutputSampleBufferDelegate";
    }

    impl DeclaredClass for Delegate {
        type Ivars = DelegateIvars;
    }

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl SCStreamOutput for Delegate {
        #[method(stream:didOutputSampleBuffer:ofType:)]
        fn stream_did_output_sample_buffer(
            &self,
            _stream: &SCStream,
            sample_buffer: CMSampleBufferRef,
            of_type: SCStreamOutputType,
        ) {
            if of_type != SCStreamOutputType::Screen {
                return;
            }

            println!("📸 Received new sample buffer");

            let sample_buffer = unsafe { CMSampleBuffer::wrap_under_get_rule(sample_buffer) };
            if let Some(image_buffer) = sample_buffer.get_image_buffer() {
                if let Some(pixel_buffer) = image_buffer.downcast::<CVPixelBuffer>() {
                    println!("📊 Got pixel buffer: {}x{}",
                        pixel_buffer.get_width(), pixel_buffer.get_height());
                    println!("   Pixel format: {:?}", pixel_buffer.get_pixel_format());

                    // Add this check
                    if pixel_buffer.get_pixel_format() != kCVPixelFormatType_32BGRA {
                        println!("⚠️ Unexpected pixel format - expected BGRA");
                    }

                    pixel_buffer.lock_base_address(kCVPixelBufferLock_ReadOnly);
                    let data = unsafe { pixel_buffer.get_base_address() };

                    if data.is_null() {
                        println!("❌ Pixel buffer base address is null");
                        return;
                    }

                    let width = pixel_buffer.get_width() as u32;
                    let height = pixel_buffer.get_height() as u32;
                    let bytes_per_row = pixel_buffer.get_bytes_per_row() as usize;

                    println!("📏 Buffer details:");
                    println!("   Width: {}, Height: {}", width, height);
                    println!("   Expected buffer size: {}", width * height * 4);
                    println!("   Bytes per row: {}", bytes_per_row);

                    // Create RGBA buffer with pre-allocated capacity
                    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

                    // Safe buffer copying
                    for y in 0..height {
                        for x in 0..width {
                            let offset = (y as usize * bytes_per_row) + (x as usize * 4);
                            unsafe {
                                let ptr = data.add(offset);
                                // Verify we're not exceeding buffer bounds
                                if offset + 4 <= bytes_per_row * height as usize {
                                    let slice = std::slice::from_raw_parts(ptr as *const u8, 4);
                                    rgba.extend_from_slice(slice);
                                }
                            }
                        }
                    }

                    println!("📊 Buffer creation results:");
                    println!("   Actual buffer size: {}", rgba.len());
                    println!("   Expected size: {}", (width * height * 4) as usize);
                    println!("   Match?: {}", rgba.len() == (width * height * 4) as usize);

                    // Update shared data only if we successfully created the buffer
                    if rgba.len() == (width * height * 4) as usize {
                        if let Ok(mut shared) = self.ivars().shared_data.lock() {
                            println!("💾 Updated shared buffer with {}x{} frame",
                                width, height);  // Changed from shared.width to width
                            shared.width = width;
                            shared.height = height;
                            shared.buffer = rgba;
                            shared.new_frame = true;
                        } else {
                            println!("❌ Failed to lock shared data");
                        }
                    } else {
                        println!("❌ Buffer size mismatch - skipping frame");
                        println!("   Got: {}, Expected: {}", rgba.len(), (width * height * 4) as usize);
                    }

                    // Always unlock the buffer
                    pixel_buffer.unlock_base_address(kCVPixelBufferLock_ReadOnly);
                } else {
                    println!("❌ Failed to get CVPixelBuffer");
                }
            } else {
                println!("❌ Failed to get image buffer");
            }
        }
    }

    unsafe impl SCStreamDelegate for Delegate {
        #[method(stream:didStopWithError:)]
        unsafe fn stream_did_stop_with_error(&self, _stream: &SCStream, error: &NSError) {
            println!("error: {:?}", error);
        }
    }
);

impl Delegate {
    pub fn new(shared_data: Arc<Mutex<SharedFrameData>>) -> Id<Self> {
        let this: Allocated<Self> = Self::alloc();
        unsafe {
            let this = this.set_ivars(DelegateIvars { shared_data });
            msg_send_id![super(this), init]
        }
    }
}

fn capture_screen(shared_data: Arc<Mutex<SharedFrameData>>) {
    println!("🎥 Starting screen capture");

    tokio::runtime::Runtime::new().unwrap().block_on(async {
        // Create channel for ShareableContent result
        let (tx, rx) = std::sync::mpsc::channel();

        // Get displays using completion handler
        SCShareableContent::get_shareable_content_with_completion_closure(move |content, error| {
            let result = content.ok_or_else(|| error.unwrap());
            tx.send(result).unwrap();
        });

        // Wait for and unwrap the ShareableContent result
        let shareable_content = rx.recv().unwrap().unwrap();
        let displays = shareable_content.displays();

        println!("🖥️ Found {} displays", displays.len());

        let display = match displays.first() {
            Some(display) => {
                println!("📺 Using display: {}x{}", display.width(), display.height());
                display
            }
            None => {
                println!("❌ No display found");
                return;
            }
        };

        // Create content filter
        let filter = SCContentFilter::init_with_display_exclude_windows(
            SCContentFilter::alloc(),
            display,
            &NSArray::new(),
        );

        // Configure stream
        let config = SCStreamConfiguration::new();
        config.set_width(display.width() as usize);
        config.set_height(display.height() as usize);

        // Create delegate with shared data
        // let delegate = Delegate {
        //     shared_data: shared_data.clone(),
        // };
        let delegate = Delegate::new(shared_data.clone());
        let stream_error = ProtocolObject::from_ref(&*delegate);

        // Create stream with filter and configuration
        let stream = SCStream::init_with_filter(SCStream::alloc(), &filter, &config, stream_error);

        // Set up queue and add stream output
        let queue = Queue::new("com.screen_capture.queue", QueueAttribute::Serial);
        let output = ProtocolObject::from_ref(&*delegate);
        stream
            .add_stream_output(output, SCStreamOutputType::Screen, &queue)
            .expect("Failed to add stream output");

        // Start capture
        stream.start_capture(|result| match result {
            None => println!("✅ Capture started successfully"),
            Some(error) => println!("❌ Capture error: {:?}", error),
        });

        println!("🔄 Entering capture loop");
        // Keep alive
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    });
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
