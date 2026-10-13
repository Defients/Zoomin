use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelAction {
    Zoom,
    Resize,
}
pub fn wheel_action(ctrl: bool, alt: bool, shift: bool) -> Option<WheelAction> {
    if ctrl && alt {
        Some(WheelAction::Zoom)
    } else if ctrl && shift {
        Some(WheelAction::Resize)
    } else {
        None
    }
}
#[cfg(test)]
mod wheel_tests {
    use super::*;
    #[test]
    fn resize_and_zoom_modifiers_are_distinct() {
        assert_eq!(wheel_action(true, false, true), Some(WheelAction::Resize));
        assert_eq!(wheel_action(true, true, false), Some(WheelAction::Zoom));
        assert_eq!(wheel_action(true, true, true), Some(WheelAction::Zoom));
        for (ctrl, alt, shift) in [
            (false, false, false),
            (true, false, false),
            (false, false, true),
            (false, true, true),
        ] {
            assert_eq!(wheel_action(ctrl, alt, shift), None);
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Shortcut {
    pub modifiers: u32,
    pub key: u32,
}
impl Shortcut {
    pub fn parse(chord: &str) -> Result<Self, String> {
        let mut modifiers = 0;
        let mut key = None;
        for token in chord.split('+').map(str::trim) {
            let bit = match token.to_ascii_lowercase().as_str() {
                "alt" => 1,
                "ctrl" | "control" => 2,
                "shift" => 4,
                "win" | "super" => 8,
                _ => 0,
            };
            if bit != 0 {
                if modifiers & bit != 0 {
                    return Err("Duplicate modifier".into());
                }
                modifiers |= bit;
            } else {
                let upper = token.to_ascii_uppercase();
                let k = if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphanumeric() {
                    upper.as_bytes()[0] as u32
                } else if let Some(n) = upper
                    .strip_prefix('F')
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|n| (1..=24).contains(n))
                {
                    0x70 + n - 1
                } else {
                    return Err(format!("Unsupported key: {token}. Use A–Z, 0–9 or F1–F24."));
                };
                if key.replace(k).is_some() {
                    return Err("Use one main key".into());
                }
            }
        }
        if modifiers & (1 | 2 | 8) == 0 {
            return Err("Use Ctrl, Alt or Win to avoid intercepting ordinary typing".into());
        }
        Ok(Self {
            modifiers,
            key: key.ok_or("Missing key")?,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parsing_normalizes() {
        assert_eq!(
            Shortcut::parse("Ctrl+Alt+Z").unwrap(),
            Shortcut::parse("alt+control+z").unwrap()
        );
        assert_eq!(Shortcut::parse("Ctrl+F12").unwrap().key, 0x7b);
    }
    #[test]
    fn rejects_ambiguous() {
        for s in ["Z", "Ctrl+Ctrl+Z", "Ctrl+Z+X", "Ctrl+", "Shift+A"] {
            assert!(Shortcut::parse(s).is_err(), "{s}");
        }
    }
}
