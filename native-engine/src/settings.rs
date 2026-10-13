use crate::{geometry::Point, input::Shortcut};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Keeps the last successfully saved configuration separate from runtime state.
/// Implicit flushes never replace a corrupt file with untouched fallback defaults.
pub struct SettingsStore {
    path: PathBuf,
    saved: Settings,
}
impl SettingsStore {
    pub fn new(path: PathBuf, baseline: Settings) -> Self {
        Self {
            path,
            saved: baseline,
        }
    }
    pub fn differs(&self, settings: &Settings) -> bool {
        self.saved != *settings
    }
    pub fn save_explicit(&mut self, settings: &Settings) -> Result<(), String> {
        settings.save(&self.path)?;
        self.saved = settings.clone();
        Ok(())
    }
    pub fn flush_if_changed(&mut self, settings: &Settings) -> Result<bool, String> {
        if !self.differs(settings) {
            return Ok(false);
        }
        self.save_explicit(settings)?;
        Ok(true)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub version: u32,
    pub zoom: f64,
    pub presets: [f64; 3],
    pub sensitivity: f64,
    pub tracking: String,
    pub smoothing_ms: f64,
    pub placement: String,
    pub offset: Point,
    pub width: f64,
    pub height: f64,
    pub border: u32,
    pub accent: String,
    pub click_through: bool,
    pub dock_edge: String,
    pub dock_size: f64,
    pub shortcuts: Vec<Binding>,
    pub crosshair: bool,
    pub grid: bool,
    pub grid_threshold: f64,
    pub pixel_perfect: bool,
    pub stream_profile: String,
    pub startup: bool,
    pub onboarding_done: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Binding {
    pub action: String,
    pub chord: String,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            version: 1,
            zoom: 4.,
            presets: [2., 4., 8.],
            sensitivity: 0.5,
            tracking: "instant".into(),
            smoothing_ms: 55.,
            placement: "follow".into(),
            offset: Point { x: 36., y: 36. },
            width: 360.,
            height: 240.,
            border: 2,
            accent: "#B58CFF".into(),
            click_through: true,
            dock_edge: "top".into(),
            dock_size: 220.,
            shortcuts: [
                ("toggle", "Ctrl+Alt+Z"),
                ("hold", "Ctrl+Alt+X"),
                ("preset1", "Ctrl+Alt+1"),
                ("preset2", "Ctrl+Alt+2"),
                ("preset3", "Ctrl+Alt+3"),
                ("pin", "Ctrl+Alt+P"),
                ("grid", "Ctrl+Alt+G"),
                ("dock", "Ctrl+Alt+D"),
            ]
            .iter()
            .map(|(a, c)| Binding {
                action: a.to_string(),
                chord: c.to_string(),
            })
            .collect(),
            crosshair: false,
            grid: false,
            grid_threshold: 8.,
            pixel_perfect: false,
            stream_profile: "visible".into(),
            startup: false,
            onboarding_done: false,
        }
    }
}
impl Settings {
    /// Scale floating dimensions together; docked mode adjusts only thickness.
    pub fn resize_lens(&mut self, wheel_steps: f64) {
        if !wheel_steps.is_finite() {
            return;
        }
        let factor = 1.1_f64.powf(wheel_steps.clamp(-100., 100.));
        if self.placement == "docked" {
            self.dock_size = (self.dock_size * factor).clamp(100., 600.);
        } else {
            let min = (160. / self.width).max(120. / self.height);
            let max = (1000. / self.width).min(800. / self.height);
            let scale = factor.clamp(min, max);
            self.width = (self.width * scale).clamp(160., 1000.);
            self.height = (self.height * scale).clamp(120., 800.);
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        fn range(v: f64, lo: f64, hi: f64, name: &str) -> Result<(), String> {
            if !v.is_finite() || v < lo || v > hi {
                Err(format!("{name} must be between {lo} and {hi}"))
            } else {
                Ok(())
            }
        }
        if self.version != 1 {
            return Err("Unsupported settings version".into());
        }
        range(self.zoom, 1., 12., "Zoom")?;
        for p in self.presets {
            range(p, 1., 12., "Preset")?
        }
        range(self.sensitivity, 0.1, 3., "Wheel sensitivity")?;
        range(self.smoothing_ms, 0., 200., "Smoothing")?;
        range(self.width, 160., 1000., "Width")?;
        range(self.height, 120., 800., "Height")?;
        range(self.dock_size, 100., 600., "Dock size")?;
        range(self.offset.x, -500., 500., "Offset X")?;
        range(self.offset.y, -500., 500., "Offset Y")?;
        range(self.grid_threshold, 2., 12., "Grid threshold")?;
        if self.border > 12 {
            return Err("Border must be 0–12 pixels".into());
        }
        if self.accent.len() != 7
            || !self.accent.starts_with('#')
            || u32::from_str_radix(&self.accent[1..], 16).is_err()
        {
            return Err("Accent must be a six-digit HEX color".into());
        }
        for (value, allowed) in [
            (&self.tracking, vec!["instant", "smooth", "fixed"]),
            (&self.placement, vec!["follow", "pinned", "docked"]),
            (&self.dock_edge, vec!["top", "bottom", "left", "right"]),
            (&self.stream_profile, vec!["visible", "local", "minimal"]),
        ] {
            if !allowed.contains(&value.as_str()) {
                return Err(format!("Invalid setting: {value}"));
            }
        }
        let actions = [
            "toggle", "hold", "preset1", "preset2", "preset3", "pin", "grid",
        ];
        if !(actions.len()..=actions.len() + 1).contains(&self.shortcuts.len()) {
            return Err("Seven core shortcuts and an optional dock shortcut are required".into());
        }
        let mut chords = std::collections::HashSet::new();
        let mut seen = std::collections::HashSet::new();
        for b in &self.shortcuts {
            if (!actions.contains(&b.action.as_str()) && b.action != "dock")
                || !seen.insert(&b.action)
            {
                return Err("Unknown or duplicate shortcut action".into());
            }
            let s = Shortcut::parse(&b.chord)?;
            if !chords.insert(s) {
                return Err(format!("Shortcut conflict: {}", b.chord));
            }
        }
        if actions
            .iter()
            .any(|action| !seen.iter().any(|s| s.as_str() == *action))
        {
            return Err("A required shortcut action is missing".into());
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(path).map_err(|e| e.to_string())?;
        let mut value: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("Corrupt settings: {e}"))?;
        // Schema 0 used `zoomFactor`; migration preserves its value.
        if value.get("version").and_then(|v| v.as_u64()) == Some(0) {
            if let Some(z) = value.get("zoomFactor").cloned() {
                value["zoom"] = z;
            }
            value["version"] = 1.into();
        }
        let mut settings: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        settings.validate()?;
        // Existing version-1 files retain every binding. Choose an unused chord
        // when the new default is already assigned to a different action.
        if !settings.shortcuts.iter().any(|b| b.action == "dock") {
            let occupied: Vec<_> = settings
                .shortcuts
                .iter()
                .map(|b| Shortcut::parse(&b.chord).unwrap())
                .collect();
            let candidates = std::iter::once("Ctrl+Alt+D".to_string())
                .chain((1..=24).map(|n| format!("Ctrl+Alt+F{n}")));
            let chord = candidates
                .into_iter()
                .find(|c| !occupied.contains(&Shortcut::parse(c).unwrap()))
                .unwrap();
            settings.shortcuts.push(Binding {
                action: "dock".into(),
                chord,
            });
        }
        Ok(settings)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let tmp = path.with_extension("tmp");
        use std::io::Write;
        let mut f = fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(
            serde_json::to_string_pretty(self)
                .map_err(|e| e.to_string())?
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        drop(f);
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::{
                core::PCWSTR,
                Win32::Storage::FileSystem::{
                    MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
                },
            };
            let a: Vec<u16> = tmp.as_os_str().encode_wide().chain(Some(0)).collect();
            let b: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            unsafe {
                MoveFileExW(
                    PCWSTR(a.as_ptr()),
                    PCWSTR(b.as_ptr()),
                    MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
                )
            }
            .map_err(|e| e.to_string())?;
        }
        #[cfg(not(windows))]
        fs::rename(tmp, path).map_err(|e| e.to_string())?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wheel_resize_preserves_ratio_bounds_and_zoom() {
        let mut s = Settings::default();
        let zoom = s.zoom;
        s.resize_lens(1.);
        assert!((s.width - 396.).abs() < 1e-9);
        s.resize_lens(-1.);
        assert!((s.width - 360.).abs() < 1e-9);
        for steps in [100., -100., f64::INFINITY] {
            s.resize_lens(steps);
            assert!(s.validate().is_ok());
            assert!((s.width / s.height - 1.5).abs() < 1e-9);
            assert_eq!(s.zoom, zoom);
        }
        s.placement = "docked".into();
        let size = (s.width, s.height);
        s.resize_lens(100.);
        assert_eq!(s.dock_size, 600.);
        s.resize_lens(-100.);
        assert_eq!(s.dock_size, 100.);
        assert_eq!((s.width, s.height), size);
    }
    #[test]
    fn roundtrip() {
        let s = Settings::default();
        let r: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(s, r);
        assert!(r.validate().is_ok());
    }
    #[test]
    fn rejects_conflicts_and_nan() {
        let mut s = Settings {
            zoom: f64::NAN,
            ..Settings::default()
        };
        assert!(s.validate().is_err());
        s.zoom = 4.;
        s.shortcuts[1].chord = s.shortcuts[0].chord.clone();
        assert!(s.validate().is_err());
    }
    #[test]
    fn atomic_persistence_and_migration() {
        let p = std::env::temp_dir().join(format!("zoomin-test-{}.json", std::process::id()));
        let s = Settings::default();
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p).unwrap(), s);
        std::fs::write(&p, r#"{"version":0,"zoomFactor":6}"#).unwrap();
        assert_eq!(Settings::load(&p).unwrap().zoom, 6.);
        std::fs::remove_file(p).unwrap();
    }
    #[test]
    fn untouched_fallback_preserves_corrupt_file() {
        let path = std::env::temp_dir().join(format!("zoomin-corrupt-{}.json", std::process::id()));
        std::fs::write(&path, "corrupt original").unwrap();
        let defaults = Settings::default();
        let mut store = SettingsStore::new(path.clone(), defaults.clone());
        assert!(!store.flush_if_changed(&defaults).unwrap());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "corrupt original");
        store.save_explicit(&defaults).unwrap();
        assert_eq!(Settings::load(&path).unwrap(), defaults);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn failed_save_remains_dirty_for_retry() {
        let dir = std::env::temp_dir().join(format!("zoomin-save-retry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut store = SettingsStore::new(dir.clone(), Settings::default());
        let changed = Settings {
            zoom: 6.,
            ..Settings::default()
        };
        assert!(store.flush_if_changed(&changed).is_err());
        assert!(store.differs(&changed));
        std::fs::remove_dir(&dir).unwrap();
        store.flush_if_changed(&changed).unwrap();
        assert!(!store.differs(&changed));
        assert!(!store.flush_if_changed(&changed).unwrap());
        std::fs::remove_file(dir).unwrap();
    }
    #[test]
    fn legacy_shortcuts_gain_dock_without_losing_custom_bindings() {
        let path = std::env::temp_dir().join(format!("zoomin-dock-{}.json", std::process::id()));
        let mut legacy = Settings::default();
        legacy.shortcuts.retain(|b| b.action != "dock");
        legacy.shortcuts[0].chord = "Ctrl+Alt+D".into();
        legacy.save(&path).unwrap();
        let upgraded = Settings::load(&path).unwrap();
        assert_eq!(&upgraded.shortcuts[..7], legacy.shortcuts.as_slice());
        assert_eq!(upgraded.shortcuts[7].action, "dock");
        assert_ne!(upgraded.shortcuts[7].chord, "Ctrl+Alt+D");
        assert!(upgraded.validate().is_ok());
        std::fs::remove_file(path).unwrap();
    }
}
