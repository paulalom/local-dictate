mod post_processing;
mod settings;
mod whisper_cli;

pub use post_processing::{KeywordSwap, PostProcessingError, PostProcessingSettings};
pub use settings::{CaptureHotkey, DictationSettings, SettingsError};
pub use whisper_cli::{
    TranscriptionError, TranscriptionRequest, TranscriptionResult, WhisperCliEngineOptions,
    WhisperCliTranscriptionEngine,
};

pub trait TranscriptionEngine {
    fn transcribe(
        &self,
        request: &TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError>;
}
