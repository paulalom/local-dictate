use std::fmt;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::TranscriptionEngine;

#[derive(Debug, Clone)]
pub struct WhisperCliEngineOptions {
    pub binary_path: PathBuf,
    pub model_path: PathBuf,
    pub extra_arguments: Vec<String>,
    pub no_timestamps: bool,
}

impl WhisperCliEngineOptions {
    pub fn new(binary_path: impl Into<PathBuf>, model_path: impl Into<PathBuf>) -> Self {
        Self {
            binary_path: binary_path.into(),
            model_path: model_path.into(),
            extra_arguments: Vec::new(),
            no_timestamps: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    pub audio_path: PathBuf,
    pub language: Option<String>,
    pub timeout: Duration,
}

impl TranscriptionRequest {
    pub fn new(audio_path: impl Into<PathBuf>) -> Self {
        Self {
            audio_path: audio_path.into(),
            language: None,
            timeout: Duration::from_secs(120),
        }
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }
}

#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub text: String,
    pub duration: Duration,
    pub engine_name: &'static str,
}

#[derive(Debug)]
pub enum TranscriptionError {
    MissingFile { label: &'static str, path: PathBuf },
    StartFailed(std::io::Error),
    WaitFailed(std::io::Error),
    TimedOut(Duration),
    NonZeroExit { code: Option<i32>, stderr: String },
    OutputReadFailed(String),
}

impl fmt::Display for TranscriptionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingFile { label, path } => {
                write!(formatter, "{label} not found: {}", path.display())
            }
            Self::StartFailed(error) => write!(formatter, "failed to start whisper-cli: {error}"),
            Self::WaitFailed(error) => {
                write!(formatter, "failed while waiting for whisper-cli: {error}")
            }
            Self::TimedOut(timeout) => {
                write!(formatter, "transcription timed out after {timeout:?}")
            }
            Self::NonZeroExit { code, stderr } => {
                write!(formatter, "whisper-cli exited with code {code:?}: {stderr}")
            }
            Self::OutputReadFailed(message) => {
                write!(formatter, "failed to read process output: {message}")
            }
        }
    }
}

impl std::error::Error for TranscriptionError {}

#[derive(Debug, Clone)]
pub struct WhisperCliTranscriptionEngine {
    options: WhisperCliEngineOptions,
}

impl WhisperCliTranscriptionEngine {
    pub fn new(options: WhisperCliEngineOptions) -> Self {
        Self { options }
    }

    fn build_command(&self, request: &TranscriptionRequest) -> Command {
        let mut command = Command::new(&self.options.binary_path);

        command
            .arg("-m")
            .arg(&self.options.model_path)
            .arg("-f")
            .arg(&request.audio_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if self.options.no_timestamps {
            command.arg("-nt");
        }

        if let Some(language) = &request.language {
            command.arg("-l").arg(language);
        }

        command.args(&self.options.extra_arguments);
        command
    }
}

impl TranscriptionEngine for WhisperCliTranscriptionEngine {
    fn transcribe(
        &self,
        request: &TranscriptionRequest,
    ) -> Result<TranscriptionResult, TranscriptionError> {
        ensure_file("Whisper CLI binary", &self.options.binary_path)?;
        ensure_file("Whisper model", &self.options.model_path)?;
        ensure_file("Audio file", &request.audio_path)?;

        let started = Instant::now();
        let mut child = self
            .build_command(request)
            .spawn()
            .map_err(TranscriptionError::StartFailed)?;

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_reader = read_pipe(stdout, "stdout");
        let stderr_reader = read_pipe(stderr, "stderr");

        let status = loop {
            if let Some(status) = child.try_wait().map_err(TranscriptionError::WaitFailed)? {
                break status;
            }

            if started.elapsed() >= request.timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Err(TranscriptionError::TimedOut(request.timeout));
            }

            thread::sleep(Duration::from_millis(20));
        };

        let stdout = stdout_reader.join().map_err(|_| {
            TranscriptionError::OutputReadFailed("stdout reader panicked".to_string())
        })??;
        let stderr = stderr_reader.join().map_err(|_| {
            TranscriptionError::OutputReadFailed("stderr reader panicked".to_string())
        })??;

        if !status.success() {
            return Err(TranscriptionError::NonZeroExit {
                code: status.code(),
                stderr,
            });
        }

        Ok(TranscriptionResult {
            text: stdout.trim().to_string(),
            duration: started.elapsed(),
            engine_name: "whisper-cli",
        })
    }
}

fn ensure_file(label: &'static str, path: &Path) -> Result<(), TranscriptionError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(TranscriptionError::MissingFile {
            label,
            path: path.to_path_buf(),
        })
    }
}

fn read_pipe(
    pipe: Option<impl Read + Send + 'static>,
    label: &'static str,
) -> thread::JoinHandle<Result<String, TranscriptionError>> {
    thread::spawn(move || {
        let Some(mut pipe) = pipe else {
            return Ok(String::new());
        };

        let mut output = String::new();
        pipe.read_to_string(&mut output)
            .map_err(|error| TranscriptionError::OutputReadFailed(format!("{label}: {error}")))?;
        Ok(output)
    })
}
