extern crate core_graphics;
extern crate objc;

use core_graphics::display::{CGDirectDisplayID, CGDisplay, CGDisplayConfigRef};
use core_graphics::display::CGDisplayMode;
use objc::runtime::{Object};
use objc_foundation::{NSString, INSString};
use objc_id::Id;
use std::ptr;

fn get_display_name(display_id: CGDirectDisplayID) -> Option<String> {
    unsafe {
        // Use the Core Graphics function to get a dictionary of display info
        let io_service_port = core_graphics::display::CGDisplayIOServicePort(display_id);

        // Access the display's name using the kDisplayProductName key
        let display_name_key = NSString::from_str("kDisplayProductName");
        let display_name_obj: *mut Object = objc::msg_send![io_service_port, objectForKey: display_name_key];
        
        if !display_name_obj.is_null() {
            let display_name: Id<NSString> = Id::from_ptr(display_name_obj);
            return Some(display_name.as_str().to_owned());
        }
    }
    None
}

fn get_display_id_by_name(name: &str) -> Option<CGDirectDisplayID> {
    let active_displays = CGDisplay::active_displays().unwrap();

    for display_id in active_displays.iter() {
        if let Some(display_name) = get_display_name(*display_id) {
            if display_name == name {
                return Some(*display_id);
            }
        }
    }
    None
}

fn set_display_mode(display_id: CGDirectDisplayID, width: usize, height: usize, x: i32, y: i32) {
    let modes = CGDisplay::modes_for_display(display_id);
    for mode in modes {
        if mode.width() == width && mode.height() == height {
            let mut config: CGDisplayConfigRef = ptr::null_mut();

            unsafe {
                if core_graphics::display::CGBeginDisplayConfiguration(&mut config) == 0 {
                    core_graphics::display::CGConfigureDisplayWithDisplayMode(
                        config,
                        display_id,
                        mode.as_concrete_TypeRef(),
                        ptr::null(),
                    );

                    core_graphics::display::CGConfigureDisplayOrigin(
                        config,
                        display_id,
                        x,
                        y,
                    );

                    core_graphics::display::CGCompleteDisplayConfiguration(
                        config,
                        core_graphics::display::CGConfigureOption::ConfigureForSession,
                    );
                }
            }

            break;
        }
    }
}

fn main() {
    if let Some(air_display_id) = get_display_id_by_name("Air") {
        set_display_mode(air_display_id, 1920, 1080, 1000, 0); // Example values
    } else {
        println!("Air display not found.");
    }
}
