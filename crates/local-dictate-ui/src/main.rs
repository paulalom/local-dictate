mod config;

use std::path::PathBuf;

use config::{AppConfig, KeywordSwapConfig};
use eframe::egui;
use local_dictate_core::{KeywordSwap, PostProcessingSettings};

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 620.0])
            .with_min_inner_size([560.0, 460.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Local Dictate Settings",
        native_options,
        Box::new(|_creation_context| Ok(Box::new(SettingsApp::new()))),
    )
}

#[derive(Debug)]
struct SettingsApp {
    config_path: Option<PathBuf>,
    capture_hotkey: String,
    swaps: Vec<SwapRow>,
    new_from: String,
    new_to: String,
    preview_input: String,
    status: StatusMessage,
    recording_hotkey: bool,
    dirty: bool,
}

impl SettingsApp {
    fn new() -> Self {
        match config::load_or_default() {
            Ok(loaded) => Self::from_config(
                loaded.config,
                Some(loaded.path),
                StatusMessage::info("Settings ready"),
            ),
            Err(error) => Self::from_config(
                AppConfig::default(),
                config::default_config_path().ok(),
                StatusMessage::error(format!("Using defaults: {error}")),
            ),
        }
    }

    fn from_config(config: AppConfig, config_path: Option<PathBuf>, status: StatusMessage) -> Self {
        Self {
            config_path,
            capture_hotkey: config.capture_hotkey,
            swaps: config
                .keyword_swaps
                .into_iter()
                .map(SwapRow::from)
                .collect(),
            new_from: String::new(),
            new_to: String::new(),
            preview_input: "Ask peers to review this before we merge.".to_string(),
            status,
            recording_hotkey: false,
            dirty: false,
        }
    }

    fn mark_dirty(&mut self) {
        self.dirty = true;
        self.status = StatusMessage::info("Unsaved changes");
    }

    fn add_swap(&mut self) {
        if self.new_from.trim().is_empty() {
            self.status = StatusMessage::error("Keyword cannot be empty");
            return;
        }

        self.swaps.push(SwapRow {
            from: self.new_from.trim().to_string(),
            to: self.new_to.trim().to_string(),
        });
        self.new_from.clear();
        self.new_to.clear();
        self.mark_dirty();
    }

    fn reload(&mut self) {
        match config::load_or_default() {
            Ok(loaded) => {
                *self = Self::from_config(
                    loaded.config,
                    Some(loaded.path),
                    StatusMessage::success("Settings reloaded"),
                );
            }
            Err(error) => {
                self.status = StatusMessage::error(format!("Reload failed: {error}"));
            }
        }
    }

    fn reset_defaults(&mut self) {
        let config_path = self
            .config_path
            .clone()
            .or_else(|| config::default_config_path().ok());
        *self = Self::from_config(
            AppConfig::default(),
            config_path,
            StatusMessage::info("Defaults restored"),
        );
        self.dirty = true;
    }

    fn save(&mut self) {
        let Ok(config) = self.current_config() else {
            return;
        };

        match config::save(&config) {
            Ok(path) => {
                self.config_path = Some(path.clone());
                self.status = StatusMessage::success(format!("Saved to {}", path.display()));
                self.dirty = false;
            }
            Err(error) => {
                self.status = StatusMessage::error(format!("Save failed: {error}"));
            }
        }
    }

    fn current_config(&mut self) -> Result<AppConfig, ()> {
        let capture_hotkey = self.capture_hotkey.trim().to_string();
        let mut keyword_swaps = Vec::new();

        for (index, row) in self.swaps.iter().enumerate() {
            let from = row.from.trim();
            let to = row.to.trim();

            if from.is_empty() && to.is_empty() {
                continue;
            }

            if from.is_empty() {
                self.status =
                    StatusMessage::error(format!("Keyword swap {} needs a keyword", index + 1));
                return Err(());
            }

            keyword_swaps.push(KeywordSwapConfig {
                from: from.to_string(),
                to: to.to_string(),
            });
        }

        let config = AppConfig {
            capture_hotkey,
            keyword_swaps,
        };

        if let Err(error) = config.to_settings() {
            self.status = StatusMessage::error(error.to_string());
            return Err(());
        }

        Ok(config)
    }

    fn preview_output(&self) -> String {
        let swaps = self
            .swaps
            .iter()
            .filter_map(|row| KeywordSwap::new(row.from.trim(), row.to.trim()).ok())
            .collect::<Vec<_>>();

        PostProcessingSettings::new(swaps).apply(&self.preview_input)
    }

    fn record_hotkey_from_events(&mut self, context: &egui::Context) {
        if !self.recording_hotkey {
            return;
        }

        let recorded = context.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } => Some(format_hotkey(*modifiers, *key)),
                _ => None,
            })
        });

        if let Some(hotkey) = recorded {
            self.capture_hotkey = hotkey;
            self.recording_hotkey = false;
            self.mark_dirty();
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Local Dictate");
            ui.add_space(8.0);

            if self.dirty {
                ui.label(egui::RichText::new("Unsaved").color(egui::Color32::from_rgb(180, 96, 0)));
            }
        });

        if let Some(path) = &self.config_path {
            ui.small(path.display().to_string());
        }
    }

    fn hotkey_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Voice Capture");
        ui.horizontal(|ui| {
            ui.label("Hotkey");
            let response = ui.add_sized(
                [260.0, 24.0],
                egui::TextEdit::singleline(&mut self.capture_hotkey),
            );

            if response.changed() {
                self.mark_dirty();
            }

            let record_label = if self.recording_hotkey {
                "Recording"
            } else {
                "Record"
            };

            if ui.button(record_label).clicked() {
                self.recording_hotkey = !self.recording_hotkey;
                self.status = if self.recording_hotkey {
                    StatusMessage::info("Press a hotkey")
                } else {
                    StatusMessage::info("Hotkey recording cancelled")
                };
            }
        });
    }

    fn swaps_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Keyword Swaps");

        let mut remove_index = None;
        let mut changed = false;

        egui::Grid::new("keyword_swaps_grid")
            .num_columns(3)
            .spacing([12.0, 8.0])
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Heard");
                ui.strong("Use");
                ui.end_row();

                for (index, row) in self.swaps.iter_mut().enumerate() {
                    changed |= ui
                        .add_sized([260.0, 24.0], egui::TextEdit::singleline(&mut row.from))
                        .changed();
                    changed |= ui
                        .add_sized([260.0, 24.0], egui::TextEdit::singleline(&mut row.to))
                        .changed();

                    if ui.button("Remove").clicked() {
                        remove_index = Some(index);
                    }

                    ui.end_row();
                }
            });

        if let Some(index) = remove_index {
            self.swaps.remove(index);
            changed = true;
        }

        if changed {
            self.mark_dirty();
        }

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_sized(
                [260.0, 24.0],
                egui::TextEdit::singleline(&mut self.new_from).hint_text("peers"),
            );
            ui.add_sized(
                [260.0, 24.0],
                egui::TextEdit::singleline(&mut self.new_to).hint_text("PRs"),
            );

            let can_add = !self.new_from.trim().is_empty();
            if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                self.add_swap();
            }
        });
    }

    fn preview_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preview");

        ui.label("Input");
        ui.add_sized(
            [ui.available_width(), 72.0],
            egui::TextEdit::multiline(&mut self.preview_input),
        );

        ui.add_space(8.0);
        ui.label("Output");
        let mut output = self.preview_output();
        ui.add_enabled(
            false,
            egui::TextEdit::multiline(&mut output).desired_width(ui.available_width()),
        );
    }

    fn action_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("Save").clicked() {
                self.save();
            }

            if ui.button("Reload").clicked() {
                self.reload();
            }

            if ui.button("Reset").clicked() {
                self.reset_defaults();
            }

            ui.separator();
            self.status.show(ui);
        });
    }
}

impl eframe::App for SettingsApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.record_hotkey_from_events(context);

        if context.input_mut(|input| {
            input.consume_shortcut(&egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::S,
            ))
        }) {
            self.save();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            self.top_bar(ui);

            ui.add_space(12.0);
            self.hotkey_section(ui);

            ui.separator();
            self.swaps_section(ui);

            ui.separator();
            self.preview_section(ui);

            ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                self.action_bar(ui);
            });
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SwapRow {
    from: String,
    to: String,
}

impl From<KeywordSwapConfig> for SwapRow {
    fn from(swap: KeywordSwapConfig) -> Self {
        Self {
            from: swap.from,
            to: swap.to,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StatusMessage {
    text: String,
    kind: StatusKind,
}

impl StatusMessage {
    fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Info,
        }
    }

    fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Success,
        }
    }

    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: StatusKind::Error,
        }
    }

    fn show(&self, ui: &mut egui::Ui) {
        let color = match self.kind {
            StatusKind::Info => ui.visuals().text_color(),
            StatusKind::Success => egui::Color32::from_rgb(36, 128, 72),
            StatusKind::Error => egui::Color32::from_rgb(190, 56, 48),
        };

        ui.label(egui::RichText::new(&self.text).color(color));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusKind {
    Info,
    Success,
    Error,
}

fn format_hotkey(modifiers: egui::Modifiers, key: egui::Key) -> String {
    let mut parts = Vec::new();

    if modifiers.ctrl {
        parts.push("Ctrl".to_string());
    }

    if modifiers.alt {
        parts.push("Alt".to_string());
    }

    if modifiers.shift {
        parts.push("Shift".to_string());
    }

    if modifiers.mac_cmd {
        parts.push("Cmd".to_string());
    }

    parts.push(format!("{key:?}"));
    parts.join("+")
}
