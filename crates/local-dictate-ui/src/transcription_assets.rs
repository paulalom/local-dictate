use std::collections::HashSet;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};

pub const DEFAULT_ENGINE_ID: &str = "bundled-whisper-cpp";
pub const CUSTOM_ENGINE_ID: &str = "custom-whisper-cli";
pub const DEFAULT_MODEL_ID: &str = "small.en";
pub const CUSTOM_MODEL_ID: &str = "custom-ggml-model";

const ASSET_ROOT_ENV: &str = "LOCAL_DICTATE_ASSET_DIR";

#[derive(Debug, Clone, Copy)]
pub struct AssetOption {
    pub id: &'static str,
    pub label: &'static str,
}

pub const ENGINE_OPTIONS: &[AssetOption] = &[
    AssetOption {
        id: DEFAULT_ENGINE_ID,
        label: "Bundled whisper.cpp",
    },
    AssetOption {
        id: CUSTOM_ENGINE_ID,
        label: "Custom whisper-cli",
    },
];

pub const MODEL_OPTIONS: &[AssetOption] = &[
    AssetOption {
        id: "tiny.en",
        label: "Tiny English",
    },
    AssetOption {
        id: "base.en",
        label: "Base English",
    },
    AssetOption {
        id: DEFAULT_MODEL_ID,
        label: "Small English",
    },
    AssetOption {
        id: "base",
        label: "Base multilingual",
    },
    AssetOption {
        id: "small",
        label: "Small multilingual",
    },
    AssetOption {
        id: CUSTOM_MODEL_ID,
        label: "Custom ggml model",
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTranscriptionAssets {
    pub engine_path: PathBuf,
    pub model_path: PathBuf,
}

pub fn resolve_transcription_assets(
    engine_id: &str,
    model_id: &str,
    engine_path_override: &str,
    model_path_override: &str,
) -> Result<ResolvedTranscriptionAssets, AssetResolutionError> {
    let engine_path = match resolve_override("Whisper engine override", engine_path_override)? {
        Some(path) => path,
        None => bundled_engine_path(engine_id)?,
    };

    let model_path = match resolve_override("Whisper model override", model_path_override)? {
        Some(path) => path,
        None => bundled_model_path(model_id)?,
    };

    Ok(ResolvedTranscriptionAssets {
        engine_path,
        model_path,
    })
}

pub fn engine_label(id: &str) -> &'static str {
    option_label(ENGINE_OPTIONS, id).unwrap_or("Unknown engine")
}

pub fn model_label(id: &str) -> &'static str {
    option_label(MODEL_OPTIONS, id).unwrap_or("Unknown model")
}

pub fn default_engine_path_hint() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "engines/whisper-cli.exe"
    }

    #[cfg(not(target_os = "windows"))]
    {
        "engines/whisper-cli"
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetResolutionError {
    UnknownEngine(String),
    UnknownModel(String),
    MissingCustomEnginePath,
    MissingCustomModelPath,
    MissingAsset {
        label: &'static str,
        file_name: String,
        searched: Vec<PathBuf>,
    },
    MissingOverride {
        label: &'static str,
        path: PathBuf,
        searched: Vec<PathBuf>,
    },
}

impl fmt::Display for AssetResolutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEngine(engine) => {
                write!(formatter, "unknown transcription engine: {engine}")
            }
            Self::UnknownModel(model) => write!(formatter, "unknown transcription model: {model}"),
            Self::MissingCustomEnginePath => {
                write!(formatter, "custom whisper-cli requires an engine path")
            }
            Self::MissingCustomModelPath => {
                write!(formatter, "custom ggml model requires a model path")
            }
            Self::MissingAsset {
                label,
                file_name,
                searched,
            } => write!(
                formatter,
                "{label} not found: expected {file_name}; searched {}",
                format_paths(searched)
            ),
            Self::MissingOverride {
                label,
                path,
                searched,
            } => write!(
                formatter,
                "{label} not found: {}; searched {}",
                path.display(),
                format_paths(searched)
            ),
        }
    }
}

impl std::error::Error for AssetResolutionError {}

fn bundled_engine_path(engine_id: &str) -> Result<PathBuf, AssetResolutionError> {
    match normalized_engine_id(engine_id) {
        DEFAULT_ENGINE_ID => find_asset(
            "Bundled whisper.cpp engine",
            "engines",
            engine_binary_name(),
        ),
        CUSTOM_ENGINE_ID => Err(AssetResolutionError::MissingCustomEnginePath),
        other => Err(AssetResolutionError::UnknownEngine(other.to_string())),
    }
}

fn bundled_model_path(model_id: &str) -> Result<PathBuf, AssetResolutionError> {
    match normalized_model_id(model_id) {
        CUSTOM_MODEL_ID => Err(AssetResolutionError::MissingCustomModelPath),
        model if option_label(MODEL_OPTIONS, model).is_some() => {
            find_asset("Whisper model", "models", &model_file_name(model))
        }
        other => Err(AssetResolutionError::UnknownModel(other.to_string())),
    }
}

fn resolve_override(
    label: &'static str,
    value: &str,
) -> Result<Option<PathBuf>, AssetResolutionError> {
    let trimmed = value.trim();

    if trimmed.is_empty() {
        return Ok(None);
    }

    let path = PathBuf::from(trimmed);
    let candidates = override_candidates(&path);

    for candidate in &candidates {
        if candidate.is_file() {
            return Ok(Some(candidate.clone()));
        }
    }

    Err(AssetResolutionError::MissingOverride {
        label,
        path,
        searched: candidates,
    })
}

fn find_asset(
    label: &'static str,
    directory: &str,
    file_name: &str,
) -> Result<PathBuf, AssetResolutionError> {
    let searched = asset_roots()
        .into_iter()
        .map(|root| root.join(directory).join(file_name))
        .collect::<Vec<_>>();

    for candidate in &searched {
        if candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    Err(AssetResolutionError::MissingAsset {
        label,
        file_name: file_name.to_string(),
        searched,
    })
}

fn override_candidates(path: &Path) -> Vec<PathBuf> {
    if path.is_absolute() {
        return vec![path.to_path_buf()];
    }

    let mut candidates = Vec::new();
    push_unique(&mut candidates, path.to_path_buf());

    if let Ok(current_dir) = env::current_dir() {
        push_unique(&mut candidates, current_dir.join(path));
    }

    for root in asset_roots() {
        push_unique(&mut candidates, root.join(path));
    }

    candidates
}

fn asset_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(root) = env::var_os(ASSET_ROOT_ENV) {
        push_root_with_ancestors(&mut roots, PathBuf::from(root));
    }

    if let Ok(exe_path) = env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        if let Some(resources_dir) = macos_bundle_resource_root(exe_dir) {
            push_root_with_ancestors(&mut roots, resources_dir);
        }

        push_root_with_ancestors(&mut roots, exe_dir.to_path_buf());
    }

    if let Ok(current_dir) = env::current_dir() {
        push_root_with_ancestors(&mut roots, current_dir);
    }

    roots
}

fn macos_bundle_resource_root(exe_dir: &Path) -> Option<PathBuf> {
    if !path_file_name_eq(exe_dir, "MacOS") {
        return None;
    }

    let contents_dir = exe_dir.parent()?;
    if !path_file_name_eq(contents_dir, "Contents") {
        return None;
    }

    Some(contents_dir.join("Resources"))
}

fn path_file_name_eq(path: &Path, expected: &str) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == expected)
}

fn push_root_with_ancestors(roots: &mut Vec<PathBuf>, root: PathBuf) {
    let mut seen = roots.iter().cloned().collect::<HashSet<_>>();

    for ancestor in root.ancestors().take(8) {
        let candidate = ancestor.to_path_buf();

        if seen.insert(candidate.clone()) {
            roots.push(candidate);
        }
    }
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|candidate| candidate == &path) {
        paths.push(path);
    }
}

fn option_label(options: &[AssetOption], id: &str) -> Option<&'static str> {
    let normalized = normalize_id(id);

    options
        .iter()
        .find(|option| option.id == normalized)
        .map(|option| option.label)
}

fn normalized_engine_id(id: &str) -> &str {
    let normalized = normalize_id(id);

    if normalized.is_empty() {
        DEFAULT_ENGINE_ID
    } else {
        normalized
    }
}

fn normalized_model_id(id: &str) -> &str {
    let normalized = normalize_id(id);

    if normalized.is_empty() {
        DEFAULT_MODEL_ID
    } else {
        normalized
    }
}

fn normalize_id(id: &str) -> &str {
    id.trim()
}

fn engine_binary_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "whisper-cli.exe"
    }

    #[cfg(not(target_os = "windows"))]
    {
        "whisper-cli"
    }
}

fn model_file_name(model_id: &str) -> String {
    format!("ggml-{model_id}.bin")
}

fn format_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::{
        CUSTOM_MODEL_ID, DEFAULT_ENGINE_ID, DEFAULT_MODEL_ID, default_engine_path_hint,
        engine_label, macos_bundle_resource_root, model_file_name,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn labels_known_defaults() {
        assert_eq!(engine_label(DEFAULT_ENGINE_ID), "Bundled whisper.cpp");
        assert_eq!(model_file_name(DEFAULT_MODEL_ID), "ggml-small.en.bin");
    }

    #[test]
    fn default_engine_hint_uses_current_platform_binary_name() {
        #[cfg(target_os = "windows")]
        assert_eq!(default_engine_path_hint(), "engines/whisper-cli.exe");

        #[cfg(not(target_os = "windows"))]
        assert_eq!(default_engine_path_hint(), "engines/whisper-cli");
    }

    #[test]
    fn model_file_name_uses_ggml_convention() {
        assert_eq!(model_file_name("tiny.en"), "ggml-tiny.en.bin");
        assert_eq!(
            model_file_name(CUSTOM_MODEL_ID),
            "ggml-custom-ggml-model.bin"
        );
    }

    #[test]
    fn finds_macos_bundle_resources_from_executable_directory() {
        let exe_dir = Path::new("/Applications/Local Dictate.app/Contents/MacOS");

        assert_eq!(
            macos_bundle_resource_root(exe_dir),
            Some(PathBuf::from(
                "/Applications/Local Dictate.app/Contents/Resources"
            ))
        );
    }

    #[test]
    fn ignores_non_macos_bundle_executable_directories() {
        let exe_dir = Path::new("/usr/local/bin");

        assert_eq!(macos_bundle_resource_root(exe_dir), None);
    }
}
