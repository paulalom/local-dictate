use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTarget {
    raw_handle: isize,
}

pub fn current_focus_target() -> Option<FocusTarget> {
    platform::current_focus_target()
}

pub fn restore_focus(target: Option<FocusTarget>) {
    if let Some(target) = target {
        platform::restore_focus(target);
    }
}

fn settle_after_restore() {
    thread::sleep(Duration::from_millis(80));
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{FocusTarget, settle_after_restore};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, IsWindow, SetForegroundWindow,
    };

    pub fn current_focus_target() -> Option<FocusTarget> {
        let handle = unsafe { GetForegroundWindow() };
        let raw_handle = handle as isize;

        if raw_handle == 0 {
            None
        } else {
            Some(FocusTarget { raw_handle })
        }
    }

    pub fn restore_focus(target: FocusTarget) {
        let handle = target.raw_handle as _;

        if unsafe { IsWindow(handle) } == 0 {
            return;
        }

        unsafe {
            SetForegroundWindow(handle);
        }
        settle_after_restore();
    }
}

#[cfg(not(target_os = "windows"))]
mod platform {
    use super::FocusTarget;

    pub fn current_focus_target() -> Option<FocusTarget> {
        None
    }

    pub fn restore_focus(_target: FocusTarget) {}
}
