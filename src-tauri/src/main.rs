#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use zoomin_engine::{
    settings::{Settings, SettingsStore},
    Action, Engine, Snapshot,
};

struct AppState {
    engine: Option<Engine>,
    fallback: Mutex<Snapshot>,
    store: Mutex<SettingsStore>,
    transaction: Mutex<()>,
}
impl AppState {
    fn snapshot(&self) -> Snapshot {
        let mut s = self
            .engine
            .as_ref()
            .map(Engine::snapshot)
            .unwrap_or_else(|| self.fallback.lock().unwrap().clone());
        if s.error.is_none() {
            s.error = self.fallback.lock().unwrap().error.clone();
        }
        s
    }
    fn engine(&self) -> Result<&Engine, String> {
        self.engine.as_ref().ok_or_else(|| {
            self.snapshot()
                .error
                .unwrap_or("Native engine unavailable".into())
        })
    }
}
#[tauri::command]
fn get_state(state: tauri::State<AppState>) -> Snapshot {
    state.snapshot()
}
fn apply(app: &tauri::AppHandle, s: Settings) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let _guard = state.transaction.lock().map_err(|e| e.to_string())?;
    s.validate()?;
    let previous = state.snapshot().settings;
    let startup = app.autolaunch();
    let previous_startup = startup.is_enabled().map_err(|e| e.to_string())?;
    state.engine()?.apply(s.clone())?;
    let result = (|| -> Result<(), String> {
        if s.startup != previous_startup {
            if s.startup {
                startup.enable()
            } else {
                startup.disable()
            }
            .map_err(|e| e.to_string())?;
        }
        state
            .store
            .lock()
            .map_err(|e| e.to_string())?
            .save_explicit(&s)
    })();
    if let Err(e) = result {
        let native_restore = state.engine()?.apply(previous);
        let startup_restore = if previous_startup {
            startup.enable()
        } else {
            startup.disable()
        };
        let mut errors = vec![e];
        if let Err(error) = native_restore {
            errors.push(format!("Native rollback failed: {error}"));
        }
        if let Err(error) = startup_restore {
            errors.push(format!("Startup rollback failed: {error}"));
        }
        return Err(errors.join("; "));
    }
    state.fallback.lock().unwrap().error = None;
    let snapshot = state.snapshot();
    let _ = app.emit("engine-state", &snapshot);
    Ok(snapshot)
}
#[tauri::command]
fn apply_settings(app: tauri::AppHandle, settings: Settings) -> Result<Snapshot, String> {
    apply(&app, settings)
}
#[tauri::command]
fn reset_settings(app: tauri::AppHandle) -> Result<Snapshot, String> {
    apply(
        &app,
        Settings {
            onboarding_done: true,
            ..Settings::default()
        },
    )
}
#[tauri::command]
fn perform_action(
    state: tauri::State<AppState>,
    action: String,
    value: Option<f64>,
) -> Result<(), String> {
    state.engine()?.action(parse_action(action, value)?)
}
fn parse_action(action: String, value: Option<f64>) -> Result<Action, String> {
    let a = match action.as_str() {
        "toggle" => Action::Toggle,
        "hide" => Action::Show(false),
        "show" => Action::Show(true),
        "pin" => Action::Pin,
        "dock" => Action::Dock,
        "grid" => Action::Grid,
        "ruler" | "region" => Action::Measure(action),
        "cancel_measure" => Action::CancelMeasure,
        "reset_measure" => Action::ResetMeasure,
        "zoom" => {
            let zoom = value.ok_or("Zoom value required")?;
            if !zoom.is_finite() {
                return Err("Zoom must be finite".into());
            }
            Action::Zoom(zoom)
        }
        "preset" => {
            let index = value.ok_or("Preset index required")?;
            if !index.is_finite() || index.fract() != 0. || !(0. ..3.).contains(&index) {
                return Err("Preset index must be 0, 1 or 2".into());
            }
            Action::Preset(index as usize)
        }
        _ => return Err("Unknown action".into()),
    };
    Ok(a)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preset_rejects_invalid_indices() {
        for value in [f64::NAN, f64::INFINITY, -1., 0.5, 3.] {
            assert!(parse_action("preset".into(), Some(value)).is_err());
        }
        for index in 0..3 {
            assert!(
                matches!(parse_action("preset".into(),Some(index as f64)),Ok(Action::Preset(i)) if i==index)
            );
        }
    }
    #[test]
    fn zoom_rejects_nonfinite_but_keeps_finite_clamping() {
        assert!(parse_action("zoom".into(), Some(f64::NAN)).is_err());
        assert!(matches!(
            parse_action("zoom".into(), Some(100.)),
            Ok(Action::Zoom(100.))
        ));
    }
}
#[tauri::command]
fn copy_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}
#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}
fn show_settings(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}
fn main() {
    let done = Arc::new(AtomicBool::new(false));
    let exit_flag = done.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_settings(app)
        }))
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args(["--background"])
                .build(),
        )
        .plugin(tauri_plugin_clipboard_manager::init())
        .invoke_handler(tauri::generate_handler![
            get_state,
            apply_settings,
            reset_settings,
            perform_action,
            copy_text,
            exit_app
        ])
        .setup(move |app| {
            let path = app.path().app_config_dir()?.join("settings.json");
            let (settings, warning) = match Settings::load(&path) {
                Ok(s) => (s, None),
                Err(e) => (
                    Settings::default(),
                    Some(format!(
                        "{e}. Defaults loaded; the original file remains until settings are saved."
                    )),
                ),
            };
            let mut exclusions = vec![];
            if let Some(w) = app.get_webview_window("main") {
                exclusions.push(w.hwnd()?.0 as isize);
            }
            let (engine, fallback) = match Engine::start(settings.clone(), exclusions) {
                Ok(e) => (
                    Some(e),
                    Snapshot {
                        settings: settings.clone(),
                        error: warning.clone(),
                        ..Snapshot::default()
                    },
                ),
                Err(e) => (
                    None,
                    Snapshot {
                        settings: settings.clone(),
                        error: Some(e),
                        ..Snapshot::default()
                    },
                ),
            };
            app.manage(AppState {
                engine,
                fallback: Mutex::new(fallback),
                store: Mutex::new(SettingsStore::new(path, settings.clone())),
                transaction: Mutex::new(()),
            });
            if let Some(warning) = warning {
                eprintln!("{warning}");
            }
            let launch = app.autolaunch();
            let startup_result = if settings.startup {
                launch.enable()
            } else {
                launch.disable()
            };
            if let Err(e) = startup_result {
                app.state::<AppState>().fallback.lock().unwrap().error =
                    Some(format!("Startup: {e}"));
            }
            let entries = [
                ("toggle", "Show / hide lens"),
                ("dock", "Toggle dock"),
                ("p1", "Zoom preset 1"),
                ("p2", "Zoom preset 2"),
                ("p3", "Zoom preset 3"),
                ("instant", "Instant tracking"),
                ("smooth", "Smooth tracking"),
                ("pin", "Pin / unpin target"),
                ("click", "Toggle click-through"),
                ("startup", "Toggle start at sign-in"),
                ("settings", "Open settings"),
                ("quit", "Exit Zoomin'"),
            ];
            let items: Vec<MenuItem<_>> = entries
                .iter()
                .map(|(id, label)| MenuItem::with_id(app, *id, *label, true, None::<&str>))
                .collect::<Result<_, _>>()?;
            let refs: Vec<&dyn tauri::menu::IsMenuItem<_>> = items
                .iter()
                .map(|i| i as &dyn tauri::menu::IsMenuItem<_>)
                .collect();
            let menu = Menu::with_items(app, &refs)?;
            TrayIconBuilder::new()
                .icon(
                    app.default_window_icon()
                        .ok_or("Application icon missing")?
                        .clone(),
                )
                .tooltip("Zoomin' — Ctrl + Alt + Z")
                .menu(&menu)
                .on_menu_event(|app, event| {
                    let state = app.state::<AppState>();
                    let action = match event.id.as_ref() {
                        "toggle" => Some(Action::Toggle),
                        "dock" => Some(Action::Dock),
                        "p1" => Some(Action::Preset(0)),
                        "p2" => Some(Action::Preset(1)),
                        "p3" => Some(Action::Preset(2)),
                        "pin" => Some(Action::Pin),
                        "settings" => {
                            show_settings(app);
                            None
                        }
                        "quit" => {
                            app.exit(0);
                            None
                        }
                        "instant" | "smooth" | "click" | "startup" => {
                            let mut s = state.snapshot().settings;
                            match event.id.as_ref() {
                                "click" => s.click_through = !s.click_through,
                                "startup" => s.startup = !s.startup,
                                v => s.tracking = v.into(),
                            };
                            if let Err(e) = apply(app, s) {
                                let _ = app.emit("app-error", e);
                            }
                            None
                        }
                        _ => None,
                    };
                    if let (Some(a), Ok(e)) = (action, state.engine()) {
                        if let Err(error) = e.action(a) {
                            let _ = app.emit("app-error", error);
                        }
                    }
                })
                .build(app)?;
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let mut pending_at = None;
                while !done.load(Ordering::Relaxed) {
                    let state = handle.state::<AppState>();
                    let snapshot = state.snapshot();
                    let _ = handle.emit("engine-state", &snapshot);
                    if state.store.lock().unwrap().differs(&snapshot.settings) {
                        if pending_at.is_none() {
                            pending_at = Some(Instant::now());
                        }
                        if pending_at
                            .is_some_and(|t: Instant| t.elapsed() > Duration::from_millis(400))
                        {
                            if let Ok(_guard) = state.transaction.lock() {
                                let current = state.snapshot().settings;
                                if let Err(e) =
                                    state.store.lock().unwrap().flush_if_changed(&current)
                                {
                                    let _ = handle.emit(
                                        "app-error",
                                        format!("Settings could not be saved: {e}"),
                                    );
                                }
                            }
                            pending_at = None;
                        }
                    } else {
                        pending_at = None;
                    }
                    std::thread::sleep(Duration::from_millis(250));
                }
            });
            if std::env::args().any(|a| a == "--background") {
                if let Some(w) = app.get_webview_window("main") {
                    w.hide()?;
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .build(tauri::generate_context!())
        .expect("Unable to start Zoomin'");
    app.run(move |handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            exit_flag.store(true, Ordering::Relaxed);
            let state = handle.state::<AppState>();
            let _guard = state.transaction.lock().unwrap();
            if let Some(engine) = &state.engine {
                engine.stop();
                // Drain queued native actions before flushing the final settings.
                let snapshot = engine.snapshot();
                if let Err(error) = state
                    .store
                    .lock()
                    .unwrap()
                    .flush_if_changed(&snapshot.settings)
                {
                    eprintln!("Could not save preferences on exit: {error}");
                }
            }
        }
    });
}
