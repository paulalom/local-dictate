use std::fmt;

use enigo::{Enigo, Keyboard, Settings};

pub fn insert_text(text: &str) -> Result<(), TextInputError> {
    if text.is_empty() {
        return Ok(());
    }

    let mut enigo = Enigo::new(&Settings::default())
        .map_err(|source| TextInputError::Connect(source.to_string()))?;
    enigo
        .text(text)
        .map_err(|source| TextInputError::Insert(source.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextInputError {
    Connect(String),
    Insert(String),
}

impl fmt::Display for TextInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Connect(source) => write!(
                formatter,
                "could not connect to text input system: {source}"
            ),
            Self::Insert(source) => write!(formatter, "could not insert dictated text: {source}"),
        }
    }
}

impl std::error::Error for TextInputError {}
