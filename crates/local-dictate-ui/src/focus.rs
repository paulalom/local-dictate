use std::thread;
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

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
mod platform {
    use super::FocusTarget;

    pub fn current_focus_target() -> Option<FocusTarget> {
        None
    }

    pub fn restore_focus(_target: FocusTarget) {}
}

#[cfg(target_os = "linux")]
mod platform {
    use super::{FocusTarget, settle_after_restore};
    use libxdo_sys::{
        xdo_activate_window, xdo_focus_window, xdo_free, xdo_get_active_window,
        xdo_get_focused_window_sane, xdo_new,
    };
    use std::ptr;
    use x11::xlib::Window;

    pub fn current_focus_target() -> Option<FocusTarget> {
        let xdo = XdoHandle::new()?;
        let foreground_window = active_window(&xdo)?;
        let focused_control = focused_window(&xdo);

        Some(FocusTarget {
            foreground_window: foreground_window as isize,
            focused_control: focused_control.map(|window| window as isize),
        })
    }

    pub fn restore_focus(target: FocusTarget) {
        let Some(xdo) = XdoHandle::new() else {
            return;
        };

        let foreground_window = target.foreground_window as Window;
        if foreground_window == 0 {
            return;
        }

        unsafe {
            xdo_activate_window(xdo.as_ptr(), foreground_window);
            xdo_focus_window(xdo.as_ptr(), foreground_window);
        }
        settle_after_restore();

        if let Some(focused_control) = target.focused_control {
            let focused_control = focused_control as Window;

            if focused_control != 0 && focused_control != foreground_window {
                unsafe {
                    xdo_focus_window(xdo.as_ptr(), focused_control);
                }
                settle_after_restore();
            }
        }
    }

    struct XdoHandle {
        handle: *mut libxdo_sys::xdo_t,
    }

    impl XdoHandle {
        fn new() -> Option<Self> {
            let handle = unsafe { xdo_new(ptr::null()) };

            if handle.is_null() {
                None
            } else {
                Some(Self { handle })
            }
        }

        fn as_ptr(&self) -> *mut libxdo_sys::xdo_t {
            self.handle
        }
    }

    impl Drop for XdoHandle {
        fn drop(&mut self) {
            unsafe {
                xdo_free(self.handle);
            }
        }
    }

    fn active_window(xdo: &XdoHandle) -> Option<Window> {
        let mut window = 0;
        let result = unsafe { xdo_get_active_window(xdo.as_ptr(), &mut window) };

        (result == 0 && window != 0).then_some(window)
    }

    fn focused_window(xdo: &XdoHandle) -> Option<Window> {
        let mut window = 0;
        let result = unsafe { xdo_get_focused_window_sane(xdo.as_ptr(), &mut window) };

        (result == 0 && window != 0).then_some(window)
    }
}
