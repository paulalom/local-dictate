use std::collections::HashMap;
use std::env;
use std::path::PathBuf;

use local_dictate_core::{
    CaptureHotkey, DictationSettings, KeywordSwap, PostProcessingSettings, TranscriptionEngine,
    TranscriptionRequest, WhisperCliEngineOptions, WhisperCliTranscriptionEngine,
};

fn main() {
    let arguments = CliArguments::parse(env::args().skip(1));

    if arguments.show_help {
        print_usage();
        return;
    }

    match run(arguments) {
        Ok(text) => println!("{text}"),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run(arguments: CliArguments) -> Result<String, String> {
    let settings = arguments.dictation_settings()?;
    let engine = WhisperCliTranscriptionEngine::new(WhisperCliEngineOptions::new(
        arguments.required_path("engine")?,
        arguments.required_path("model")?,
    ));

    let mut request = TranscriptionRequest::new(arguments.required_path("audio")?);

    if let Some(language) = arguments.get("language") {
        request = request.with_language(language);
    }

    engine
        .transcribe(&request)
        .map(|result| settings.post_processing().apply(&result.text))
        .map_err(|error| error.to_string())
}

#[derive(Debug, Default)]
struct CliArguments {
    values: HashMap<String, Vec<String>>,
    show_help: bool,
}

impl CliArguments {
    fn parse(arguments: impl Iterator<Item = String>) -> Self {
        let args = arguments.collect::<Vec<_>>();
        let show_help = args.is_empty()
            || args
                .iter()
                .any(|argument| argument == "--help" || argument == "-h");

        let mut parsed = Self {
            values: HashMap::new(),
            show_help,
        };

        let mut index = 0;
        while index < args.len() {
            let argument = &args[index];

            if !argument.starts_with("--") {
                index += 1;
                continue;
            }

            let key = argument.trim_start_matches("--").to_string();
            let has_value = args
                .get(index + 1)
                .is_some_and(|next| !next.starts_with("--"));
            let value = if has_value {
                args[index + 1].clone()
            } else {
                "true".to_string()
            };

            if has_value {
                index += 1;
            }

            parsed.values.entry(key).or_default().push(value);
            index += 1;
        }

        parsed
    }

    fn get(&self, key: &str) -> Option<String> {
        self.values
            .get(key)
            .and_then(|values| values.first())
            .cloned()
    }

    fn get_all(&self, key: &str) -> Vec<String> {
        self.values.get(key).cloned().unwrap_or_default()
    }

    fn required_path(&self, key: &str) -> Result<PathBuf, String> {
        self.get(key)
            .map(PathBuf::from)
            .ok_or_else(|| format!("missing required argument --{key}"))
    }

    fn dictation_settings(&self) -> Result<DictationSettings, String> {
        let capture_hotkey = match self.get("capture-hotkey") {
            Some(value) => CaptureHotkey::parse(value).map_err(|error| error.to_string())?,
            None => CaptureHotkey::default(),
        };

        let mut swap_values = self.get_all("swap");
        swap_values.extend(self.get_all("keyword-swap"));

        let keyword_swaps = swap_values
            .iter()
            .map(|value| parse_keyword_swap(value))
            .collect::<Result<Vec<_>, _>>()?;

        let cleanup_disfluencies =
            self.flag_enabled("cleanup-disfluencies") || self.flag_enabled("clean-up-dictation");
        let cleanup_revisions =
            self.flag_enabled("cleanup-revisions") || self.flag_enabled("clean-up-revisions");

        Ok(DictationSettings::new(
            capture_hotkey,
            PostProcessingSettings::new(keyword_swaps)
                .with_cleanup_disfluencies(cleanup_disfluencies)
                .with_cleanup_revisions(cleanup_revisions),
        ))
    }

    fn flag_enabled(&self, key: &str) -> bool {
        self.values.get(key).is_some_and(|values| {
            values
                .first()
                .is_none_or(|value| !value.eq_ignore_ascii_case("false"))
        })
    }
}

fn parse_keyword_swap(value: &str) -> Result<KeywordSwap, String> {
    let Some((from, to)) = value.split_once('=') else {
        return Err(format!(
            "invalid keyword swap {value:?}; expected --swap <from=to>"
        ));
    };

    KeywordSwap::new(from.trim(), to.trim())
        .map_err(|error| format!("invalid keyword swap {value:?}: {error}"))
}

fn print_usage() {
    println!(
        "\
local-dictate CLI

Usage:
  local-dictate-cli --engine <whisper-cli> --model <ggml-model> --audio <wav-file> [--language en] [--capture-hotkey Ctrl+Win] [--cleanup-disfluencies] [--cleanup-revisions] [--swap peers=PRs]

Example:
  cargo run -p local-dictate-cli -- --engine .\\engines\\whisper-cli.exe --model .\\models\\ggml-small.en.bin --audio .\\recordings\\sample.wav --language en --capture-hotkey Ctrl+Win --cleanup-disfluencies --cleanup-revisions --swap peers=PRs"
    );
}
