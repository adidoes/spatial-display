mod ar_drivers {
    pub mod lib;
}
use ar_drivers::lib::{any_glasses, GlassesEvent};

use bevy::diagnostic::FrameTimeDiagnosticsPlugin;
use bevy::diagnostic::LogDiagnosticsPlugin;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::Extent3d;
use bevy::render::render_resource::TextureDimension;
use bevy::window::{
    MonitorSelection, PresentMode, Window, WindowLevel, WindowMode, WindowPlugin, WindowPosition,
};

use bevy::render::render_resource::{TextureFormat, TextureUsages};
use core_foundation::base::TCFType;
use core_media::sample_buffer::{CMSampleBuffer, CMSampleBufferRef};
use core_video::pixel_buffer::kCVPixelBufferLock_ReadOnly;
use core_video::pixel_buffer::CVPixelBuffer;
use dispatch2::{Queue, QueueAttribute};
use objc2::{
    declare_class, msg_send_id, mutability,
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
    stream::{SCStream, SCStreamConfiguration},
};

use dcmimu::DCMIMU;

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
        .add_systems(FixedUpdate, update_texture)
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

fn setup(
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
    commands.insert_resource(ScreenTexture {
        handle: texture_handle.clone(),
    });
    println!("Texture handle: {:?}", texture_handle);

    // Create material with the screen texture
    let screen_material = materials.add(StandardMaterial {
        base_color_texture: Some(texture_handle),
        unlit: true,                   // Make sure lighting doesn't affect the texture
        alpha_mode: AlphaMode::Opaque, // Change from Blend to Opaque
        ..default()
    });

    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Plane3d::new(Vec3::Z, Vec2::splat(5.0))))),
        MeshMaterial3d(screen_material),
        Transform::from_xyz(0.0, 0.0, -10.0),
        // Transform::from_xyz(0.0, 0.0, -10.0)
        //     .with_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2)), // Rotate 90 degrees around X axis
    ));

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
            base_color: Color::linear_rgb(255.0, 200.0, 0.0),
            ..default()
        })),
        Transform::from_xyz(-4.0, 1.0, -7.0),
    ));

    // Camera
    commands.spawn((
        Camera3d::default(),
        Camera::default(),
        Projection::Perspective(PerspectiveProjection {
            fov: 21.70f32.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0.0, 0.0, 0.0),
    ));

    // Light
    commands.spawn((
        PointLight {
            intensity: 1500.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0),
    ));
}

fn update_texture(
    frame_data: Res<FrameDataResource>,
    screen_texture: Res<ScreenTexture>,
    mut images: ResMut<Assets<Image>>,
) {
    let mut shared = frame_data.shared_data.lock().unwrap();

    if !shared.new_frame {
        return;
    }

    if let Some(texture) = images.get_mut(&screen_texture.handle) {
        // Check if texture dimensions need updating
        if texture.width() != shared.width || texture.height() != shared.height {
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
            // Update existing texture data
            texture.data.clear();
            texture.data.extend_from_slice(&shared.buffer);
        }
    }

    shared.new_frame = false;
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

            // println!("📸 Received new sample buffer");

            let sample_buffer = unsafe { CMSampleBuffer::wrap_under_get_rule(sample_buffer) };
            if let Some(image_buffer) = sample_buffer.get_image_buffer() {
                if let Some(pixel_buffer) = image_buffer.downcast::<CVPixelBuffer>() {
                    // println!("📊 Got pixel buffer: {}x{}",
                    //     pixel_buffer.get_width(), pixel_buffer.get_height());
                    // println!("   Pixel format: {:?}", pixel_buffer.get_pixel_format());

                    pixel_buffer.lock_base_address(kCVPixelBufferLock_ReadOnly);
                    let data = unsafe { pixel_buffer.get_base_address() };

                    if data.is_null() {
                        println!("❌ Pixel buffer base address is null");
                        return;
                    }

                    let width = pixel_buffer.get_width() as u32;
                    let height = pixel_buffer.get_height() as u32;
                    let bytes_per_row = pixel_buffer.get_bytes_per_row() as usize;

                    // println!("📏 Buffer details:");
                    // println!("   Width: {}, Height: {}", width, height);
                    // println!("   Expected buffer size: {}", width * height * 4);
                    // println!("   Bytes per row: {}", bytes_per_row);

                    // Create RGBA buffer with pre-allocated capacity
                    let mut rgba = Vec::with_capacity((width * height * 4) as usize);

                    // Safe buffer copying with proper stride handling
                    unsafe {
                        for y in 0..height as usize {
                            let row_ptr = data.add(y * bytes_per_row) as *const u8;
                            let row_slice = std::slice::from_raw_parts(row_ptr, width as usize * 4);

                            // Convert BGRA to RGBA if needed
                            for pixel in row_slice.chunks_exact(4) {
                                // BGRA to RGBA conversion
                                rgba.push(pixel[2]); // R (from B)
                                rgba.push(pixel[1]); // G (stays same)
                                rgba.push(pixel[0]); // B (from R)
                                rgba.push(pixel[3]); // A (stays same)
                            }
                        }
                    }

                    // println!("📊 Buffer creation results:");
                    // println!("   Actual buffer size: {}", rgba.len());
                    // println!("   Expected size: {}", (width * height * 4) as usize);
                    // println!("   Match?: {}", rgba.len() == (width * height * 4) as usize);

                    // Update shared data only if we successfully created the buffer
                    if rgba.len() == (width * height * 4) as usize {
                        if let Ok(mut shared) = self.ivars().shared_data.lock() {
                            // println!("💾 Updated shared buffer with {}x{} frame", width, height);
                            shared.width = width;
                            shared.height = height;
                            shared.buffer = rgba;
                            shared.new_frame = true;
                            // println!("New frame captured: {}x{}", width, height);
                        } else {
                            println!("❌ Failed to lock shared data");
                        }
                    } else {
                        println!("❌ Buffer size mismatch - skipping frame");
                        println!("   Got: {}, Expected: {}", rgba.len(), (width * height * 4) as usize);
                    }

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
