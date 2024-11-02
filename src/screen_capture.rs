use std::ops::DerefMut;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use core_foundation::base::TCFType;
use core_media::sample_buffer::{CMSampleBuffer, CMSampleBufferRef};
use core_video::pixel_buffer::{
    kCVPixelBufferLock_ReadOnly, kCVPixelFormatType_32BGRA,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange, CVPixelBuffer,
};
use dispatch2::{Queue, QueueAttribute};
use libc::size_t;
use objc2::mutability;
use objc2::{
    declare_class, extern_methods, msg_send, msg_send_id,
    rc::{Allocated, Id},
    runtime::ProtocolObject,
    ClassType, DeclaredClass,
};
use objc2_foundation::{NSArray, NSError, NSObject, NSObjectProtocol};
use screen_capture_kit::{
    shareable_content::SCShareableContent,
    stream::{
        SCContentFilter, SCStream, SCStreamConfiguration, SCStreamDelegate, SCStreamOutput,
        SCStreamOutputType,
    },
};
use tokio::sync::mpsc::{self, Receiver, Sender};

use crate::stage::AssetHandles;
use crate::ScaleFactor;
use core_graphics2::display::CGDisplay; // Use core-graphics2

#[derive(Resource)]
struct FrameChannel {
    sender: Arc<Sender<Vec<u8>>>,
    receiver: Receiver<Vec<u8>>,
}

pub struct ScreenCapturePlugin;

impl Plugin for ScreenCapturePlugin {
    fn build(&self, app: &mut App) {
        let (tx, rx) = mpsc::channel::<Vec<u8>>(60);
        app.insert_resource(FrameChannel {
            sender: Arc::new(tx),
            receiver: rx,
        })
        .add_systems(Startup, setup_screen_capture)
        .add_systems(Update, update_screen_texture);
    }
}

pub struct DelegateIvars {
    frame_sender: Arc<Sender<Vec<u8>>>,
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
        unsafe fn stream_did_output_sample_buffer(&self, _stream: &SCStream, sample_buffer: CMSampleBufferRef, of_type: SCStreamOutputType) {
            if of_type != SCStreamOutputType::Screen {
                return;
            }
            let sample_buffer = CMSampleBuffer::wrap_under_get_rule(sample_buffer);
            if let Some(image_buffer) = sample_buffer.get_image_buffer() {
                if let Some(pixel_buffer) = image_buffer.downcast::<CVPixelBuffer>() {
                    // Lock the base address of the pixel buffer
                    pixel_buffer.lock_base_address(kCVPixelBufferLock_ReadOnly);

                    // println!("frame_sender: {:?}", self.ivars().frame_sender);

                    if pixel_buffer.get_pixel_format() != kCVPixelFormatType_32BGRA {
                        println!("Unexpected pixel format");
                        return;
                    }
                    // let _ = self.ivars().frame_sender.send(rgba_data);

                    let width = pixel_buffer.get_width();
                    let height = pixel_buffer.get_height();
                    let bytes_per_row = pixel_buffer.get_bytes_per_row();
                    let buffer_size = height * bytes_per_row;
                    let base_address = unsafe { pixel_buffer.get_base_address() };
                    let pixels = std::slice::from_raw_parts(
                        base_address as *const u8,
                        buffer_size
                    );

                    // Create RGBA buffer with pre-allocated capacity
                    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
                    for y in 0..height {
                        for x in 0..width {
                            let src_idx = (y * bytes_per_row + x * 4) as usize;
                            // BGRA to RGBA conversion
                            let b = pixels[src_idx];
                            let g = pixels[src_idx + 1];
                            let r = pixels[src_idx + 2];
                            let a = pixels[src_idx + 3];
                            rgba.extend_from_slice(&[r, g, b, a]);
                        }
                    }

                    pixel_buffer.unlock_base_address(kCVPixelBufferLock_ReadOnly);

                    if let Err(e) = self.ivars().frame_sender.try_send(rgba) {
                        error!("Failed to send frame data: {}", e);
                    }

                    // println!("base address: {:?}", base_address);
                    // println!("pixel buffer: {:?}", pixel_buffer);
                    // println!("pixel format: {}", pixel_buffer.get_pixel_format());
                    // println!("width: {}, height: {}, bytes_per_row: {}", width, height, bytes_per_row);
                    // println!("pixels: {:?}", pixels);

                    // // Get plane 0 (Y plane)
                    // let y_plane_base = pixel_buffer.get_base_address_of_plane(0);
                    // let y_plane_bytes_per_row = pixel_buffer.get_bytes_per_row_of_plane(0);
                    // let y_plane_height = pixel_buffer.get_height_of_plane(0);
                    // let y_plane = slice::from_raw_parts(
                    //     y_plane_base as *const u8,
                    //     y_plane_height * y_plane_bytes_per_row
                    // );

                    // // Get plane 1 (UV plane)
                    // let uv_plane_base = pixel_buffer.get_base_address_of_plane(1);
                    // let uv_plane_bytes_per_row = pixel_buffer.get_bytes_per_row_of_plane(1);
                    // let uv_plane_height = pixel_buffer.get_height_of_plane(1);
                    // let uv_plane = slice::from_raw_parts(
                    //     uv_plane_base as *const u8,
                    //     uv_plane_height * uv_plane_bytes_per_row
                    // );

                    // // Now save or process the image using both planes
                    // save_yuv_as_png(y_plane, uv_plane, width, height,
                    //             y_plane_bytes_per_row, uv_plane_bytes_per_row);

                    // Unlock the base address when done
                    // pixel_buffer.unlock_base_address(kCVPixelBufferLock_ReadOnly);
                }
            }
        }
    }

    unsafe impl SCStreamDelegate for Delegate {
        #[method(stream:didStopWithError:)]
        unsafe fn stream_did_stop_with_error(&self, _stream: &SCStream, error: &NSError) {
            println!("error: {:?}", error);
        }
    }

    // unsafe impl Delegate {
    //     #[method_id(init)]
    //     fn init(this: Allocated<Self>) -> Option<Id<Self>> {
    //         let this = this.set_ivars(DelegateIvars {});
    //         unsafe { msg_send_id![super(this), init] }
    //     }
    // }
);

impl Delegate {
    pub fn new(frame_sender: Arc<Sender<Vec<u8>>>) -> Id<Self> {
        let this: Allocated<Self> = Self::alloc();
        unsafe {
            let this = this.set_ivars(DelegateIvars { frame_sender });
            msg_send_id![super(this), init]
        }
    }
}

fn setup_screen_capture(
    mut commands: Commands,
    frame_channel: Res<FrameChannel>,
    scale_factor: Res<ScaleFactor>,
) {
    let sender = frame_channel.sender.clone();
    let scale_factor_value = scale_factor.value.clone();

    std::thread::spawn(move || {
        let (sc_tx, mut sc_rx) = mpsc::channel(1);
        SCShareableContent::get_shareable_content_with_completion_closure(
            move |shareable_content, error| {
                let ret = shareable_content.ok_or_else(|| error.unwrap());
                sc_tx.blocking_send(ret).unwrap();
            },
        );
        let shareable_content = sc_rx.blocking_recv().unwrap();
        if let Err(error) = shareable_content {
            println!("error: {:?}", error);
            return;
        }
        let shareable_content = shareable_content.unwrap();

        let displays = shareable_content.displays();
        let display = match displays.first() {
            Some(display) => display,
            None => {
                println!("no display found");
                return;
            }
        };

        let filter = SCContentFilter::init_with_display_exclude_windows(
            SCContentFilter::alloc(),
            display,
            &NSArray::new(),
        );

        let configuration: Id<SCStreamConfiguration> = SCStreamConfiguration::new();

        configuration.set_width((display.width() as f64 * scale_factor_value) as size_t);
        configuration.set_height((display.height() as f64 * scale_factor_value) as size_t);
        configuration.set_minimum_frame_interval(core_media::time::CMTime::make(1, 60));
        // it's better to do the conversion after capturing rather than asking macOS to do it during capture.
        // the internal representation of pixels on macOS uses BGRA order
        configuration.set_pixel_format(kCVPixelFormatType_32BGRA);

        let delegate = Delegate::new(sender);
        let stream_error = ProtocolObject::from_ref(&*delegate);
        let stream =
            SCStream::init_with_filter(SCStream::alloc(), &filter, &configuration, stream_error);
        let queue = Queue::new("com.spatial_display.queue", QueueAttribute::Serial);
        let output = ProtocolObject::from_ref(&*delegate);
        if let Err(ret) = stream.add_stream_output(output, SCStreamOutputType::Screen, &queue) {
            println!("error: {:?}", ret);
            return;
        }

        info!("Waiting 5 seconds before starting capture...");
        std::thread::sleep(std::time::Duration::from_secs(5));

        info!("STARTING CAP");
        stream.start_capture(move |result| {
            info!("start_capture");
            if let Some(error) = result {
                warn!("error: {:?}", error);
            }
        });
        // std::thread::sleep(std::time::Duration::from_secs(10));
        // stream.stop_capture(move |result| {
        //     if let Some(error) = result {
        //         println!("error: {:?}", error);
        //     }
        // });
        info!("🔄 Entering capture loop");
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

fn update_screen_texture(
    mut frame_channel: ResMut<FrameChannel>,
    mut images: ResMut<Assets<Image>>,
    asset_handles: Res<AssetHandles>,
    mut extracted_asset_events: EventWriter<AssetEvent<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if let Ok(frame_data) = frame_channel.receiver.try_recv() {
        // info!("Got frame: {}x{} = {} bytes", 1800, 1169, frame_data.len());

        if let Some(image) = images.get_mut(&asset_handles.screen) {
            // info!(
            //     "Current texture: {}x{} = {} bytes",
            //     image.texture_descriptor.size.width,
            //     image.texture_descriptor.size.height,
            //     image.data.len()
            // );

            // Verify dimensions
            // let expected_size = screen_size.width * screen_size.height * 4;
            // assert_eq!(
            //     frame_data.len(),
            //     expected_size,
            //     "Frame data size mismatch: got {} expected {}",
            //     frame_data.len(),
            //     expected_size
            // );

            image.data = frame_data;

            for (_, mut material) in materials.iter_mut() {
                // material.deref_mut();
            }

            // Force texture update by sending asset event
            // extracted_asset_events.send(AssetEvent::Modified {
            //     id: asset_handles.screen.id(),
            // });
        }
    }
}
