#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app_tray;
mod audio;
mod config;
mod focus;
mod hotkey;
mod icon;
mod single_instance;
mod text_input;
mod transcription_assets;

use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::Duration;

use app_tray::{AppTray, TrayAction};
use audio::AudioRecorder;
use config::{AppConfig, KeywordSwapConfig};
use eframe::egui;
use hotkey::{HotkeyEvent, HotkeyMonitor};
use icon::local_dictate_icon_data;
use local_dictate_core::{
    KeywordSwap, PostProcessingSettings, TranscriptionEngine, TranscriptionRequest,
    WhisperCliEngineOptions, WhisperCliTranscriptionEngine,
};
use single_instance::{InstanceCommand, SingleInstance, StartupAction};
use transcription_assets::{
    CUSTOM_ENGINE_ID, CUSTOM_MODEL_ID, ENGINE_OPTIONS, MODEL_OPTIONS, engine_label, model_label,
    resolve_transcription_assets,
};

fn main() -> eframe::Result {
    let single_instance = match single_instance::prepare_startup() {
        StartupAction::Run(single_instance) => single_instance,
        StartupAction::Exit => return Ok(()),
    };
    let mut single_instance = single_instance;
    let icon_data = local_dictate_icon_data();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([760.0, 620.0])
            .with_min_inner_size([560.0, 460.0])
            .with_icon(Arc::new(egui::IconData {
                rgba: icon_data.rgba,
                width: icon_data.width,
                height: icon_data.height,
            })),
        ..Default::default()
    };

    eframe::run_native(
        "Local Dictate Settings",
        native_options,
        Box::new(move |_creation_context| Ok(Box::new(SettingsApp::new(single_instance.take())))),
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
    use_clipboard_insert: bool,
    cleanup_disfluencies: bool,
    cleanup_revisions: bool,
    append_trailing_space: bool,
    hide_to_tray: bool,
    swaps: Vec<SwapRow>,
    new_from: String,
    new_to: String,
    preview_input: String,
    status: StatusMessage,
    recording_hotkey: bool,
    hotkey_monitor: Option<HotkeyMonitor>,
    hotkey_sender: mpsc::Sender<HotkeyEvent>,
    hotkey_events: mpsc::Receiver<HotkeyEvent>,
    runtime_events: mpsc::Receiver<RuntimeEvent>,
    runtime_sender: mpsc::Sender<RuntimeEvent>,
    recorder: Option<AudioRecorder>,
    capture_context: Option<CaptureContext>,
    runtime_state: RuntimeState,
    tray: Option<AppTray>,
    single_instance: Option<SingleInstance>,
    window_hidden: bool,
    exit_requested: bool,
}

impl SettingsApp {
    fn new(single_instance: Option<SingleInstance>) -> Self {
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
                    single_instance,
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
                    single_instance,
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
        single_instance: Option<SingleInstance>,
    ) -> Self {
        let hotkey_monitor =
            HotkeyMonitor::start(&config.capture_hotkey, hotkey_sender.clone()).ok();

        let (tray, tray_status) = initialize_app_tray();

        let mut app = Self {
            config_path,
            capture_hotkey: config.capture_hotkey,
            engine: config.engine,
            model: config.model,
            engine_path: config.engine_path,
            model_path: config.model_path,
            language: config.language,
            auto_insert: config.auto_insert,
            use_clipboard_insert: config.use_clipboard_insert,
            cleanup_disfluencies: config.cleanup_disfluencies,
            cleanup_revisions: config.cleanup_revisions,
            append_trailing_space: config.append_trailing_space,
            hide_to_tray: config.hide_to_tray,
            swaps: config
                .keyword_swaps
                .into_iter()
                .map(SwapRow::from)
                .collect(),
            new_from: String::new(),
            new_to: String::new(),
            preview_input: "Um, ask peers peers to review this before we merge. I said, this is now, I'm gonna like test, I want to test, this is, I'm going to test this now.".to_string(),
            status,
            recording_hotkey: false,
            hotkey_monitor,
            hotkey_sender,
            hotkey_events,
            runtime_events,
            runtime_sender,
            recorder: None,
            capture_context: None,
            runtime_state: RuntimeState::Idle,
            tray,
            single_instance,
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

    fn save_settings(&mut self) {
        let Ok(config) = self.current_config() else {
            return;
        };

        self.save_config(&config, StatusMessage::success("Settings saved"));
    }

    fn save_config(&mut self, config: &AppConfig, success_status: StatusMessage) {
        match config::save(config) {
            Ok(path) => {
                self.config_path = Some(path);
                self.status = success_status;
                self.update_hotkey_monitor(&config.capture_hotkey);
            }
            Err(error) => {
                self.status = StatusMessage::error(format!("Save failed: {error}"));
            }
        }
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
        self.save_settings();
    }

    fn reload(&mut self) {
        match config::load_or_default() {
            Ok(loaded) => {
                self.apply_config(
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
        let config = AppConfig::default();
        self.apply_config(
            config.clone(),
            config_path,
            StatusMessage::info("Defaults restored"),
        );
        self.save_config(&config, StatusMessage::success("Defaults restored"));
    }

    fn apply_config(
        &mut self,
        config: AppConfig,
        config_path: Option<PathBuf>,
        status: StatusMessage,
    ) {
        self.config_path = config_path;
        self.capture_hotkey = config.capture_hotkey;
        self.engine = config.engine;
        self.model = config.model;
        self.engine_path = config.engine_path;
        self.model_path = config.model_path;
        self.language = config.language;
        self.auto_insert = config.auto_insert;
        self.use_clipboard_insert = config.use_clipboard_insert;
        self.cleanup_disfluencies = config.cleanup_disfluencies;
        self.cleanup_revisions = config.cleanup_revisions;
        self.append_trailing_space = config.append_trailing_space;
        self.hide_to_tray = config.hide_to_tray;
        self.swaps = config
            .keyword_swaps
            .into_iter()
            .map(SwapRow::from)
            .collect();
        self.status = status;
        self.recording_hotkey = false;
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
            use_clipboard_insert: self.use_clipboard_insert,
            cleanup_disfluencies: self.cleanup_disfluencies,
            cleanup_revisions: self.cleanup_revisions,
            append_trailing_space: self.append_trailing_space,
            hide_to_tray: self.hide_to_tray,
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

        PostProcessingSettings::new(swaps)
            .with_cleanup_disfluencies(self.cleanup_disfluencies)
            .with_cleanup_revisions(self.cleanup_revisions)
            .apply(&self.preview_input)
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

    fn poll_single_instance_commands(&mut self, context: &egui::Context) {
        let Some(single_instance) = &mut self.single_instance else {
            return;
        };

        match single_instance.poll_command() {
            Some(InstanceCommand::Show) => self.show_window(context),
            Some(InstanceCommand::Exit) => self.exit_requested = true,
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
            if self.should_hide_to_tray() {
                context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.hide_window(context);
            }
            return;
        }

        let minimized = context.input(|input| input.viewport().minimized == Some(true));
        if self.should_hide_to_tray() && minimized && !self.window_hidden {
            self.hide_window(context);
        }
    }

    fn should_hide_to_tray(&self) -> bool {
        self.hide_to_tray && self.tray.is_some()
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
            self.save_settings();
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Local Dictate");
            ui.add_space(8.0);
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
                self.save_settings();
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
                    self.save_settings();
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
                    self.save_settings();
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
                        self.save_settings();
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
                        self.save_settings();
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
                    self.save_settings();
                }
                ui.end_row();

                ui.label("Insert");
                if ui.checkbox(&mut self.auto_insert, "Automatic").changed() {
                    self.save_settings();
                }
                ui.end_row();

                ui.label("Paste");
                if ui
                    .add_enabled(
                        self.auto_insert,
                        egui::Checkbox::new(&mut self.use_clipboard_insert, "Use clipboard"),
                    )
                    .changed()
                {
                    self.save_settings();
                }
                ui.end_row();

                ui.label("Cleanup");
                if ui
                    .checkbox(
                        &mut self.cleanup_disfluencies,
                        "Remove stutters and fillers",
                    )
                    .changed()
                {
                    self.save_settings();
                }
                ui.end_row();

                ui.label("Revisions");
                if ui
                    .checkbox(&mut self.cleanup_revisions, "Remove abandoned rewrites")
                    .changed()
                {
                    self.save_settings();
                }
                ui.end_row();

                ui.label("Spacing");
                if ui
                    .checkbox(&mut self.append_trailing_space, "Add trailing space")
                    .changed()
                {
                    self.save_settings();
                }
                ui.end_row();

                if self.tray.is_some() {
                    ui.label("Window");
                    if ui
                        .checkbox(&mut self.hide_to_tray, "Hide to tray when closed")
                        .changed()
                    {
                        self.save_settings();
                    }
                    ui.end_row();
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
            self.save_settings();
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
        self.poll_single_instance_commands(context);
        self.handle_window_lifecycle(context);
        self.poll_hotkey_events();
        self.poll_runtime_events();
        self.record_hotkey_from_events(context);
        // App::ui is skipped while the settings window is hidden to the tray.
        self.overlay(context);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            self.top_bar(ui);

            let action_bar_height = 32.0;
            let settings_height = (ui.available_height() - action_bar_height).max(160.0);

            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .max_height(settings_height)
                .show(ui, |ui| {
                    ui.add_space(12.0);
                    self.hotkey_section(ui);

                    ui.separator();
                    self.engine_section(ui);

                    ui.separator();
                    self.swaps_section(ui);

                    ui.separator();
                    self.preview_section(ui);
                });

            ui.separator();
            self.action_bar(ui);
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

#[cfg(target_os = "linux")]
fn initialize_app_tray() -> (Option<AppTray>, Option<String>) {
    (None, None)
}

#[cfg(not(target_os = "linux"))]
fn initialize_app_tray() -> (Option<AppTray>, Option<String>) {
    match AppTray::new() {
        Ok(tray) => (Some(tray), None),
        Err(error) => (None, Some(error)),
    }
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
        let insert_mode = if config.use_clipboard_insert {
            text_input::TextInsertMode::Clipboard
        } else {
            text_input::TextInsertMode::Typing
        };
        text_input::insert_text(&text, insert_mode).map_err(|error| error.to_string())?;
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
