mod camera;
mod debug;
mod hmd;
mod screen_capture;
mod stage;

use bevy::{
    prelude::*,
    window::{PresentMode, WindowLevel, WindowMode},
};

use camera::CameraPlugin;
use debug::DebugPlugin;
use hmd::HMDPlugin;
use screen_capture::ScreenCapturePlugin;
use stage::StagePlugin;

fn main() {
    App::new()
        .insert_resource(AmbientLight {
            color: Color::default(),
            brightness: 100.0,
        })
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                // https://docs.rs/bevy_window/latest/bevy_window/enum.PresentMode.html
                present_mode: PresentMode::AutoNoVsync, // AutoVsync, AutoNoVsync
                // when using AutoVsync, add the bevy_framepace plugin and uncomment
                // the framespace_settings lines in setup()
                resizable: true,
                focused: false,
                // visible: false,
                window_level: WindowLevel::AlwaysOnTop,
                // mode: WindowMode::Fullscreen(MonitorSelection::Index(1)),
                // position: WindowPosition::Centered(MonitorSelection::Index(1)), // 0 is primary, 1 is secondary
                mode: WindowMode::Windowed,
                ..default()
            }),
            ..default()
        }))
        .add_plugins(CameraPlugin)
        .add_plugins(StagePlugin)
        .add_plugins(HMDPlugin)
        .add_plugins(ScreenCapturePlugin)
        // .add_plugins(DebugPlugin)
        .run();
}
