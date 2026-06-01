use std::fmt;
use std::thread;
use std::time::Duration;

use arboard::{Clipboard, Error as ClipboardError};
use enigo::{
    Direction::{Click, Press, Release},
    Enigo, Key, Keyboard, Settings,
};

const PASTE_SETTLE_DELAY: Duration = Duration::from_millis(50);
const CLIPBOARD_RESTORE_DELAY: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInsertMode {
    Clipboard,
    Typing,
}

pub fn insert_text(text: &str, mode: TextInsertMode) -> Result<(), TextInputError> {
    if text.is_empty() {
        return Ok(());
    }

    if mode == TextInsertMode::Typing {
        return type_text(text);
    }

    if let Err(error) = paste_text(text) {
        let fallback_error = type_text(text).err();
        return match fallback_error {
            Some(fallback_error) => Err(TextInputError::Insert(format!(
                "clipboard paste failed ({error}); fallback typing failed ({fallback_error})"
            ))),
            None => Ok(()),
        };
    }

    Ok(())
}

fn paste_text(text: &str) -> Result<(), TextInputError> {
    let mut clipboard =
        Clipboard::new().map_err(|source| TextInputError::Clipboard(source.to_string()))?;
    let previous_text = match clipboard.get_text() {
        Ok(text) => Some(text),
        Err(error) if is_missing_clipboard_text(&error) => None,
        Err(error) => return Err(TextInputError::Clipboard(error.to_string())),
    };

    clipboard
        .set_text(text.to_string())
        .map_err(|source| TextInputError::Clipboard(source.to_string()))?;

    thread::sleep(PASTE_SETTLE_DELAY);
    let paste_result = send_paste_shortcut();

    if let Some(previous_text) = previous_text {
        thread::sleep(CLIPBOARD_RESTORE_DELAY);
        clipboard
            .set_text(previous_text)
            .map_err(|source| TextInputError::ClipboardRestore(source.to_string()))?;
    }

    paste_result
}

fn send_paste_shortcut() -> Result<(), TextInputError> {
    let mut enigo = new_enigo()?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Press)
        .map_err(|source| TextInputError::Insert(source.to_string()))?;
    let paste_result = enigo
        .key(Key::Unicode('v'), Click)
        .map_err(|source| TextInputError::Insert(source.to_string()));
    let release_result = enigo
        .key(modifier, Release)
        .map_err(|source| TextInputError::Insert(source.to_string()));

    paste_result?;
    release_result
}

fn type_text(text: &str) -> Result<(), TextInputError> {
    let mut enigo = new_enigo()?;
    enigo
        .text(text)
        .map_err(|source| TextInputError::Insert(source.to_string()))
}

fn new_enigo() -> Result<Enigo, TextInputError> {
    let enigo = Enigo::new(&Settings::default())
        .map_err(|source| TextInputError::Connect(source.to_string()))?;
    Ok(enigo)
}

fn is_missing_clipboard_text(error: &ClipboardError) -> bool {
    matches!(error, ClipboardError::ContentNotAvailable)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputError {
    Clipboard(String),
    ClipboardRestore(String),
    Connect(String),
    Insert(String),
}

impl fmt::Display for TextInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Clipboard(source) => write!(formatter, "could not use clipboard: {source}"),
            Self::ClipboardRestore(source) => {
                write!(formatter, "could not restore clipboard: {source}")
            }
            Self::Connect(source) => write!(
                formatter,
                "could not connect to text input system: {source}"
            ),
            Self::Insert(source) => write!(formatter, "could not insert dictated text: {source}"),
        }
    }
}

impl std::error::Error for TextInputError {}
