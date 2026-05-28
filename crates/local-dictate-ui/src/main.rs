#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app_tray;
mod audio;
mod config;
mod focus;
mod hotkey;
mod text_input;
mod transcription_assets;

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use app_tray::{AppTray, TrayAction};
use audio::AudioRecorder;
use config::{AppConfig, KeywordSwapConfig};
use eframe::egui;
use hotkey::{HotkeyEvent, HotkeyMonitor};
use local_dictate_core::{
    KeywordSwap, PostProcessingSettings, TranscriptionEngine, TranscriptionRequest,
    WhisperCliEngineOptions, WhisperCliTranscriptionEngine,
};
use transcription_assets::{
    CUSTOM_ENGINE_ID, CUSTOM_MODEL_ID, ENGINE_OPTIONS, MODEL_OPTIONS, engine_label, model_label,
    resolve_transcription_assets,
};

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
    engine: String,
    model: String,
    engine_path: String,
    model_path: String,
    language: String,
    auto_insert: bool,
    append_trailing_space: bool,
    swaps: Vec<SwapRow>,
    new_from: String,
    new_to: String,
    preview_input: String,
    status: StatusMessage,
    recording_hotkey: bool,
    dirty: bool,
    hotkey_monitor: Option<HotkeyMonitor>,
    hotkey_sender: mpsc::Sender<HotkeyEvent>,
    hotkey_events: mpsc::Receiver<HotkeyEvent>,
    runtime_events: mpsc::Receiver<RuntimeEvent>,
    runtime_sender: mpsc::Sender<RuntimeEvent>,
    recorder: Option<AudioRecorder>,
    capture_context: Option<CaptureContext>,
    runtime_state: RuntimeState,
    tray: Option<AppTray>,
    window_hidden: bool,
    exit_requested: bool,
}

impl SettingsApp {
    fn new() -> Self {
        let (hotkey_sender, hotkey_events) = mpsc::channel();
        let (runtime_sender, runtime_events) = mpsc::channel();

        match config::load_or_default() {
            Ok(loaded) => {
                let status = StatusMessage::info("Settings ready");
                Self::from_config(
                    loaded.config,
                    Some(loaded.path),
                    status,
                    hotkey_sender,
                    hotkey_events,
                    runtime_sender,
                    runtime_events,
                )
            }
            Err(error) => {
                let status = StatusMessage::error(format!("Using defaults: {error}"));
                Self::from_config(
                    AppConfig::default(),
                    config::default_config_path().ok(),
                    status,
                    hotkey_sender,
                    hotkey_events,
                    runtime_sender,
                    runtime_events,
                )
            }
        }
    }

    fn from_config(
        config: AppConfig,
        config_path: Option<PathBuf>,
        status: StatusMessage,
        hotkey_sender: mpsc::Sender<HotkeyEvent>,
        hotkey_events: mpsc::Receiver<HotkeyEvent>,
        runtime_sender: mpsc::Sender<RuntimeEvent>,
        runtime_events: mpsc::Receiver<RuntimeEvent>,
    ) -> Self {
        let hotkey_monitor =
            HotkeyMonitor::start(&config.capture_hotkey, hotkey_sender.clone()).ok();

        let tray = AppTray::new();
        let tray_status = tray.as_ref().err().cloned();

        let mut app = Self {
            config_path,
            capture_hotkey: config.capture_hotkey,
            engine: config.engine,
            model: config.model,
            engine_path: config.engine_path,
            model_path: config.model_path,
            language: config.language,
            auto_insert: config.auto_insert,
            append_trailing_space: config.append_trailing_space,
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
            hotkey_monitor,
            hotkey_sender,
            hotkey_events,
            runtime_events,
            runtime_sender,
            recorder: None,
            capture_context: None,
            runtime_state: RuntimeState::Idle,
            tray: tray.ok(),
            window_hidden: false,
            exit_requested: false,
        };

        if let Some(error) = tray_status {
            app.status = StatusMessage::error(format!("Tray unavailable: {error}"));
        } else if app.hotkey_monitor.is_none() {
            app.status = StatusMessage::error("Hotkey monitor could not start");
        }

        app
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
                self.apply_config(
                    loaded.config,
                    Some(loaded.path),
                    StatusMessage::success("Settings reloaded"),
                    false,
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
        self.apply_config(
            AppConfig::default(),
            config_path,
            StatusMessage::info("Defaults restored"),
            true,
        );
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
                self.update_hotkey_monitor(&config.capture_hotkey);
            }
            Err(error) => {
                self.status = StatusMessage::error(format!("Save failed: {error}"));
            }
        }
    }

    fn apply_config(
        &mut self,
        config: AppConfig,
        config_path: Option<PathBuf>,
        status: StatusMessage,
        dirty: bool,
    ) {
        self.config_path = config_path;
        self.capture_hotkey = config.capture_hotkey;
        self.engine = config.engine;
        self.model = config.model;
        self.engine_path = config.engine_path;
        self.model_path = config.model_path;
        self.language = config.language;
        self.auto_insert = config.auto_insert;
        self.append_trailing_space = config.append_trailing_space;
        self.swaps = config
            .keyword_swaps
            .into_iter()
            .map(SwapRow::from)
            .collect();
        self.status = status;
        self.recording_hotkey = false;
        self.dirty = dirty;
        self.update_hotkey_monitor(&self.capture_hotkey.clone());
    }

    fn update_hotkey_monitor(&mut self, hotkey: &str) {
        if let Some(monitor) = &self.hotkey_monitor
            && monitor.update_hotkey(hotkey).is_ok()
        {
            return;
        }

        match HotkeyMonitor::start(hotkey, self.hotkey_sender.clone()) {
            Ok(monitor) => {
                self.hotkey_monitor = Some(monitor);
            }
            Err(error) => {
                self.status = StatusMessage::error(format!("Hotkey disabled: {error}"));
                self.hotkey_monitor = None;
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
            engine: self.engine.trim().to_string(),
            model: self.model.trim().to_string(),
            engine_path: self.engine_path.trim().to_string(),
            model_path: self.model_path.trim().to_string(),
            language: self.language.trim().to_string(),
            auto_insert: self.auto_insert,
            append_trailing_space: self.append_trailing_space,
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

    fn poll_hotkey_events(&mut self) {
        while let Ok(event) = self.hotkey_events.try_recv() {
            if self.recording_hotkey {
                continue;
            }

            match event {
                HotkeyEvent::Pressed => self.begin_capture(),
                HotkeyEvent::Released => self.finish_capture(),
            }
        }
    }

    fn poll_runtime_events(&mut self) {
        while let Ok(event) = self.runtime_events.try_recv() {
            match event {
                RuntimeEvent::Inserted => {
                    self.runtime_state = RuntimeState::Idle;
                    self.status = StatusMessage::success("Inserted dictation");
                }
                RuntimeEvent::NoText => {
                    self.runtime_state = RuntimeState::Idle;
                    self.status = StatusMessage::info("No dictation detected");
                }
                RuntimeEvent::ProcessedWithoutInsert => {
                    self.runtime_state = RuntimeState::Idle;
                    self.status = StatusMessage::success("Processed dictation");
                }
                RuntimeEvent::Failed(error) => {
                    self.runtime_state = RuntimeState::Idle;
                    self.status = StatusMessage::error(error);
                }
            }
        }
    }

    fn poll_tray_events(&mut self, context: &egui::Context) {
        let Some(tray) = &self.tray else {
            return;
        };

        match tray.poll_action() {
            Some(TrayAction::Show) => self.show_window(context),
            Some(TrayAction::Exit) => self.exit_requested = true,
            None => {}
        }
    }

    fn handle_window_lifecycle(&mut self, context: &egui::Context) {
        if self.exit_requested {
            context.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        let close_requested = context.input(|input| input.viewport().close_requested());
        if close_requested {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window(context);
            return;
        }

        let minimized = context.input(|input| input.viewport().minimized == Some(true));
        if minimized && !self.window_hidden {
            self.hide_window(context);
        }
    }

    fn hide_window(&mut self, context: &egui::Context) {
        context.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        context.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        self.window_hidden = true;
        self.status = StatusMessage::info("Still running in the tray");
    }

    fn show_window(&mut self, context: &egui::Context) {
        context.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        context.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
        context.send_viewport_cmd(egui::ViewportCommand::Focus);
        self.window_hidden = false;
    }

    fn begin_capture(&mut self) {
        if self.runtime_state != RuntimeState::Idle {
            return;
        }

        let Ok(config) = self.current_config() else {
            return;
        };

        if let Err(error) = resolve_transcription_assets(
            &config.engine,
            &config.model,
            &config.engine_path,
            &config.model_path,
        ) {
            self.status = StatusMessage::error(error.to_string());
            return;
        }

        match AudioRecorder::start() {
            Ok(recorder) => {
                let focus_target = focus::current_focus_target();
                self.recorder = Some(recorder);
                self.capture_context = Some(CaptureContext {
                    config,
                    focus_target,
                });
                self.runtime_state = RuntimeState::Capturing;
                self.status = StatusMessage::info("Capturing");
            }
            Err(error) => {
                self.status = StatusMessage::error(format!("Capture failed: {error}"));
            }
        }
    }

    fn finish_capture(&mut self) {
        if self.runtime_state != RuntimeState::Capturing {
            return;
        }

        let Some(recorder) = self.recorder.take() else {
            self.runtime_state = RuntimeState::Idle;
            return;
        };

        match recorder.stop() {
            Ok(captured_audio) => {
                let Some(capture_context) = self.capture_context.take() else {
                    self.runtime_state = RuntimeState::Idle;
                    self.status = StatusMessage::error("Missing capture settings");
                    return;
                };

                self.runtime_state = RuntimeState::Processing;
                self.status = StatusMessage::info("Processing dictation");
                self.process_capture(captured_audio, capture_context);
            }
            Err(error) => {
                self.runtime_state = RuntimeState::Idle;
                self.capture_context = None;
                self.status = StatusMessage::error(format!("Capture failed: {error}"));
            }
        }
    }

    fn process_capture(
        &self,
        captured_audio: audio::CapturedAudio,
        capture_context: CaptureContext,
    ) {
        let sender = self.runtime_sender.clone();

        thread::spawn(move || {
            let event = process_capture(captured_audio, capture_context);
            let _ = sender.send(event);
        });
    }

    fn overlay(&self, context: &egui::Context) {
        let label = match self.runtime_state {
            RuntimeState::Capturing => "Capturing",
            RuntimeState::Processing => "Processing",
            RuntimeState::Idle => return,
        };
        let label = label.to_string();

        context.show_viewport_deferred(
            egui::ViewportId::from_hash_of("capture-overlay"),
            egui::ViewportBuilder::default()
                .with_title("Local Dictate Capture")
                .with_inner_size([260.0, 72.0])
                .with_resizable(false)
                .with_decorations(false)
                .with_transparent(true)
                .with_active(false)
                .with_always_on_top()
                .with_mouse_passthrough(true)
                .with_taskbar(false),
            move |ui, _class| {
                let rect = ui.max_rect();
                let painter = ui.painter();
                let pill = egui::Rect::from_center_size(rect.center(), egui::vec2(220.0, 48.0));

                painter.rect_filled(
                    pill,
                    egui::CornerRadius::same(8),
                    egui::Color32::from_rgba_unmultiplied(24, 28, 32, 232),
                );
                painter.circle_filled(
                    pill.left_center() + egui::vec2(24.0, 0.0),
                    6.0,
                    egui::Color32::from_rgb(222, 58, 64),
                );
                painter.text(
                    pill.center() + egui::vec2(12.0, 0.0),
                    egui::Align2::CENTER_CENTER,
                    label.as_str(),
                    egui::TextStyle::Button.resolve(ui.style()),
                    egui::Color32::WHITE,
                );
            },
        );
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

    fn engine_section(&mut self, ui: &mut egui::Ui) {
        ui.heading("Transcription");

        egui::Grid::new("engine_settings_grid")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label("Engine");
                let selected_engine = engine_label(&self.engine);
                let mut engine_changed = false;
                egui::ComboBox::from_id_salt("transcription_engine")
                    .selected_text(selected_engine)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for option in ENGINE_OPTIONS {
                            engine_changed |= ui
                                .selectable_value(
                                    &mut self.engine,
                                    option.id.to_string(),
                                    option.label,
                                )
                                .changed();
                        }
                    });
                if engine_changed {
                    self.mark_dirty();
                }
                ui.end_row();

                ui.label("Model");
                let selected_model = model_label(&self.model);
                let mut model_changed = false;
                egui::ComboBox::from_id_salt("transcription_model")
                    .selected_text(selected_model)
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for option in MODEL_OPTIONS {
                            model_changed |= ui
                                .selectable_value(
                                    &mut self.model,
                                    option.id.to_string(),
                                    option.label,
                                )
                                .changed();
                        }
                    });
                if model_changed {
                    self.mark_dirty();
                }
                ui.end_row();

                if self.engine == CUSTOM_ENGINE_ID || !self.engine_path.trim().is_empty() {
                    ui.label("Engine path");
                    if ui
                        .add_sized(
                            [ui.available_width(), 24.0],
                            egui::TextEdit::singleline(&mut self.engine_path)
                                .hint_text("engines/whisper-cli.exe"),
                        )
                        .changed()
                    {
                        self.mark_dirty();
                    }
                    ui.end_row();
                }

                if self.model == CUSTOM_MODEL_ID || !self.model_path.trim().is_empty() {
                    ui.label("Model path");
                    if ui
                        .add_sized(
                            [ui.available_width(), 24.0],
                            egui::TextEdit::singleline(&mut self.model_path)
                                .hint_text("models/ggml-base.en.bin"),
                        )
                        .changed()
                    {
                        self.mark_dirty();
                    }
                    ui.end_row();
                }

                ui.label("Language");
                if ui
                    .add_sized(
                        [160.0, 24.0],
                        egui::TextEdit::singleline(&mut self.language),
                    )
                    .changed()
                {
                    self.mark_dirty();
                }
                ui.end_row();

                ui.label("Insert");
                if ui.checkbox(&mut self.auto_insert, "Automatic").changed() {
                    self.mark_dirty();
                }
                ui.end_row();

                ui.label("Spacing");
                if ui
                    .checkbox(&mut self.append_trailing_space, "Add trailing space")
                    .changed()
                {
                    self.mark_dirty();
                }
                ui.end_row();
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
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Color32::TRANSPARENT.to_normalized_gamma_f32()
    }

    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        context.request_repaint_after(Duration::from_millis(25));
        self.poll_tray_events(context);
        self.handle_window_lifecycle(context);
        self.poll_hotkey_events();
        self.poll_runtime_events();
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
        self.overlay(ui.ctx());

        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            self.top_bar(ui);

            ui.add_space(12.0);
            self.hotkey_section(ui);

            ui.separator();
            self.engine_section(ui);

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuntimeState {
    Idle,
    Capturing,
    Processing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RuntimeEvent {
    Inserted,
    NoText,
    ProcessedWithoutInsert,
    Failed(String),
}

#[derive(Debug, Clone)]
struct CaptureContext {
    config: AppConfig,
    focus_target: Option<focus::FocusTarget>,
}

fn process_capture(
    captured_audio: audio::CapturedAudio,
    capture_context: CaptureContext,
) -> RuntimeEvent {
    match process_capture_inner(captured_audio, capture_context) {
        Ok(event) => event,
        Err(error) => RuntimeEvent::Failed(error),
    }
}

fn process_capture_inner(
    captured_audio: audio::CapturedAudio,
    capture_context: CaptureContext,
) -> Result<RuntimeEvent, String> {
    let config = capture_context.config;
    let settings = config.to_settings().map_err(|error| error.to_string())?;
    let assets = resolve_transcription_assets(
        &config.engine,
        &config.model,
        &config.engine_path,
        &config.model_path,
    )
    .map_err(|error| error.to_string())?;

    let engine = WhisperCliTranscriptionEngine::new(WhisperCliEngineOptions::new(
        assets.engine_path,
        assets.model_path,
    ));
    let mut request = TranscriptionRequest::new(captured_audio.path().to_path_buf());

    if !config.language.trim().is_empty() {
        request = request.with_language(config.language.trim().to_string());
    }

    let result = engine
        .transcribe(&request)
        .map_err(|error| error.to_string())?;
    let text = settings.post_processing().apply(&result.text);
    let text = text.trim();

    if text.is_empty() {
        return Ok(RuntimeEvent::NoText);
    }

    if config.auto_insert {
        focus::restore_focus(capture_context.focus_target);
        let text = insertion_text(text, config.append_trailing_space);
        text_input::insert_text(&text).map_err(|error| error.to_string())?;
        Ok(RuntimeEvent::Inserted)
    } else {
        Ok(RuntimeEvent::ProcessedWithoutInsert)
    }
}

fn insertion_text(text: &str, append_trailing_space: bool) -> String {
    if append_trailing_space {
        format!("{text} ")
    } else {
        text.to_string()
    }
}

fn format_hotkey(modifiers: egui::Modifiers, key: egui::Key) -> String {
    let mut parts = Vec::new();

    if modifiers.ctrl {
        parts.push("Ctrl".to_string());
    }

    if modifiers.alt {
        parts.push(alt_key_label().to_string());
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

#[cfg(target_os = "macos")]
fn alt_key_label() -> &'static str {
    "Option"
}

#[cfg(not(target_os = "macos"))]
fn alt_key_label() -> &'static str {
    "Alt"
}

#[cfg(test)]
mod tests {
    use super::insertion_text;

    #[test]
    fn appends_trailing_space_when_enabled() {
        assert_eq!(insertion_text("First sentence.", true), "First sentence. ");
    }

    #[test]
    fn leaves_inserted_text_unchanged_when_spacing_is_disabled() {
        assert_eq!(insertion_text("First sentence.", false), "First sentence.");
    }
}
