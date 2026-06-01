use std::fmt;

use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

use crate::icon::local_dictate_icon_data;

pub struct AppTray {
    _tray_icon: TrayIcon,
    show_item: MenuItem,
    exit_item: MenuItem,
}

impl fmt::Debug for AppTray {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("AppTray")
    }
}

impl AppTray {
    pub fn new() -> Result<Self, String> {
        initialize_platform_tray()?;

        let show_item = MenuItem::new("Show Local Dictate", true, None);
        let exit_item = MenuItem::new("Exit", true, None);
        let separator = PredefinedMenuItem::separator();
        let menu = Menu::new();
        menu.append_items(&[&show_item, &separator, &exit_item])
            .map_err(|error| error.to_string())?;

        let tray_icon = TrayIconBuilder::new()
            .with_tooltip("Local Dictate")
            .with_icon(local_dictate_icon()?)
            .with_icon_as_template(cfg!(target_os = "macos"))
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(cfg!(target_os = "macos"))
            .with_menu_on_right_click(true)
            .build()
            .map_err(|error| error.to_string())?;

        Ok(Self {
            _tray_icon: tray_icon,
            show_item,
            exit_item,
        })
    }

    pub fn poll_action(&self) -> Option<TrayAction> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if event.id() == self.show_item.id() {
                return Some(TrayAction::Show);
            }

            if event.id() == self.exit_item.id() {
                return Some(TrayAction::Exit);
            }
        }

        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            match event {
                TrayIconEvent::DoubleClick { .. } => return Some(TrayAction::Show),
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } if !cfg!(target_os = "macos") => return Some(TrayAction::Show),
                _ => {}
            }
        }

        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Show,
    Exit,
}

fn local_dictate_icon() -> Result<Icon, String> {
    let data = local_dictate_icon_data();
    Icon::from_rgba(data.rgba, data.width, data.height).map_err(|error| error.to_string())
}

#[cfg(target_os = "linux")]
fn initialize_platform_tray() -> Result<(), String> {
    if gtk::is_initialized() {
        return Ok(());
    }

    gtk::init().map_err(|error| format!("GTK initialization failed: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn initialize_platform_tray() -> Result<(), String> {
    Ok(())
}
