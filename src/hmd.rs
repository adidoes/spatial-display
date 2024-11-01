use std::sync::{Arc, Mutex};

use ar_drivers::{any_glasses, GlassesEvent};

use bevy::prelude::*;
use dcmimu::DCMIMU;

#[derive(Resource)]
struct ImuStore {
    dcmimu: Arc<Mutex<DCMIMU>>,
}

pub struct HmdPlugin;

impl Plugin for HmdPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ImuStore {
            dcmimu: Arc::new(Mutex::new(DCMIMU::new())),
        })
        .add_systems(Startup, hmd_motion_tracking)
        .add_systems(FixedPreUpdate, update_camera_orientation);
    }
}

/// Initializes HMD (Head-Mounted Display) motion tracking in a separate thread
///
/// # Arguments
/// * `imu_store` - Resource containing shared DCMIMU state for orientation tracking
///
/// # Details
/// This function:
/// 1. Spawns a dedicated thread for processing IMU (Inertial Measurement Unit) data
/// 2. Continuously reads accelerometer and gyroscope data from the glasses
/// 3. Updates the shared DCMIMU state with new motion data
/// 4. Calculates time delta between measurements for accurate integration
///
/// The motion data is used to update camera orientation in the main rendering thread.
fn hmd_motion_tracking(imu_store: Res<ImuStore>) {
    let shared_dcmimu_clone = Arc::clone(&imu_store.dcmimu);

    std::thread::spawn(move || {
        let mut glasses = any_glasses().unwrap();
        // println!("Got glasses, serial={}", glasses.serial().unwrap());
        let mut last_timestamp: Option<u64> = None;

        // use std::time::{Duration, Instant};
        // let mut last_print_time = Instant::now();
        // let mut loop_counter = 0;

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

            // loop_counter += 1;

            // if last_print_time.elapsed() > Duration::from_secs(1) {
            //     println!("Loop has run {} times in the last second", loop_counter);
            //     loop_counter = 0;
            //     last_print_time = Instant::now();
            // }
        }
    });
}

/// Updates camera orientation based on HMD/glasses IMU data
///
/// # Arguments
/// * `query` - Query for all camera transforms that need updating
/// * `state` - Shared state containing IMU orientation data
///
/// # Details
/// Converts IMU orientation (yaw, pitch, roll) into a quaternion rotation
/// and applies it to all camera entities in the scene.
///
/// The rotation order is YXZ (yaw, roll inverted, pitch) to match the IMU coordinate system.
fn update_camera_orientation(mut query: Query<&mut Transform, With<Camera>>, state: Res<ImuStore>) {
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
