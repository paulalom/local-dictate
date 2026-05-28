use std::fmt;

use crate::PostProcessingSettings;

#[cfg(target_os = "windows")]
const DEFAULT_CAPTURE_HOTKEY: &str = "Win+Alt";

#[cfg(target_os = "macos")]
const DEFAULT_CAPTURE_HOTKEY: &str = "Cmd+Option";

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
const DEFAULT_CAPTURE_HOTKEY: &str = "Super+Alt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictationSettings {
    capture_hotkey: CaptureHotkey,
    post_processing: PostProcessingSettings,
}

impl DictationSettings {
    pub fn new(capture_hotkey: CaptureHotkey, post_processing: PostProcessingSettings) -> Self {
        Self {
            capture_hotkey,
            post_processing,
        }
    }

    pub fn with_capture_hotkey(mut self, capture_hotkey: CaptureHotkey) -> Self {
        self.capture_hotkey = capture_hotkey;
        self
    }

    pub fn with_post_processing(mut self, post_processing: PostProcessingSettings) -> Self {
        self.post_processing = post_processing;
        self
    }

    pub fn capture_hotkey(&self) -> &CaptureHotkey {
        &self.capture_hotkey
    }

    pub fn post_processing(&self) -> &PostProcessingSettings {
        &self.post_processing
    }
}

impl Default for DictationSettings {
    fn default() -> Self {
        Self {
            capture_hotkey: CaptureHotkey::default(),
            post_processing: PostProcessingSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureHotkey {
    value: String,
}

impl CaptureHotkey {
    pub fn parse(value: impl Into<String>) -> Result<Self, SettingsError> {
        let value = normalize_hotkey(value.into())?;

        Ok(Self { value })
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }
}

impl Default for CaptureHotkey {
    fn default() -> Self {
        Self {
            value: DEFAULT_CAPTURE_HOTKEY.to_string(),
        }
    }
}

impl fmt::Display for CaptureHotkey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsError {
    EmptyCaptureHotkey,
    InvalidCaptureHotkey(String),
}

impl fmt::Display for SettingsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyCaptureHotkey => write!(formatter, "capture hotkey cannot be empty"),
            Self::InvalidCaptureHotkey(value) => {
                write!(
                    formatter,
                    "capture hotkey contains an empty key part: {value}"
                )
            }
        }
    }
}

impl std::error::Error for SettingsError {}

fn normalize_hotkey(value: String) -> Result<String, SettingsError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Err(SettingsError::EmptyCaptureHotkey);
    }

    let parts = trimmed.split('+').map(str::trim).collect::<Vec<_>>();

    if parts.iter().any(|part| part.is_empty()) {
        return Err(SettingsError::InvalidCaptureHotkey(value));
    }

    Ok(parts.join("+"))
}

#[cfg(test)]
mod tests {
    use super::{CaptureHotkey, DictationSettings};

    #[test]
    fn defaults_capture_hotkey() {
        assert_eq!(
            DictationSettings::default().capture_hotkey().as_str(),
            expected_default_capture_hotkey()
        );
    }

    #[test]
    fn normalizes_capture_hotkey_spacing() {
        assert_eq!(
            CaptureHotkey::parse(" Ctrl + Shift + D ").unwrap().as_str(),
            "Ctrl+Shift+D"
        );
    }

    #[test]
    fn rejects_empty_capture_hotkeys() {
        assert_eq!(
            CaptureHotkey::parse(" ").unwrap_err().to_string(),
            "capture hotkey cannot be empty"
        );
    }

    #[test]
    fn accepts_single_key_capture_hotkeys() {
        assert_eq!(CaptureHotkey::parse("F13").unwrap().as_str(), "F13");
    }

    #[test]
    fn rejects_capture_hotkeys_with_empty_parts() {
        assert_eq!(
            CaptureHotkey::parse("Ctrl++Space").unwrap_err().to_string(),
            "capture hotkey contains an empty key part: Ctrl++Space"
        );
    }

    #[cfg(target_os = "windows")]
    fn expected_default_capture_hotkey() -> &'static str {
        "Win+Alt"
    }

    #[cfg(target_os = "macos")]
    fn expected_default_capture_hotkey() -> &'static str {
        "Cmd+Option"
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    fn expected_default_capture_hotkey() -> &'static str {
        "Super+Alt"
    }
}
