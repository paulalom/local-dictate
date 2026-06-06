use std::fmt;
use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use local_dictate_core::{CaptureHotkey, DictationSettings, KeywordSwap, PostProcessingSettings};
use serde::{Deserialize, Serialize};

use crate::transcription_assets::{DEFAULT_ENGINE_ID, DEFAULT_MODEL_ID};

const CONFIG_FILE_NAME: &str = "settings.toml";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub capture_hotkey: String,
    pub engine: String,
    pub model: String,
    pub engine_path: String,
    pub model_path: String,
    pub language: String,
    #[serde(default = "default_true")]
    pub use_clipboard_insert: bool,
    pub cleanup_disfluencies: bool,
    #[serde(default = "default_true")]
    pub cleanup_revisions: bool,
    #[serde(default = "default_true")]
    pub append_trailing_space: bool,
    #[serde(default = "default_hide_to_tray")]
    pub hide_to_tray: bool,
    pub keyword_swaps: Vec<KeywordSwapConfig>,
}

impl AppConfig {
    pub fn to_settings(&self) -> Result<DictationSettings, ConfigError> {
        let capture_hotkey = CaptureHotkey::parse(self.capture_hotkey.clone())
            .map_err(|error| ConfigError::InvalidCaptureHotkey(error.to_string()))?;

        let keyword_swaps = self
            .keyword_swaps
            .iter()
            .enumerate()
            .map(|(index, swap)| {
                KeywordSwap::new(swap.from.clone(), swap.to.clone()).map_err(|error| {
                    ConfigError::InvalidKeywordSwap {
                        index,
                        message: error.to_string(),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(DictationSettings::new(
            capture_hotkey,
            PostProcessingSettings::new(keyword_swaps)
                .with_cleanup_disfluencies(self.cleanup_disfluencies)
                .with_cleanup_revisions(self.cleanup_revisions),
        ))
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            capture_hotkey: CaptureHotkey::default().to_string(),
            engine: DEFAULT_ENGINE_ID.to_string(),
            model: DEFAULT_MODEL_ID.to_string(),
            engine_path: String::new(),
            model_path: String::new(),
            language: "en".to_string(),
            use_clipboard_insert: true,
            cleanup_disfluencies: true,
            cleanup_revisions: true,
            append_trailing_space: true,
            hide_to_tray: default_hide_to_tray(),
            keyword_swaps: Vec::new(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_hide_to_tray() -> bool {
    true
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeywordSwapConfig {
    pub from: String,
    pub to: String,
}

impl From<&KeywordSwap> for KeywordSwapConfig {
    fn from(swap: &KeywordSwap) -> Self {
        Self {
            from: swap.from().to_string(),
            to: swap.to().to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub path: PathBuf,
}

pub fn load_or_default() -> Result<LoadedConfig, ConfigError> {
    let path = default_config_path()?;

    if !path.exists() {
        return Ok(LoadedConfig {
            config: AppConfig::default(),
            path,
        });
    }

    let text = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
        path: path.clone(),
        source: source.to_string(),
    })?;
    let config = toml::from_str(&text).map_err(|source| ConfigError::Parse {
        path: path.clone(),
        source: source.to_string(),
    })?;

    Ok(LoadedConfig { config, path })
}

pub fn save(config: &AppConfig) -> Result<PathBuf, ConfigError> {
    let path = default_config_path()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::CreateDirectory {
            path: parent.to_path_buf(),
            source: source.to_string(),
        })?;
    }

    let text = toml::to_string_pretty(config).map_err(|source| ConfigError::Serialize {
        source: source.to_string(),
    })?;

    fs::write(&path, text).map_err(|source| ConfigError::Write {
        path: path.clone(),
        source: source.to_string(),
    })?;

    Ok(path)
}

pub fn default_config_path() -> Result<PathBuf, ConfigError> {
    let project_dirs = ProjectDirs::from("dev", "Local Dictate", "Local Dictate")
        .ok_or(ConfigError::NoConfigDirectory)?;

    Ok(project_dirs.config_dir().join(CONFIG_FILE_NAME))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    NoConfigDirectory,
    CreateDirectory { path: PathBuf, source: String },
    Read { path: PathBuf, source: String },
    Parse { path: PathBuf, source: String },
    Serialize { source: String },
    Write { path: PathBuf, source: String },
    InvalidCaptureHotkey(String),
    InvalidKeywordSwap { index: usize, message: String },
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoConfigDirectory => write!(formatter, "could not locate an OS config directory"),
            Self::CreateDirectory { path, source } => {
                write!(formatter, "could not create {}: {source}", path.display())
            }
            Self::Read { path, source } => {
                write!(formatter, "could not read {}: {source}", path.display())
            }
            Self::Parse { path, source } => {
                write!(formatter, "could not parse {}: {source}", path.display())
            }
            Self::Serialize { source } => write!(formatter, "could not serialize config: {source}"),
            Self::Write { path, source } => {
                write!(formatter, "could not write {}: {source}", path.display())
            }
            Self::InvalidCaptureHotkey(message) => write!(formatter, "{message}"),
            Self::InvalidKeywordSwap { index, message } => {
                write!(
                    formatter,
                    "keyword swap {} is invalid: {message}",
                    index + 1
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::{AppConfig, KeywordSwapConfig};
    use crate::transcription_assets::{DEFAULT_ENGINE_ID, DEFAULT_MODEL_ID};

    #[test]
    fn default_config_uses_default_capture_hotkey() {
        assert_eq!(
            AppConfig::default().capture_hotkey,
            expected_default_capture_hotkey()
        );
    }

    #[test]
    fn converts_to_core_settings() {
        let config = AppConfig {
            capture_hotkey: "Ctrl+Shift+D".to_string(),
            engine: DEFAULT_ENGINE_ID.to_string(),
            model: DEFAULT_MODEL_ID.to_string(),
            engine_path: "engines/whisper-cli.exe".to_string(),
            model_path: "models/ggml-base.en.bin".to_string(),
            language: "en".to_string(),
            use_clipboard_insert: true,
            cleanup_disfluencies: true,
            cleanup_revisions: true,
            append_trailing_space: true,
            hide_to_tray: true,
            keyword_swaps: vec![KeywordSwapConfig {
                from: "peers".to_string(),
                to: "PRs".to_string(),
            }],
        };

        let settings = config.to_settings().unwrap();

        assert_eq!(settings.capture_hotkey().as_str(), "Ctrl+Shift+D");
        assert_eq!(
            settings
                .post_processing()
                .apply("Um, peers peers need review"),
            "PRs need review"
        );
        assert!(settings.post_processing().cleanup_disfluencies());
        assert!(settings.post_processing().cleanup_revisions());
    }

    #[test]
    fn rejects_invalid_keyword_swaps() {
        let config = AppConfig {
            capture_hotkey: "Ctrl+Shift+D".to_string(),
            engine: DEFAULT_ENGINE_ID.to_string(),
            model: DEFAULT_MODEL_ID.to_string(),
            engine_path: "engines/whisper-cli.exe".to_string(),
            model_path: "models/ggml-base.en.bin".to_string(),
            language: "en".to_string(),
            use_clipboard_insert: true,
            cleanup_disfluencies: false,
            cleanup_revisions: false,
            append_trailing_space: true,
            hide_to_tray: true,
            keyword_swaps: vec![KeywordSwapConfig {
                from: " ".to_string(),
                to: "PRs".to_string(),
            }],
        };

        assert_eq!(
            config.to_settings().unwrap_err().to_string(),
            "keyword swap 1 is invalid: keyword swap source cannot be empty"
        );
    }

    #[test]
    fn disabled_cleanup_flags_leave_post_processing_disabled() {
        let config = AppConfig {
            capture_hotkey: "Ctrl+Shift+D".to_string(),
            engine: DEFAULT_ENGINE_ID.to_string(),
            model: DEFAULT_MODEL_ID.to_string(),
            engine_path: String::new(),
            model_path: String::new(),
            language: "en".to_string(),
            use_clipboard_insert: true,
            cleanup_disfluencies: false,
            cleanup_revisions: false,
            append_trailing_space: true,
            hide_to_tray: true,
            keyword_swaps: Vec::new(),
        };

        let settings = config.to_settings().unwrap();
        let text = "Um, I'm gonna test, I want to test, I'm going to test this now.";

        assert!(!settings.post_processing().cleanup_disfluencies());
        assert!(!settings.post_processing().cleanup_revisions());
        assert_eq!(settings.post_processing().apply(text), text);
    }

    #[test]
    fn defaults_to_bundled_whisper_cpp_and_base_english() {
        let config = AppConfig::default();

        assert_eq!(config.engine, "bundled-whisper-cpp");
        assert_eq!(config.model, "base.en");
        assert!(config.engine_path.is_empty());
        assert!(config.model_path.is_empty());
        assert!(config.use_clipboard_insert);
        assert!(config.cleanup_disfluencies);
        assert!(config.cleanup_revisions);
        assert!(config.append_trailing_space);
        assert!(config.hide_to_tray);
    }

    #[test]
    fn loads_legacy_configs_without_engine_or_model_ids() {
        let config: AppConfig = toml::from_str(
            r#"
capture_hotkey = "Ctrl+Shift+D"
engine_path = "engines/whisper-cli.exe"
model_path = "models/ggml-base.en.bin"
language = "en"
auto_insert = true
keyword_swaps = []
"#,
        )
        .unwrap();

        assert_eq!(config.engine, "bundled-whisper-cpp");
        assert_eq!(config.model, "base.en");
        assert!(config.use_clipboard_insert);
        assert!(config.append_trailing_space);
        assert!(config.hide_to_tray);
        assert!(config.cleanup_disfluencies);
        assert!(config.cleanup_revisions);
        assert_eq!(config.engine_path, "engines/whisper-cli.exe");
        assert_eq!(config.model_path, "models/ggml-base.en.bin");
    }

    #[cfg(target_os = "windows")]
    fn expected_default_capture_hotkey() -> &'static str {
        "Ctrl+Win"
    }

    #[cfg(target_os = "macos")]
    fn expected_default_capture_hotkey() -> &'static str {
        "Ctrl+Cmd"
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    fn expected_default_capture_hotkey() -> &'static str {
        "Ctrl+Super"
    }
}
