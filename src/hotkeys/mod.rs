use anyhow::Context;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
};

pub const HK_OPEN_PICKER: i32 = 1;
pub const HK_CAPTURE: i32 = 2;
pub const HK_QUIT_DEV: i32 = 99;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyRegistrationReport {
    pub registered: Vec<HotkeyBinding>,
    pub failed: Vec<HotkeyFailure>,
}

impl HotkeyRegistrationReport {
    fn new() -> Self {
        Self {
            registered: Vec::new(),
            failed: Vec::new(),
        }
    }

    pub fn open_picker_available(&self) -> bool {
        self.registered
            .iter()
            .any(|binding| binding.id == HK_OPEN_PICKER)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyBinding {
    pub id: i32,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyFailure {
    pub binding: HotkeyBinding,
    pub error: String,
}

pub fn register(hwnd: HWND) -> HotkeyRegistrationReport {
    let config = HotkeyConfig::load_or_default();
    register_config(hwnd, &config)
}

pub fn register_config(hwnd: HWND, config: &HotkeyConfig) -> HotkeyRegistrationReport {
    let mut report = HotkeyRegistrationReport::new();

    unsafe {
        register_spec(
            hwnd,
            HK_OPEN_PICKER,
            "open_picker",
            &config.open_picker,
            &mut report,
        );
        register_spec(hwnd, HK_CAPTURE, "capture", &config.capture, &mut report);

        // Dev-only: Win + Alt + Q to quit.
        #[cfg(debug_assertions)]
        register_spec(hwnd, HK_QUIT_DEV, "quit_dev", &config.quit_dev, &mut report);
    }

    for failure in &report.failed {
        tracing::warn!(
            hotkey = failure.binding.label,
            error = %failure.error,
            "hotkey unavailable"
        );
    }
    if !report.open_picker_available() {
        tracing::warn!("picker hotkey is unavailable; use the tray icon menu or Settings");
    }

    report
}

pub fn apply_config(hwnd: HWND, config: &HotkeyConfig) -> HotkeyRegistrationReport {
    unregister(hwnd);
    register_config(hwnd, config)
}

pub fn unregister(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HK_OPEN_PICKER);
        let _ = UnregisterHotKey(Some(hwnd), HK_CAPTURE);
        #[cfg(debug_assertions)]
        let _ = UnregisterHotKey(Some(hwnd), HK_QUIT_DEV);
    }
}

unsafe fn register_one(
    hwnd: HWND,
    binding: HotkeyBinding,
    modifiers: HOT_KEY_MODIFIERS,
    vk: u32,
    report: &mut HotkeyRegistrationReport,
) {
    match RegisterHotKey(Some(hwnd), binding.id, modifiers, vk) {
        Ok(()) => report.registered.push(binding),
        Err(err) => report.failed.push(HotkeyFailure {
            binding,
            error: err.message(),
        }),
    }
}

unsafe fn register_spec(
    hwnd: HWND,
    id: i32,
    name: &'static str,
    spec: &HotkeySpec,
    report: &mut HotkeyRegistrationReport,
) {
    let binding = HotkeyBinding {
        id,
        label: spec.label(),
    };

    let Some(modifiers) = spec.modifiers() else {
        report.failed.push(HotkeyFailure {
            binding,
            error: format!("invalid modifiers for {name}"),
        });
        return;
    };
    let Some(vk) = spec.vk() else {
        report.failed.push(HotkeyFailure {
            binding,
            error: format!("invalid key for {name}"),
        });
        return;
    };

    register_one(hwnd, binding, modifiers, vk, report);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub open_picker: HotkeySpec,
    pub capture: HotkeySpec,
    pub quit_dev: HotkeySpec,
}

impl HotkeyConfig {
    pub fn default_config() -> Self {
        Self {
            open_picker: HotkeySpec::new(&["ctrl", "win"], "V"),
            capture: HotkeySpec::new(&["win", "ctrl", "alt"], "C"),
            quit_dev: HotkeySpec::new(&["win", "ctrl", "alt"], "Q"),
        }
    }

    pub fn load_or_default() -> Self {
        let default = Self::default_config();
        let Some(path) = config_path() else {
            return default;
        };

        if !path.exists() {
            if let Err(err) = write_default_config(&path, &default) {
                tracing::warn!(error = ?err, path = %path.display(), "failed to write default hotkey config");
            }
            return default;
        }

        match fs::read_to_string(&path)
            .with_context(|| format!("read {}", path.display()))
            .and_then(|s| serde_json::from_str::<Self>(&s).context("parse hotkeys.json"))
        {
            Ok(config) => config,
            Err(err) => {
                tracing::warn!(
                    error = ?err,
                    path = %path.display(),
                    "failed to load hotkey config; using defaults"
                );
                default
            }
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let Some(path) = config_path() else {
            anyhow::bail!("config directory unavailable");
        };
        write_default_config(&path, self)
    }

    pub fn reset_defaults(&mut self) {
        *self = Self::default_config();
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        self.open_picker
            .validate()
            .context("invalid open picker hotkey")?;
        self.capture.validate().context("invalid capture hotkey")?;
        self.quit_dev.validate().context("invalid quit hotkey")?;
        Ok(())
    }
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

impl HotkeyRegistrationReport {
    pub fn required_hotkeys_available(&self) -> bool {
        self.registered
            .iter()
            .any(|binding| binding.id == HK_OPEN_PICKER)
            && self
                .registered
                .iter()
                .any(|binding| binding.id == HK_CAPTURE)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeySpec {
    pub modifiers: Vec<String>,
    pub key: String,
}

impl HotkeySpec {
    pub fn new(modifiers: &[&str], key: &str) -> Self {
        Self {
            modifiers: modifiers.iter().map(|s| s.to_string()).collect(),
            key: key.to_string(),
        }
    }

    pub fn from_owned(modifiers: Vec<String>, key: String) -> Self {
        Self { modifiers, key }
    }

    pub fn label(&self) -> String {
        let mut parts: Vec<String> = self
            .modifiers
            .iter()
            .map(|m| normalize_modifier_name(m))
            .collect();
        parts.push(self.key.to_uppercase());
        parts.join("+")
    }

    pub fn modifiers(&self) -> Option<HOT_KEY_MODIFIERS> {
        let mut value = HOT_KEY_MODIFIERS(0);
        for modifier in &self.modifiers {
            value |= match modifier.trim().to_ascii_lowercase().as_str() {
                "win" | "windows" | "super" => MOD_WIN,
                "ctrl" | "control" => MOD_CONTROL,
                "alt" => MOD_ALT,
                "shift" => MOD_SHIFT,
                _ => return None,
            };
        }
        Some(value)
    }

    pub fn vk(&self) -> Option<u32> {
        let key = self.key.trim().to_ascii_uppercase();
        if key.len() == 1 {
            let b = key.as_bytes()[0];
            if b.is_ascii_alphanumeric() {
                return Some(b as u32);
            }
        }

        if let Some(rest) = key.strip_prefix('F') {
            let n = rest.parse::<u32>().ok()?;
            if (1..=24).contains(&n) {
                return Some(0x70 + n - 1);
            }
        }

        None
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.modifiers().is_none() {
            anyhow::bail!("unknown modifier");
        }
        if self.modifiers.is_empty() {
            anyhow::bail!("at least one modifier is required");
        }
        if self.vk().is_none() {
            anyhow::bail!("unsupported key");
        }
        Ok(())
    }
}

fn normalize_modifier_name(modifier: &str) -> String {
    match modifier.trim().to_ascii_lowercase().as_str() {
        "win" | "windows" | "super" => "Win".to_string(),
        "ctrl" | "control" => "Ctrl".to_string(),
        "alt" => "Alt".to_string(),
        "shift" => "Shift".to_string(),
        other => other.to_string(),
    }
}

fn config_path() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("com", "ChargedMonk", "RipMultiPaste")?;
    Some(dirs.config_dir().join("hotkeys.json"))
}

fn write_default_config(path: &PathBuf, config: &HotkeyConfig) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(config)?;
    fs::write(path, json)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_marks_picker_available_when_registered() {
        let report = HotkeyRegistrationReport {
            registered: vec![HotkeyBinding {
                id: HK_OPEN_PICKER,
                label: "Win+Ctrl+Alt+V".to_string(),
            }],
            failed: Vec::new(),
        };

        assert!(report.open_picker_available());
    }

    #[test]
    fn report_marks_picker_unavailable_when_conflicted() {
        let report = HotkeyRegistrationReport {
            registered: vec![HotkeyBinding {
                id: HK_CAPTURE,
                label: "Win+Ctrl+Alt+C".to_string(),
            }],
            failed: vec![HotkeyFailure {
                binding: HotkeyBinding {
                    id: HK_OPEN_PICKER,
                    label: "Win+Ctrl+Alt+V".to_string(),
                },
                error: "Hot key is already registered. (0x80070581)".to_string(),
            }],
        };

        assert!(!report.open_picker_available());
        assert_eq!(report.failed[0].binding.id, HK_OPEN_PICKER);
    }

    #[test]
    fn default_hotkey_labels() {
        let config = HotkeyConfig::default_config();

        assert_eq!(config.open_picker.label(), "Ctrl+Win+V");
        assert_eq!(config.capture.label(), "Win+Ctrl+Alt+C");
    }

    #[test]
    fn hotkey_spec_parses_letters_and_function_keys() {
        assert_eq!(
            HotkeySpec::new(&["win", "ctrl", "alt"], "v").vk(),
            Some('V' as u32)
        );
        assert_eq!(HotkeySpec::new(&["ctrl", "alt"], "F12").vk(), Some(0x7B));
    }

    #[test]
    fn report_marks_required_hotkeys_available_only_when_both_registered() {
        let both = HotkeyRegistrationReport {
            registered: vec![
                HotkeyBinding {
                    id: HK_OPEN_PICKER,
                    label: "Win+Ctrl+Alt+V".to_string(),
                },
                HotkeyBinding {
                    id: HK_CAPTURE,
                    label: "Win+Ctrl+Alt+C".to_string(),
                },
            ],
            failed: Vec::new(),
        };
        let picker_only = HotkeyRegistrationReport {
            registered: vec![HotkeyBinding {
                id: HK_OPEN_PICKER,
                label: "Win+Ctrl+Alt+V".to_string(),
            }],
            failed: Vec::new(),
        };

        assert!(both.required_hotkeys_available());
        assert!(!picker_only.required_hotkeys_available());
    }

    #[test]
    fn hotkey_spec_normalizes_modifier_labels() {
        let spec = HotkeySpec::new(&[" windows ", "control", "super"], "f1");

        assert_eq!(spec.label(), "Win+Ctrl+Win+F1");
    }

    #[test]
    fn hotkey_spec_parses_modifier_bits() {
        let spec = HotkeySpec::new(&["win", "ctrl", "alt", "shift"], "K");
        let modifiers = spec.modifiers().unwrap();

        assert_eq!(modifiers.0, (MOD_WIN | MOD_CONTROL | MOD_ALT | MOD_SHIFT).0);
    }

    #[test]
    fn hotkey_spec_rejects_invalid_inputs() {
        assert!(HotkeySpec::new(&["hyper"], "V").validate().is_err());
        assert!(HotkeySpec::new(&[], "V").validate().is_err());
        assert!(HotkeySpec::new(&["ctrl"], "F25").validate().is_err());
        assert!(HotkeySpec::new(&["ctrl"], "Enter").validate().is_err());
    }

    #[test]
    fn hotkey_config_validate_reports_action_context() {
        let mut config = HotkeyConfig::default_config();
        config.capture = HotkeySpec::new(&["ctrl"], "F25");

        let err = config.validate().unwrap_err().to_string();
        assert!(err.contains("invalid capture hotkey"));
    }
}
