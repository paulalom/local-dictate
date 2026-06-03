#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FocusTarget {
    foreground_window: isize,
    focused_control: Option<isize>,
}

pub fn current_focus_target() -> Option<FocusTarget> {
    platform::current_focus_target()
}

pub fn restore_focus(target: Option<FocusTarget>) {
    if let Some(target) = target {
        platform::restore_focus(target);
    }
}

#[cfg(target_os = "windows")]
fn settle_after_restore() {
    thread::sleep(Duration::from_millis(80));
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{FocusTarget, settle_after_restore};
    use std::mem;
    use std::ptr;
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GetForegroundWindow, GetGUIThreadInfo, GetWindowThreadProcessId, IsWindow,
        MSG, PM_NOREMOVE, PeekMessageW, SetForegroundWindow,
    };

    pub fn current_focus_target() -> Option<FocusTarget> {
        let handle = unsafe { GetForegroundWindow() };

        if handle.is_null() {
            None
        } else {
            Some(FocusTarget {
                foreground_window: handle as isize,
                focused_control: focused_control_for_window(handle).map(|handle| handle as isize),
            })
        }
    }

    pub fn restore_focus(target: FocusTarget) {
        let handle = target.foreground_window as HWND;

        if !is_valid_window(handle) {
            return;
        }

        let current_thread = unsafe { GetCurrentThreadId() };
        ensure_message_queue();

        let foreground_thread = window_thread(unsafe { GetForegroundWindow() });
        let target_thread = window_thread(handle);
        let foreground_attached = attach_input_thread(current_thread, foreground_thread);
        let target_attached = if target_thread == foreground_thread {
            None
        } else {
            attach_input_thread(current_thread, target_thread)
        };

        unsafe {
            SetForegroundWindow(handle);
        }
        settle_after_restore();

        if let Some(focused_control) = target.focused_control {
            let focused_control = focused_control as HWND;

            if is_valid_window(focused_control) {
                unsafe {
                    SetFocus(focused_control);
                }
            }
        }

        detach_input_thread(current_thread, target_attached);
        detach_input_thread(current_thread, foreground_attached);
    }

    fn focused_control_for_window(window: HWND) -> Option<HWND> {
        let target_thread = window_thread(window);
        if target_thread == 0 {
            return None;
        }

        let mut info = GUITHREADINFO {
            cbSize: mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };

        if unsafe { GetGUIThreadInfo(target_thread, &mut info) } == 0
            || info.hwndFocus.is_null()
            || !is_valid_window(info.hwndFocus)
        {
            None
        } else {
            Some(info.hwndFocus)
        }
    }

    fn ensure_message_queue() {
        let mut message = MSG::default();
        unsafe {
            PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_NOREMOVE);
        }
    }

    fn window_thread(window: HWND) -> u32 {
        if window.is_null() {
            0
        } else {
            unsafe { GetWindowThreadProcessId(window, ptr::null_mut()) }
        }
    }

    fn attach_input_thread(current_thread: u32, other_thread: u32) -> Option<u32> {
        if other_thread == 0 || other_thread == current_thread {
            return None;
        }

        (unsafe { AttachThreadInput(current_thread, other_thread, true.into()) } != 0)
            .then_some(other_thread)
    }

    fn detach_input_thread(current_thread: u32, attached_thread: Option<u32>) {
        let Some(attached_thread) = attached_thread else {
            return;
        };

        unsafe {
            AttachThreadInput(current_thread, attached_thread, false.into());
        }
    }

    fn is_valid_window(handle: HWND) -> bool {
        !handle.is_null() && unsafe { IsWindow(handle) } != 0
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::FocusTarget;
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    pub fn current_focus_target() -> Option<FocusTarget> {
        let window = xdotool_output(&["getactivewindow"])?;
        let foreground_window = window.trim().parse::<isize>().ok()?;

        if foreground_window <= 0 {
            return None;
        }

        Some(FocusTarget {
            foreground_window,
            focused_control: None,
        })
    }

    pub fn restore_focus(target: FocusTarget) {
        if target.foreground_window <= 0 {
            return;
        }

        let window = target.foreground_window.to_string();
        let _ = Command::new("xdotool")
            .args(["windowactivate", "--sync", &window])
            .status();

        thread::sleep(Duration::from_millis(120));
    }

    fn xdotool_output(args: &[&str]) -> Option<String> {
        let output = Command::new("xdotool").args(args).output().ok()?;

        if output.status.success() {
            String::from_utf8(output.stdout).ok()
        } else {
            None
        }
    }
}

#[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
mod platform {
    use super::FocusTarget;

    pub fn current_focus_target() -> Option<FocusTarget> {
        None
    }

    pub fn restore_focus(_target: FocusTarget) {}
}
