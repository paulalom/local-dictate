use std::collections::HashSet;
use std::fmt;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use device_query::{DeviceQuery, DeviceState, Keycode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyEvent {
    Pressed,
    Released,
}

#[derive(Debug)]
pub struct HotkeyMonitor {
    binding: Arc<Mutex<HotkeyBinding>>,
}

impl HotkeyMonitor {
    pub fn start(hotkey: &str, sender: mpsc::Sender<HotkeyEvent>) -> Result<Self, HotkeyError> {
        let binding = Arc::new(Mutex::new(HotkeyBinding::parse(hotkey)?));
        let thread_binding = Arc::clone(&binding);

        thread::spawn(move || {
            let device_state = DeviceState::new();
            let mut active = false;

            loop {
                let pressed = device_state.get_keys();
                let pressed = pressed.iter().copied().collect::<HashSet<_>>();
                let binding = thread_binding.lock().map(|binding| binding.clone());
                let Ok(binding) = binding else {
                    break;
                };

                let is_down = binding.is_down(&pressed);

                if is_down && !active {
                    active = true;
                    if sender.send(HotkeyEvent::Pressed).is_err() {
                        break;
                    }
                } else if !is_down && active {
                    active = false;
                    if sender.send(HotkeyEvent::Released).is_err() {
                        break;
                    }
                }

                thread::sleep(Duration::from_millis(25));
            }
        });

        Ok(Self { binding })
    }

    pub fn update_hotkey(&self, hotkey: &str) -> Result<(), HotkeyError> {
        let parsed = HotkeyBinding::parse(hotkey)?;
        let mut binding = self.binding.lock().map_err(|_| HotkeyError::UpdateFailed)?;
        *binding = parsed;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    parts: Vec<HotkeyPart>,
}

impl HotkeyBinding {
    pub fn parse(value: &str) -> Result<Self, HotkeyError> {
        let trimmed = value.trim();

        if trimmed.is_empty() {
            return Err(HotkeyError::Empty);
        }

        let parts = trimmed
            .split('+')
            .map(parse_hotkey_part)
            .collect::<Result<Vec<_>, _>>()?;

        if parts.is_empty() {
            return Err(HotkeyError::Empty);
        }

        Ok(Self { parts })
    }

    fn is_down(&self, pressed: &HashSet<Keycode>) -> bool {
        self.parts
            .iter()
            .all(|part| part.any_key.iter().any(|key| pressed.contains(key)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyPart {
    any_key: Vec<Keycode>,
}

impl HotkeyPart {
    fn one(key: Keycode) -> Self {
        Self { any_key: vec![key] }
    }

    fn either(keys: &[Keycode]) -> Self {
        Self {
            any_key: keys.to_vec(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HotkeyError {
    Empty,
    EmptyPart(String),
    UnknownPart(String),
    UpdateFailed,
}

impl fmt::Display for HotkeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(formatter, "capture hotkey cannot be empty"),
            Self::EmptyPart(value) => write!(
                formatter,
                "capture hotkey contains an empty key part: {value}"
            ),
            Self::UnknownPart(value) => write!(formatter, "unknown capture hotkey part: {value}"),
            Self::UpdateFailed => write!(formatter, "could not update capture hotkey monitor"),
        }
    }
}

impl std::error::Error for HotkeyError {}

fn parse_hotkey_part(part: &str) -> Result<HotkeyPart, HotkeyError> {
    let token = part.trim();

    if token.is_empty() {
        return Err(HotkeyError::EmptyPart(part.to_string()));
    }

    let normalized = token.to_ascii_uppercase();
    let part = match normalized.as_str() {
        "ALT" | "OPTION" => HotkeyPart::either(&[
            Keycode::LAlt,
            Keycode::RAlt,
            Keycode::LOption,
            Keycode::ROption,
        ]),
        "CONTROL" | "CTRL" => HotkeyPart::either(&[Keycode::LControl, Keycode::RControl]),
        "SHIFT" => HotkeyPart::either(&[Keycode::LShift, Keycode::RShift]),
        "COMMAND" | "CMD" | "SUPER" | "WIN" | "WINDOWS" | "META" => HotkeyPart::either(&[
            Keycode::Command,
            Keycode::RCommand,
            Keycode::LMeta,
            Keycode::RMeta,
        ]),
        "SPACE" => HotkeyPart::one(Keycode::Space),
        "ENTER" | "RETURN" => HotkeyPart::one(Keycode::Enter),
        "ESC" | "ESCAPE" => HotkeyPart::one(Keycode::Escape),
        "TAB" => HotkeyPart::one(Keycode::Tab),
        "BACKSPACE" => HotkeyPart::one(Keycode::Backspace),
        "CAPSLOCK" => HotkeyPart::one(Keycode::CapsLock),
        "INSERT" => HotkeyPart::one(Keycode::Insert),
        "DELETE" => HotkeyPart::one(Keycode::Delete),
        "HOME" => HotkeyPart::one(Keycode::Home),
        "END" => HotkeyPart::one(Keycode::End),
        "PAGEUP" => HotkeyPart::one(Keycode::PageUp),
        "PAGEDOWN" => HotkeyPart::one(Keycode::PageDown),
        "UP" | "ARROWUP" => HotkeyPart::one(Keycode::Up),
        "DOWN" | "ARROWDOWN" => HotkeyPart::one(Keycode::Down),
        "LEFT" | "ARROWLEFT" => HotkeyPart::one(Keycode::Left),
        "RIGHT" | "ARROWRIGHT" => HotkeyPart::one(Keycode::Right),
        "0" | "KEY0" | "DIGIT0" => HotkeyPart::one(Keycode::Key0),
        "1" | "KEY1" | "DIGIT1" => HotkeyPart::one(Keycode::Key1),
        "2" | "KEY2" | "DIGIT2" => HotkeyPart::one(Keycode::Key2),
        "3" | "KEY3" | "DIGIT3" => HotkeyPart::one(Keycode::Key3),
        "4" | "KEY4" | "DIGIT4" => HotkeyPart::one(Keycode::Key4),
        "5" | "KEY5" | "DIGIT5" => HotkeyPart::one(Keycode::Key5),
        "6" | "KEY6" | "DIGIT6" => HotkeyPart::one(Keycode::Key6),
        "7" | "KEY7" | "DIGIT7" => HotkeyPart::one(Keycode::Key7),
        "8" | "KEY8" | "DIGIT8" => HotkeyPart::one(Keycode::Key8),
        "9" | "KEY9" | "DIGIT9" => HotkeyPart::one(Keycode::Key9),
        "A" | "KEYA" => HotkeyPart::one(Keycode::A),
        "B" | "KEYB" => HotkeyPart::one(Keycode::B),
        "C" | "KEYC" => HotkeyPart::one(Keycode::C),
        "D" | "KEYD" => HotkeyPart::one(Keycode::D),
        "E" | "KEYE" => HotkeyPart::one(Keycode::E),
        "F" | "KEYF" => HotkeyPart::one(Keycode::F),
        "G" | "KEYG" => HotkeyPart::one(Keycode::G),
        "H" | "KEYH" => HotkeyPart::one(Keycode::H),
        "I" | "KEYI" => HotkeyPart::one(Keycode::I),
        "J" | "KEYJ" => HotkeyPart::one(Keycode::J),
        "K" | "KEYK" => HotkeyPart::one(Keycode::K),
        "L" | "KEYL" => HotkeyPart::one(Keycode::L),
        "M" | "KEYM" => HotkeyPart::one(Keycode::M),
        "N" | "KEYN" => HotkeyPart::one(Keycode::N),
        "O" | "KEYO" => HotkeyPart::one(Keycode::O),
        "P" | "KEYP" => HotkeyPart::one(Keycode::P),
        "Q" | "KEYQ" => HotkeyPart::one(Keycode::Q),
        "R" | "KEYR" => HotkeyPart::one(Keycode::R),
        "S" | "KEYS" => HotkeyPart::one(Keycode::S),
        "T" | "KEYT" => HotkeyPart::one(Keycode::T),
        "U" | "KEYU" => HotkeyPart::one(Keycode::U),
        "V" | "KEYV" => HotkeyPart::one(Keycode::V),
        "W" | "KEYW" => HotkeyPart::one(Keycode::W),
        "X" | "KEYX" => HotkeyPart::one(Keycode::X),
        "Y" | "KEYY" => HotkeyPart::one(Keycode::Y),
        "Z" | "KEYZ" => HotkeyPart::one(Keycode::Z),
        "F1" => HotkeyPart::one(Keycode::F1),
        "F2" => HotkeyPart::one(Keycode::F2),
        "F3" => HotkeyPart::one(Keycode::F3),
        "F4" => HotkeyPart::one(Keycode::F4),
        "F5" => HotkeyPart::one(Keycode::F5),
        "F6" => HotkeyPart::one(Keycode::F6),
        "F7" => HotkeyPart::one(Keycode::F7),
        "F8" => HotkeyPart::one(Keycode::F8),
        "F9" => HotkeyPart::one(Keycode::F9),
        "F10" => HotkeyPart::one(Keycode::F10),
        "F11" => HotkeyPart::one(Keycode::F11),
        "F12" => HotkeyPart::one(Keycode::F12),
        "F13" => HotkeyPart::one(Keycode::F13),
        "F14" => HotkeyPart::one(Keycode::F14),
        "F15" => HotkeyPart::one(Keycode::F15),
        "F16" => HotkeyPart::one(Keycode::F16),
        "F17" => HotkeyPart::one(Keycode::F17),
        "F18" => HotkeyPart::one(Keycode::F18),
        "F19" => HotkeyPart::one(Keycode::F19),
        "F20" => HotkeyPart::one(Keycode::F20),
        _ => return Err(HotkeyError::UnknownPart(token.to_string())),
    };

    Ok(part)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use device_query::Keycode;

    use super::HotkeyBinding;

    #[test]
    fn supports_modifier_only_hotkeys() {
        let binding = HotkeyBinding::parse("Win+Alt").unwrap();
        let pressed = HashSet::from([Keycode::LMeta, Keycode::RAlt]);

        assert!(binding.is_down(&pressed));
    }

    #[test]
    fn supports_regular_key_hotkeys() {
        let binding = HotkeyBinding::parse("Ctrl+Shift+D").unwrap();
        let pressed = HashSet::from([Keycode::RControl, Keycode::LShift, Keycode::D]);

        assert!(binding.is_down(&pressed));
    }

    #[test]
    fn rejects_unknown_hotkey_parts() {
        assert_eq!(
            HotkeyBinding::parse("Ctrl+Banana").unwrap_err().to_string(),
            "unknown capture hotkey part: Banana"
        );
    }
}
