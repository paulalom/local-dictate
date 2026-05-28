use std::fmt;

use tray_icon::{
    Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
    menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
};

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
    let size = 32;
    let mut rgba = vec![0; size * size * 4];

    for y in 0..size {
        for x in 0..size {
            let index = (y * size + x) * 4;
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let in_disc = dx * dx + dy * dy <= 15 * 15;

            if in_disc {
                rgba[index] = 32;
                rgba[index + 1] = 88;
                rgba[index + 2] = 120;
                rgba[index + 3] = 255;
            }

            let mic_body = (12..=19).contains(&x) && (7..=19).contains(&y);
            let mic_stem = (15..=16).contains(&x) && (21..=25).contains(&y);
            let mic_base = (11..=20).contains(&x) && (25..=26).contains(&y);
            let mic_curve =
                (9..=22).contains(&x) && (17..=23).contains(&y) && !(12..=19).contains(&x);

            if mic_body || mic_stem || mic_base || mic_curve {
                rgba[index] = 248;
                rgba[index + 1] = 252;
                rgba[index + 2] = 255;
                rgba[index + 3] = 255;
            }
        }
    }

    Icon::from_rgba(rgba, size as u32, size as u32).map_err(|error| error.to_string())
}
