//! Only configured chords are recognized; no keystrokes are recorded.
use super::config::ConfigError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    TogglePause,
    Stop,
}
#[derive(Debug, Clone)]
pub struct HotkeyBinding {
    key: u32,
    modifiers: u8,
    action: HotkeyAction,
}
impl HotkeyBinding {
    pub fn parse(text: &str, action: HotkeyAction) -> Result<Self, ConfigError> {
        let parts: Vec<_> = text
            .split('+')
            .map(|part| part.trim().to_ascii_lowercase())
            .collect();
        let mut modifiers = 0;
        let mut key = None;
        for part in parts {
            let modifier = match part.as_str() {
                "shift" => 1,
                "ctrl" | "control" => 2,
                "alt" => 4,
                "win" | "windows" => 8,
                _ => 0,
            };
            if modifier != 0 {
                if modifiers & modifier != 0 {
                    return Err(ConfigError::value("duplicate hotkey modifier"));
                }
                modifiers |= modifier;
            } else {
                let code = virtual_key(&part)
                    .ok_or_else(|| ConfigError::value(format!("unsupported hotkey '{text}'")))?;
                if key.replace(code).is_some() {
                    return Err(ConfigError::value("one main key per hotkey required"));
                }
            }
        }
        Ok(Self {
            key: key.ok_or_else(|| ConfigError::value("hotkey main key required"))?,
            modifiers,
            action,
        })
    }
}
fn virtual_key(text: &str) -> Option<u32> {
    if text.len() == 1 {
        let value = text.as_bytes()[0];
        if value.is_ascii_alphanumeric() {
            return Some(u32::from(value.to_ascii_uppercase()));
        }
    }
    if let Some(number) = text.strip_prefix('f').and_then(|n| n.parse::<u32>().ok())
        && (1..=24).contains(&number)
    {
        return Some(0x6f + number);
    }
    Some(match text {
        "esc" | "escape" => 0x1b,
        "space" => 0x20,
        "tab" => 9,
        "enter" | "return" => 13,
        "pause" => 0x13,
        "up" => 0x26,
        "down" => 0x28,
        "left" => 0x25,
        "right" => 0x27,
        "home" => 0x24,
        "end" => 0x23,
        "insert" => 0x2d,
        "delete" | "del" => 0x2e,
        _ => return None,
    })
}
pub struct HotkeyTracker {
    bindings: Vec<HotkeyBinding>,
    down: [bool; 256],
}
impl HotkeyTracker {
    pub fn new(bindings: Vec<HotkeyBinding>) -> Result<Self, ConfigError> {
        if bindings.is_empty() || bindings.len() > 8 {
            return Err(ConfigError::value("one to eight hotkeys required"));
        }
        for (index, binding) in bindings.iter().enumerate() {
            if bindings[..index]
                .iter()
                .any(|other| other.key == binding.key && other.modifiers == binding.modifiers)
            {
                return Err(ConfigError::value("duplicate hotkey chord"));
            }
        }
        Ok(Self {
            bindings,
            down: [false; 256],
        })
    }
    pub fn seed(&mut self, key: u32, down: bool) {
        if let Some(value) = self.down.get_mut(key as usize) {
            *value = down;
        }
    }
    pub fn event(&mut self, key: u32, down: bool) -> Option<HotkeyAction> {
        self.seed(key, down);
        if !down {
            return None;
        }
        let pressed = |keys: &[usize]| keys.iter().any(|key| self.down[*key]);
        let modifiers = u8::from(pressed(&[0x10, 0xa0, 0xa1]))
            | (u8::from(pressed(&[0x11, 0xa2, 0xa3])) << 1)
            | (u8::from(pressed(&[0x12, 0xa4, 0xa5])) << 2)
            | (u8::from(pressed(&[0x5b, 0x5c])) << 3);
        let pressed_count = self.down.iter().filter(|down| **down).count();
        // Keep source key-down/repeat behavior. Do not swallow the original key.
        self.bindings
            .iter()
            .find(|binding| {
                binding.key == key
                    && binding.modifiers == modifiers
                    && pressed_count == 1 + binding.modifiers.count_ones() as usize
            })
            .map(|binding| binding.action)
    }
}
