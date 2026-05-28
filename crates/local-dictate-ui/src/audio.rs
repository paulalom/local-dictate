use std::fmt;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SizedSample};

type WavWriterHandle = Arc<Mutex<Option<hound::WavWriter<BufWriter<File>>>>>;

pub struct AudioRecorder {
    path: PathBuf,
    temp_dir: tempfile::TempDir,
    stream: cpal::Stream,
    writer: WavWriterHandle,
    stream_error: Arc<Mutex<Option<String>>>,
}

impl fmt::Debug for AudioRecorder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AudioRecorder")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct CapturedAudio {
    path: PathBuf,
    _temp_dir: tempfile::TempDir,
}

impl CapturedAudio {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AudioRecorder {
    pub fn start() -> Result<Self, AudioError> {
        let temp_dir = tempfile::Builder::new()
            .prefix("local-dictate-capture-")
            .tempdir()
            .map_err(|source| AudioError::CreateTempDirectory(source.to_string()))?;
        let path = temp_dir.path().join("capture.wav");

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or(AudioError::NoInputDevice)?;
        let config = device
            .default_input_config()
            .map_err(|source| AudioError::DefaultInputConfig(source.to_string()))?;

        let spec = hound::WavSpec {
            channels: config.channels(),
            sample_rate: config.sample_rate(),
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer =
            hound::WavWriter::create(&path, spec).map_err(|source| AudioError::CreateWav {
                path: path.clone(),
                source: source.to_string(),
            })?;
        let writer = Arc::new(Mutex::new(Some(writer)));
        let stream_error = Arc::new(Mutex::new(None));

        let stream = build_stream(
            &device,
            &config,
            Arc::clone(&writer),
            Arc::clone(&stream_error),
        )?;
        stream
            .play()
            .map_err(|source| AudioError::StartStream(source.to_string()))?;

        Ok(Self {
            path,
            temp_dir,
            stream,
            writer,
            stream_error,
        })
    }

    pub fn stop(self) -> Result<CapturedAudio, AudioError> {
        let Self {
            path,
            temp_dir,
            stream,
            writer,
            stream_error,
        } = self;

        drop(stream);

        let mut guard = writer.lock().map_err(|_| AudioError::FinalizeFailed)?;
        let Some(writer) = guard.take() else {
            return Err(AudioError::FinalizeFailed);
        };

        writer
            .finalize()
            .map_err(|source| AudioError::FinalizeWav(source.to_string()))?;

        if let Ok(mut stream_error) = stream_error.lock()
            && let Some(error) = stream_error.take()
        {
            return Err(AudioError::Stream(error));
        }

        Ok(CapturedAudio {
            path,
            _temp_dir: temp_dir,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioError {
    CreateTempDirectory(String),
    NoInputDevice,
    DefaultInputConfig(String),
    UnsupportedSampleFormat(String),
    CreateWav { path: PathBuf, source: String },
    BuildStream(String),
    StartStream(String),
    Stream(String),
    FinalizeFailed,
    FinalizeWav(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CreateTempDirectory(source) => {
                write!(
                    formatter,
                    "could not create temporary capture storage: {source}"
                )
            }
            Self::NoInputDevice => write!(formatter, "no microphone input device was found"),
            Self::DefaultInputConfig(source) => {
                write!(
                    formatter,
                    "could not read microphone input config: {source}"
                )
            }
            Self::UnsupportedSampleFormat(format) => {
                write!(formatter, "unsupported microphone sample format: {format}")
            }
            Self::CreateWav { path, source } => {
                write!(formatter, "could not create {}: {source}", path.display())
            }
            Self::BuildStream(source) => {
                write!(formatter, "could not build microphone stream: {source}")
            }
            Self::StartStream(source) => {
                write!(formatter, "could not start microphone stream: {source}")
            }
            Self::Stream(source) => write!(formatter, "microphone stream failed: {source}"),
            Self::FinalizeFailed => write!(formatter, "could not finalize microphone recording"),
            Self::FinalizeWav(source) => {
                write!(formatter, "could not finalize WAV recording: {source}")
            }
        }
    }
}

impl std::error::Error for AudioError {}

fn build_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    writer: WavWriterHandle,
    stream_error: Arc<Mutex<Option<String>>>,
) -> Result<cpal::Stream, AudioError> {
    let err_fn = move |error: cpal::StreamError| {
        if let Ok(mut stream_error) = stream_error.lock() {
            *stream_error = Some(error.to_string());
        }
    };

    match config.sample_format() {
        cpal::SampleFormat::I8 => build_stream_for_sample::<i8>(device, config, writer, err_fn),
        cpal::SampleFormat::I16 => build_stream_for_sample::<i16>(device, config, writer, err_fn),
        cpal::SampleFormat::I24 => {
            build_stream_for_sample::<cpal::I24>(device, config, writer, err_fn)
        }
        cpal::SampleFormat::I32 => build_stream_for_sample::<i32>(device, config, writer, err_fn),
        cpal::SampleFormat::I64 => build_stream_for_sample::<i64>(device, config, writer, err_fn),
        cpal::SampleFormat::U8 => build_stream_for_sample::<u8>(device, config, writer, err_fn),
        cpal::SampleFormat::U16 => build_stream_for_sample::<u16>(device, config, writer, err_fn),
        cpal::SampleFormat::U24 => {
            build_stream_for_sample::<cpal::U24>(device, config, writer, err_fn)
        }
        cpal::SampleFormat::U32 => build_stream_for_sample::<u32>(device, config, writer, err_fn),
        cpal::SampleFormat::U64 => build_stream_for_sample::<u64>(device, config, writer, err_fn),
        cpal::SampleFormat::F32 => build_stream_for_sample::<f32>(device, config, writer, err_fn),
        cpal::SampleFormat::F64 => build_stream_for_sample::<f64>(device, config, writer, err_fn),
        format => Err(AudioError::UnsupportedSampleFormat(format.to_string())),
    }
}

fn build_stream_for_sample<T>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    writer: WavWriterHandle,
    err_fn: impl FnMut(cpal::StreamError) + Send + 'static,
) -> Result<cpal::Stream, AudioError>
where
    T: Sample + SizedSample,
    i16: FromSample<T>,
{
    device
        .build_input_stream(
            &config.clone().into(),
            move |data: &[T], _| write_input_data(data, &writer),
            err_fn,
            None,
        )
        .map_err(|source| AudioError::BuildStream(source.to_string()))
}

fn write_input_data<T>(input: &[T], writer: &WavWriterHandle)
where
    T: Sample,
    i16: FromSample<T>,
{
    if let Ok(mut guard) = writer.try_lock()
        && let Some(writer) = guard.as_mut()
    {
        for &sample in input {
            let sample = i16::from_sample(sample);
            let _ = writer.write_sample(sample);
        }
    }
}
