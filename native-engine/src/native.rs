//! Every HWND, hook and Magnification call belongs to this one native thread.
//! Unsafe code is limited to Win32 boundaries; no screen frames cross IPC.
use crate::{
    geometry::{self, Point, Rect},
    input::Shortcut,
    inspector::Color,
    settings::Settings,
};
use serde::Serialize;
use std::{
    sync::atomic::{AtomicI32, AtomicU32, Ordering},
    sync::{mpsc, Arc, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows::{
    core::w,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::GetModuleHandleW, RemoteDesktop::*, Threading::*},
        UI::{
            HiDpi::*,
            Input::KeyboardAndMouse::*,
            Magnification::*,
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::*,
        },
    },
};

static WHEEL: AtomicI32 = AtomicI32::new(0);
static RESIZE_WHEEL: AtomicI32 = AtomicI32::new(0);
static RECOVERY: AtomicU32 = AtomicU32::new(0);
#[derive(Clone, Copy)]
struct Drag {
    cursor: POINT,
    rect: RECT,
    hit: u32,
}
thread_local! {static DRAG:std::cell::Cell<Option<Drag>>=const{std::cell::Cell::new(None)};}
unsafe extern "system" fn magnifier_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    parent: usize,
) -> LRESULT {
    // Route hit testing through the host so a click in the magnifier can drag it.
    if msg == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if msg == WM_LBUTTONDOWN {
        let host = HWND(parent as *mut _);
        if GetWindowLongPtrW(host, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT.0 == 0 {
            let mut p = POINT {
                x: lp.0 as u16 as i16 as i32,
                y: (lp.0 >> 16) as u16 as i16 as i32,
            };
            if ClientToScreen(hwnd, &mut p).as_bool() {
                let packed = (p.x as u16 as u32) | ((p.y as u16 as u32) << 16);
                SendMessageW(
                    host,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(packed as isize)),
                );
                return LRESULT(0);
            }
        }
    }
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(magnifier_proc), 1);
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub initialized: bool,
    pub visible: bool,
    pub holding: bool,
    pub pinned: bool,
    pub displayed_zoom: f64,
    pub cursor: Point,
    pub target: Point,
    pub source: Rect,
    pub lens: Rect,
    pub monitor: Rect,
    pub dpi: u32,
    pub update_hz: f64,
    pub update_ms: f64,
    pub sample: Option<Color>,
    pub error: Option<String>,
    pub settings: Settings,
    pub backend: String,
    pub inspector_active: bool,
    pub measurement: Option<crate::measurement::MeasurementResult>,
    pub measuring: Option<String>,
}
impl Default for Snapshot {
    fn default() -> Self {
        Self {
            initialized: false,
            visible: false,
            holding: false,
            pinned: false,
            displayed_zoom: 4.,
            cursor: Point::default(),
            target: Point::default(),
            source: Rect::default(),
            lens: Rect::default(),
            monitor: Rect::default(),
            dpi: 96,
            update_hz: 0.,
            update_ms: 0.,
            sample: None,
            error: None,
            settings: Settings::default(),
            backend: "Windows Magnification API".into(),
            inspector_active: false,
            measurement: None,
            measuring: None,
        }
    }
}
#[derive(Clone, Debug)]
pub enum Action {
    Toggle,
    Show(bool),
    Hold(bool),
    Preset(usize),
    Zoom(f64),
    Resize(f64),
    Pin,
    PinAt(Point),
    Position(Point),
    Dock,
    Grid,
    Measure(String),
    CancelMeasure,
    ResetMeasure,
    Shutdown,
}
enum Message {
    Action(Action),
    Apply(Box<Settings>, mpsc::SyncSender<Result<(), String>>),
}
pub struct Engine {
    tx: mpsc::Sender<Message>,
    state: Arc<Mutex<Snapshot>>,
    join: Mutex<Option<JoinHandle<()>>>,
}
impl Engine {
    pub fn start(settings: Settings, exclusions: Vec<isize>) -> Result<Self, String> {
        settings.validate()?;
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(Snapshot {
            settings: settings.clone(),
            ..Snapshot::default()
        }));
        let output = state.clone();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let join = thread::Builder::new()
            .name("zoomin-native".into())
            .spawn(move || {
                // SAFETY: native objects never leave this owner thread; Session releases them.
                unsafe {
                    match Session::new(settings, exclusions, output.clone()) {
                        Ok(mut session) => {
                            let _ = ready_tx.send(Ok(()));
                            session.run(rx);
                        }
                        Err(e) => {
                            output.lock().unwrap().error = Some(e.clone());
                            let _ = ready_tx.send(Err(e));
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        match ready_rx
            .recv_timeout(Duration::from_secs(10))
            .map_err(|e| e.to_string())?
        {
            Ok(()) => Ok(Self {
                tx,
                state,
                join: Mutex::new(Some(join)),
            }),
            Err(e) => {
                let _ = join.join();
                Err(e)
            }
        }
    }
    pub fn action(&self, a: Action) -> Result<(), String> {
        self.tx
            .send(Message::Action(a))
            .map_err(|_| "Native engine stopped".into())
    }
    pub fn apply(&self, s: Settings) -> Result<(), String> {
        s.validate()?;
        let (tx, rx) = mpsc::sync_channel(1);
        self.tx
            .send(Message::Apply(Box::new(s), tx))
            .map_err(|_| "Native engine stopped")?;
        rx.recv_timeout(Duration::from_secs(3))
            .map_err(|e| e.to_string())?
    }
    pub fn snapshot(&self) -> Snapshot {
        self.state.lock().unwrap().clone()
    }
    pub fn stop(&self) {
        let _ = self.action(Action::Shutdown);
        if let Some(j) = self.join.lock().unwrap().take() {
            let _ = j.join();
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Session {
    host: HWND,
    mag: HWND,
    overlay: HWND,
    hook: HHOOK,
    timer: HANDLE,
    initialized: bool,
    brush: HBRUSH,
    settings: Settings,
    state: Arc<Mutex<Snapshot>>,
    keys: Vec<(i32, Shortcut, String)>,
    next_id: i32,
    desired: bool,
    hold: bool,
    hold_suppressed: bool,
    pin: Option<Point>,
    target: Point,
    displayed: f64,
    applied_transform: f64,
    previous: Instant,
    escape_down: bool,
    layout_dirty: bool,
    last_visible: bool,
    frames: u32,
    rate_at: Instant,
    last_sample: Instant,
    capture: Option<crate::capture::Capture>,
    retry_at: Instant,
    render: Vec<u8>,
    overlay_bounds: Option<Rect>,
    overlay_opaque: Option<bool>,
    capture_error: Option<String>,
    measurement: Option<crate::measurement::Measurement>,
    exclusions: Vec<HWND>,
}
unsafe extern "system" fn host_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // System broadcasts may be sent directly, bypassing PeekMessage's queue.
    if msg == WM_POWERBROADCAST || msg == WM_WTSSESSION_CHANGE {
        RECOVERY.fetch_or(1, Ordering::Relaxed);
    }
    if msg == WM_DISPLAYCHANGE || msg == WM_DPICHANGED {
        RECOVERY.fetch_or(2, Ordering::Relaxed);
    }
    if msg == WM_NCLBUTTONDOWN
        && (wparam.0 as u32 == HTCAPTION || (HTLEFT..=HTBOTTOMRIGHT).contains(&(wparam.0 as u32)))
    {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_ok() {
            let cursor = POINT {
                x: lparam.0 as u16 as i16 as i32,
                y: (lparam.0 >> 16) as u16 as i16 as i32,
            };
            DRAG.with(|d| {
                d.set(Some(Drag {
                    cursor,
                    rect: r,
                    hit: wparam.0 as u32,
                }))
            });
            SetCapture(hwnd);
        }
        return LRESULT(0);
    }
    if msg == WM_MOUSEMOVE && GetCapture() == hwnd {
        if let Some(drag) = DRAG.with(|d| d.get()) {
            let mut p = POINT {
                x: lparam.0 as u16 as i16 as i32,
                y: (lparam.0 >> 16) as u16 as i16 as i32,
            };
            if ClientToScreen(hwnd, &mut p).as_bool() {
                let dx = p.x - drag.cursor.x;
                let dy = p.y - drag.cursor.y;
                let mut r = drag.rect;
                match drag.hit {
                    HTCAPTION => {
                        r.left += dx;
                        r.right += dx;
                        r.top += dy;
                        r.bottom += dy;
                    }
                    HTLEFT => r.left += dx,
                    HTRIGHT => r.right += dx,
                    HTTOP => r.top += dy,
                    HTBOTTOM => r.bottom += dy,
                    HTTOPLEFT => {
                        r.left += dx;
                        r.top += dy;
                    }
                    HTTOPRIGHT => {
                        r.right += dx;
                        r.top += dy;
                    }
                    HTBOTTOMLEFT => {
                        r.left += dx;
                        r.bottom += dy;
                    }
                    HTBOTTOMRIGHT => {
                        r.right += dx;
                        r.bottom += dy;
                    }
                    _ => {}
                }
                let dpi = GetDpiForWindow(hwnd).max(96);
                let minw = geometry::logical_to_physical(160., dpi);
                let minh = geometry::logical_to_physical(120., dpi);
                if r.right - r.left < minw {
                    if [HTLEFT, HTTOPLEFT, HTBOTTOMLEFT].contains(&drag.hit) {
                        r.left = r.right - minw;
                    } else {
                        r.right = r.left + minw;
                    }
                }
                if r.bottom - r.top < minh {
                    if [HTTOP, HTTOPLEFT, HTTOPRIGHT].contains(&drag.hit) {
                        r.top = r.bottom - minh;
                    } else {
                        r.bottom = r.top + minh;
                    }
                }
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    (r.right - r.left).min(geometry::logical_to_physical(1000., dpi)),
                    (r.bottom - r.top).min(geometry::logical_to_physical(800., dpi)),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        return LRESULT(0);
    }
    if [WM_LBUTTONUP, WM_CAPTURECHANGED, WM_CANCELMODE].contains(&msg) {
        DRAG.with(|d| d.set(None));
        if GetCapture() == hwnd {
            let _ = ReleaseCapture();
        }
    }
    if msg == WM_NCHITTEST {
        let mut r = RECT::default();
        let _ = GetWindowRect(hwnd, &mut r);
        let x = lparam.0 as u16 as i16 as i32;
        let y = (lparam.0 >> 16) as u16 as i16 as i32;
        let b = 8;
        let hit = match (
            x < r.left + b,
            x >= r.right - b,
            y < r.top + b,
            y >= r.bottom - b,
        ) {
            (true, _, true, _) => HTTOPLEFT,
            (_, true, true, _) => HTTOPRIGHT,
            (true, _, _, true) => HTBOTTOMLEFT,
            (_, true, _, true) => HTBOTTOMRIGHT,
            (true, _, _, _) => HTLEFT,
            (_, true, _, _) => HTRIGHT,
            (_, _, true, _) => HTTOP,
            (_, _, _, true) => HTBOTTOM,
            _ => HTCAPTION,
        };
        return LRESULT(hit as isize);
    }
    if msg == WM_MOUSEACTIVATE {
        return LRESULT(MA_NOACTIVATE as isize);
    }
    if msg == WM_ERASEBKGND {
        let mut r = RECT::default();
        let _ = GetClientRect(hwnd, &mut r);
        let brush = HBRUSH(GetClassLongPtrW(hwnd, GCLP_HBRBACKGROUND) as *mut _);
        FillRect(HDC(wparam.0 as *mut _), &r, brush);
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}
unsafe extern "system" fn wheel_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && wparam.0 as u32 == WM_MOUSEWHEEL {
        if let Some(action) = crate::input::wheel_action(
            key_down(VK_CONTROL.0 as u32),
            key_down(VK_MENU.0 as u32),
            key_down(VK_SHIFT.0 as u32),
        ) {
            let data = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            let delta = (data.mouseData >> 16) as u16 as i16 as i32;
            match action {
                crate::input::WheelAction::Zoom => WHEEL.fetch_add(delta, Ordering::Relaxed),
                crate::input::WheelAction::Resize => {
                    RESIZE_WHEEL.fetch_add(delta, Ordering::Relaxed)
                }
            };
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
unsafe fn key_down(vk: u32) -> bool {
    GetAsyncKeyState(vk as i32) < 0
}
unsafe fn chord_down(s: Shortcut) -> bool {
    key_down(s.key)
        && (s.modifiers & 1 == 0 || key_down(VK_MENU.0 as u32))
        && (s.modifiers & 2 == 0 || key_down(VK_CONTROL.0 as u32))
        && (s.modifiers & 4 == 0 || key_down(VK_SHIFT.0 as u32))
        && (s.modifiers & 8 == 0 || key_down(VK_LWIN.0 as u32) || key_down(VK_RWIN.0 as u32))
}
fn native_error(context: &str) -> String {
    format!("{context}: {}", windows::core::Error::from_win32())
}
fn winrect(r: RECT) -> Rect {
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}
fn rect(r: Rect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}
unsafe extern "system" fn overlay_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCHITTEST {
        return LRESULT(HTTRANSPARENT as isize);
    }
    if msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

impl Session {
    unsafe fn new(
        settings: Settings,
        exclusions: Vec<isize>,
        state: Arc<Mutex<Snapshot>>,
    ) -> Result<Self, String> {
        let mut s = Self {
            host: HWND::default(),
            mag: HWND::default(),
            overlay: HWND::default(),
            hook: HHOOK::default(),
            timer: HANDLE::default(),
            initialized: false,
            brush: HBRUSH::default(),
            displayed: settings.zoom,
            applied_transform: 0.,
            settings,
            state,
            keys: vec![],
            next_id: 100,
            desired: false,
            hold: false,
            hold_suppressed: false,
            pin: None,
            target: Point::default(),
            previous: Instant::now(),
            escape_down: false,
            layout_dirty: true,
            last_visible: false,
            frames: 0,
            rate_at: Instant::now(),
            last_sample: Instant::now(),
            capture: None,
            retry_at: Instant::now(),
            render: vec![],
            overlay_bounds: None,
            overlay_opaque: None,
            capture_error: None,
            measurement: None,
            exclusions: vec![],
        };
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        if !MagInitialize().as_bool() {
            return Err(native_error("MagInitialize"));
        }
        s.initialized = true;
        let hinstance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: hinstance.into(),
            lpszClassName: w!("ZoominNativeLens"),
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|e| e.to_string())?,
            ..Default::default()
        };
        // RegisterClass can already exist when an engine is deliberately recreated.
        if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(native_error("RegisterClassW"));
        }
        s.host = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
            w!("ZoominNativeLens"),
            w!("Zoomin' lens"),
            WS_POPUP | WS_THICKFRAME | WS_CLIPCHILDREN,
            0,
            0,
            360,
            240,
            None,
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("Create host: {e}"))?;
        SetLayeredWindowAttributes(s.host, COLORREF(0), 255, LWA_ALPHA)
            .map_err(|e| e.to_string())?;
        s.mag = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("Magnifier"),
            w!("Live desktop"),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(MS_SHOWMAGNIFIEDCURSOR as u32),
            0,
            0,
            360,
            240,
            Some(s.host),
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| format!("Create magnifier: {e}"))?;
        if !SetWindowSubclass(s.mag, Some(magnifier_proc), 1, s.host.0 as usize).as_bool() {
            return Err(native_error("Subclass magnifier interaction"));
        }
        let overlay_class = WNDCLASSW {
            lpfnWndProc: Some(overlay_proc),
            hInstance: hinstance.into(),
            lpszClassName: w!("ZoominInspectorOverlay"),
            ..Default::default()
        };
        RegisterClassW(&overlay_class);
        s.overlay = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
            w!("ZoominInspectorOverlay"),
            w!("Zoomin' inspection"),
            WS_POPUP,
            0,
            0,
            360,
            240,
            Some(s.host),
            None,
            Some(hinstance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;
        s.timer = CreateWaitableTimerExW(
            None,
            None,
            CREATE_WAITABLE_TIMER_HIGH_RESOLUTION,
            TIMER_ALL_ACCESS.0,
        )
        .map_err(|e| e.to_string())?;
        let mut excluded: Vec<HWND> = exclusions.into_iter().map(|h| HWND(h as *mut _)).collect();
        excluded.push(s.host);
        excluded.push(s.overlay);
        if !MagSetWindowFilterList(
            s.mag,
            MW_FILTERMODE_EXCLUDE,
            excluded.len() as i32,
            excluded.as_mut_ptr(),
        )
        .as_bool()
        {
            return Err(native_error("MagSetWindowFilterList"));
        }
        s.exclusions = excluded;
        s.configure_visuals()?;
        // A conflicting shortcut must not disable the lens or its tray recovery.
        // Registration is atomic, so none of the staged bindings remain on failure.
        if let Err(error) = s.register(&s.settings.clone()) {
            s.state.lock().unwrap().error=Some(format!("Global shortcuts were not registered: {error} The lens is available from settings and the tray; change the conflicting binding in Controls."));
        }
        s.hook = SetWindowsHookExW(WH_MOUSE_LL, Some(wheel_proc), Some(hinstance.into()), 0)
            .map_err(|e| format!("Wheel hook: {e}"))?;
        WTSRegisterSessionNotification(s.host, NOTIFY_FOR_THIS_SESSION)
            .map_err(|e| e.to_string())?;
        let mut p = POINT::default();
        GetCursorPos(&mut p).map_err(|e| e.to_string())?;
        s.target = Point {
            x: p.x as f64,
            y: p.y as f64,
        };
        if s.settings.tracking == "fixed" {
            s.pin = Some(s.target)
        }
        s.state.lock().unwrap().initialized = true;
        Ok(s)
    }
    unsafe fn register(&mut self, settings: &Settings) -> Result<(), String> {
        let mut staged = vec![];
        for b in &settings.shortcuts {
            let chord = Shortcut::parse(&b.chord)?;
            if let Some(old) = self.keys.iter().find(|(_, s, _)| *s == chord) {
                staged.push((old.0, chord, b.action.clone()));
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            if let Err(e) = RegisterHotKey(
                Some(self.host),
                id,
                HOT_KEY_MODIFIERS(chord.modifiers) | MOD_NOREPEAT,
                chord.key,
            ) {
                for (i, _, _) in &staged {
                    if !self.keys.iter().any(|k| k.0 == *i) {
                        let _ = UnregisterHotKey(Some(self.host), *i);
                    }
                }
                let outcome = if self.keys.is_empty() {
                    "No shortcuts were registered."
                } else {
                    "Previous shortcuts retained."
                };
                return Err(format!("{} is unavailable ({e}). {outcome}", b.chord));
            }
            staged.push((id, chord, b.action.clone()));
        }
        for (id, _, _) in &self.keys {
            if !staged.iter().any(|s| s.0 == *id) {
                UnregisterHotKey(Some(self.host), *id).map_err(|e| e.to_string())?;
            }
        }
        self.keys = staged;
        Ok(())
    }
    unsafe fn configure_visuals(&mut self) -> Result<(), String> {
        let mut style = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        if self.settings.click_through {
            style |= WS_EX_TRANSPARENT;
        }
        SetWindowLongPtrW(self.host, GWL_EXSTYLE, style.0 as isize);
        let c = u32::from_str_radix(&self.settings.accent[1..], 16).unwrap();
        let color = COLORREF(((c >> 16) & 255) | (c & 0x00ff00) | ((c & 255) << 16));
        let brush = CreateSolidBrush(color);
        if brush.is_invalid() {
            return Err(native_error("CreateSolidBrush"));
        }
        SetClassLongPtrW(self.host, GCLP_HBRBACKGROUND, brush.0 as isize);
        if !self.brush.is_invalid() {
            let _ = DeleteObject(self.brush.into());
        }
        self.brush = brush;
        let affinity = if self.settings.stream_profile == "local" {
            WDA_EXCLUDEFROMCAPTURE
        } else {
            WDA_NONE
        };
        SetWindowDisplayAffinity(self.host, affinity)
            .map_err(|e| format!("Capture visibility unsupported: {e}"))?;
        SetWindowDisplayAffinity(self.overlay, affinity)
            .map_err(|e| format!("Overlay capture visibility unsupported: {e}"))?;
        self.layout_dirty = true;
        Ok(())
    }
    unsafe fn handle(&mut self, a: Action) -> bool {
        match a {
            Action::Shutdown => return false,
            Action::Toggle => {
                self.measurement = None;
                self.desired = !(self.desired || self.hold);
                self.hold = false;
                self.hold_suppressed = true;
            }
            Action::Show(v) => {
                if !v {
                    DRAG.with(|d| d.set(None));
                    if GetCapture() == self.host {
                        let _ = ReleaseCapture();
                    }
                }
                if v {
                    self.measurement = None;
                }
                self.desired = v;
                if !v {
                    self.hold = false;
                    self.hold_suppressed = true;
                }
            }
            Action::Hold(v) => {
                if v {
                    if !self.hold_suppressed {
                        self.hold = true;
                    }
                } else {
                    self.hold = false;
                    self.hold_suppressed = false;
                }
            }
            Action::Preset(i) => {
                if let Some(z) = self.settings.presets.get(i) {
                    self.settings.zoom = *z;
                }
            }
            Action::Zoom(z) => self.settings.zoom = geometry::zoom(z),
            Action::Resize(steps) => {
                self.settings.resize_lens(steps);
                self.layout_dirty = true;
            }
            Action::Pin => {
                if self.pin.take().is_some() {
                    self.settings.tracking = "instant".into();
                } else {
                    self.pin = Some(self.target);
                    self.settings.tracking = "fixed".into();
                }
            }
            Action::PinAt(p) => {
                self.pin = Some(p);
                self.settings.tracking = "fixed".into();
            }
            Action::Position(p) => {
                self.settings.placement = "pinned".into();
                let _ = SetWindowPos(
                    self.host,
                    Some(HWND_TOPMOST),
                    p.x as i32,
                    p.y as i32,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            Action::Dock => {
                self.settings.placement = if self.settings.placement == "docked" {
                    "follow"
                } else {
                    "docked"
                }
                .into();
                self.layout_dirty = true;
            }
            Action::Grid => {
                self.settings.grid = !self.settings.grid;
                if self.settings.grid {
                    self.settings.pixel_perfect = true;
                }
            }
            Action::Measure(kind) => {
                self.desired = false;
                self.hold = false;
                self.hold_suppressed = true;
                match crate::measurement::Measurement::start(kind) {
                    Ok(m) => self.measurement = Some(m),
                    Err(e) => self.state.lock().unwrap().error = Some(e),
                }
            }
            Action::CancelMeasure => self.measurement = None,
            Action::ResetMeasure => {
                self.measurement = None;
                self.state.lock().unwrap().measurement = None;
            }
        }
        if !self.desired && !self.hold {
            DRAG.with(|d| d.set(None));
            if GetCapture() == self.host {
                let _ = ReleaseCapture();
            }
        }
        true
    }
    unsafe fn run(&mut self, rx: mpsc::Receiver<Message>) {
        let mut running = true;
        while running {
            loop {
                let m = match rx.try_recv() {
                    Ok(message) => message,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        running = false;
                        break;
                    }
                };
                match m {
                    Message::Action(a) => running &= self.handle(a),
                    Message::Apply(s, reply) => {
                        let previous = self.settings.clone();
                        self.settings = *s;
                        // Validate the potentially failing window changes before
                        // releasing any old shortcut registrations.
                        let result = self
                            .configure_visuals()
                            .and_then(|()| self.register(&self.settings.clone()));
                        if result.is_err() {
                            self.settings = previous.clone();
                            let _ = self.configure_visuals();
                        }
                        if self.settings.tracking == "fixed" && self.pin.is_none() {
                            self.pin = Some(self.target);
                        } else if self.settings.tracking != "fixed" {
                            self.pin = None;
                        }
                        if result.is_ok() {
                            self.capture_error = None;
                            self.state.lock().unwrap().error = None;
                        }
                        self.state.lock().unwrap().settings = self.settings.clone();
                        let _ = reply.send(result);
                    }
                }
            }
            let mut msg = MSG::default();
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                if msg.message == WM_HOTKEY {
                    if let Some((_, _, action)) =
                        self.keys.iter().find(|k| k.0 == msg.wParam.0 as i32)
                    {
                        let a = match action.as_str() {
                            "toggle" => Action::Toggle,
                            "hold" => Action::Hold(true),
                            "preset1" => Action::Preset(0),
                            "preset2" => Action::Preset(1),
                            "preset3" => Action::Preset(2),
                            "dock" => Action::Dock,
                            "pin" => Action::Pin,
                            "grid" => Action::Grid,
                            _ => continue,
                        };
                        self.handle(a);
                    }
                }
                if msg.message == WM_POWERBROADCAST || msg.message == WM_WTSSESSION_CHANGE {
                    self.hold = false;
                    self.desired = false;
                    self.hold_suppressed = true;
                    self.layout_dirty = true;
                    self.measurement = None;
                }
                if msg.message == WM_DISPLAYCHANGE || msg.message == WM_DPICHANGED {
                    self.layout_dirty = true;
                    self.capture = None;
                    self.measurement = None;
                }
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            let recovery = RECOVERY.swap(0, Ordering::Relaxed);
            if recovery != 0 {
                self.layout_dirty = true;
                self.capture = None;
                self.measurement = None;
            }
            if recovery & 1 != 0 {
                self.handle(Action::Show(false));
                self.hold_suppressed = true;
            }
            if !running {
                break;
            }
            let start = Instant::now();
            let dt = start.duration_since(self.previous).as_secs_f64().min(0.1);
            self.previous = start;
            let hold_down = self
                .keys
                .iter()
                .find(|k| k.2 == "hold")
                .is_some_and(|k| chord_down(k.1));
            if !hold_down {
                self.hold = false;
                self.hold_suppressed = false;
            }
            let escape = key_down(VK_ESCAPE.0 as u32);
            if escape
                && !self.escape_down
                && (self.desired || self.hold || self.measurement.is_some())
            {
                self.handle(Action::Show(false));
                self.measurement = None;
            }
            self.escape_down = escape;
            if let Some(measure) = self.measurement.as_mut() {
                let (done, result) = measure.poll();
                if done {
                    self.measurement = None;
                    if let Some(result) = result {
                        self.state.lock().unwrap().measurement = Some(result);
                    }
                }
            }
            self.state.lock().unwrap().measuring = self.measurement.as_ref().map(|m| m.kind());
            let wheel = WHEEL.swap(0, Ordering::Relaxed);
            if wheel != 0 {
                self.settings.zoom = geometry::zoom(
                    self.settings.zoom + wheel as f64 / 120. * self.settings.sensitivity,
                );
            }
            let visible = self.desired || self.hold;
            let resize = RESIZE_WHEEL.swap(0, Ordering::Relaxed);
            if resize != 0 {
                self.handle(Action::Resize(resize as f64 / 120.));
            }
            if visible {
                if let Err(e) = self.tick(dt) {
                    self.desired = false;
                    self.hold = false;
                    self.hold_suppressed = true;
                    self.state.lock().unwrap().error = Some(e);
                }
            }
            let visible = self.desired || self.hold;
            if visible != self.last_visible {
                let _ = ShowWindow(self.host, if visible { SW_SHOWNOACTIVATE } else { SW_HIDE });
                if !visible {
                    let _ = ShowWindow(self.overlay, SW_HIDE);
                    self.capture = None;
                    self.state.lock().unwrap().inspector_active = false;
                }
                self.last_visible = visible;
            }
            {
                let mut state = self.state.lock().unwrap();
                state.visible = visible;
                state.holding = self.hold;
                state.pinned = self.pin.is_some();
                state.settings = self.settings.clone();
                state.displayed_zoom = self.displayed;
                if visible {
                    state.update_ms = start.elapsed().as_secs_f64() * 1000.;
                }
            }
            if visible {
                self.frames += 1;
            }
            if self.rate_at.elapsed() >= Duration::from_secs(1) {
                self.state.lock().unwrap().update_hz =
                    self.frames as f64 / self.rate_at.elapsed().as_secs_f64();
                self.frames = 0;
                self.rate_at = Instant::now();
            }
            // Message pumping continues while hidden for hotkeys, release recovery and hooks.
            let budget = Duration::from_micros(if visible { 16667 } else { 25000 });
            let remaining = budget.saturating_sub(start.elapsed());
            if !remaining.is_zero() {
                let due = -((remaining.as_nanos() / 100).max(1) as i64);
                if SetWaitableTimer(self.timer, &due, 0, None, None, false).is_ok() {
                    WaitForSingleObject(self.timer, 50);
                } else {
                    thread::sleep(remaining);
                }
            }
        }
        let mut state = self.state.lock().unwrap();
        state.initialized = false;
        state.settings = self.settings.clone();
        state.visible = false;
        state.holding = false;
        state.inspector_active = false;
        state.measuring = None;
        state.update_hz = 0.;
    }
    fn report_capture_error(&mut self, error: String) {
        let mut state = self.state.lock().unwrap();
        // Recovery of an optional backend must preserve unrelated warnings.
        if state.error.is_none() || state.error == self.capture_error {
            state.error = Some(error.clone());
        }
        self.capture_error = Some(error.clone());
    }
    fn clear_capture_error(&mut self) {
        if let Some(error) = self.capture_error.take() {
            let mut state = self.state.lock().unwrap();
            if state.error.as_ref() == Some(&error) {
                state.error = None;
            }
        }
    }
    unsafe fn paint_overlay(
        &mut self,
        source: Rect,
        bounds: Rect,
        cw: i32,
        ch: i32,
        b: i32,
    ) -> Result<bool, String> {
        if !self.settings.pixel_perfect && !self.settings.crosshair {
            if IsWindowVisible(self.overlay).as_bool() {
                let _ = ShowWindow(self.overlay, SW_HIDE);
            }
            self.capture = None;
            let mut s = self.state.lock().unwrap();
            s.inspector_active = false;
            s.backend = "Windows Magnification API".into();
            return Ok(false);
        }
        let mut origin = POINT { x: b, y: b };
        if !ClientToScreen(self.host, &mut origin).as_bool() {
            return Err(native_error("ClientToScreen"));
        }
        let overlay_bounds = Rect {
            left: origin.x,
            top: origin.y,
            right: origin.x + cw,
            bottom: origin.y + ch,
        };
        if self.overlay_bounds != Some(overlay_bounds) {
            SetWindowPos(
                self.overlay,
                Some(HWND_TOPMOST),
                origin.x,
                origin.y,
                cw,
                ch,
                SWP_NOACTIVATE,
            )
            .map_err(|e| e.to_string())?;
            self.overlay_bounds = Some(overlay_bounds);
        }
        let inspection = self.settings.pixel_perfect;
        let mut active = false;
        if inspection {
            let lens = Rect {
                left: origin.x,
                top: origin.y,
                right: origin.x + cw,
                bottom: origin.y + ch,
            };
            let overlaps = source.left < lens.right
                && source.right > lens.left
                && source.top < lens.bottom
                && source.bottom > lens.top
                || self.exclusions.iter().any(|h| {
                    let mut r = RECT::default();
                    IsWindowVisible(*h).as_bool()
                        && GetWindowRect(*h, &mut r).is_ok()
                        && source.left < r.right
                        && source.right > r.left
                        && source.top < r.bottom
                        && source.bottom > r.top
                });
            if overlaps {
                self.report_capture_error("Move or pin the lens away from the source to inspect original pixels. Live magnification is active.".into());
            } else {
                if self.capture.as_ref().is_some_and(|c| c.bounds != bounds) {
                    self.capture = None;
                    self.retry_at = Instant::now();
                }
                if self.capture.is_none() && Instant::now() >= self.retry_at {
                    match crate::capture::Capture::new(bounds) {
                        Ok(c) => self.capture = Some(c),
                        Err(e) => {
                            self.report_capture_error(e);
                            self.retry_at = Instant::now() + Duration::from_secs(2);
                        }
                    }
                }
                if let Some(c) = self.capture.as_mut() {
                    match c.update(source) {
                        Ok(_) => active = !c.pixels.is_empty() && c.source == source,
                        Err(e) => {
                            self.report_capture_error(e);
                            self.capture = None;
                            self.retry_at = Instant::now() + Duration::from_secs(2);
                        }
                    }
                }
            }
        } else {
            self.capture = None;
        }
        if active {
            self.clear_capture_error();
        }
        {
            let mut s = self.state.lock().unwrap();
            s.inspector_active = active;
            s.backend = if active {
                "DXGI / D3D11 region readback + native nearest-neighbor"
            } else {
                "Windows Magnification API"
            }
            .into();
        }
        if !active && !self.settings.crosshair {
            if IsWindowVisible(self.overlay).as_bool() {
                let _ = ShowWindow(self.overlay, SW_HIDE);
            }
            return Ok(false);
        }
        if self.overlay_opaque != Some(active) {
            SetLayeredWindowAttributes(
                self.overlay,
                COLORREF(0),
                255,
                if active { LWA_ALPHA } else { LWA_COLORKEY },
            )
            .map_err(|e| e.to_string())?;
            self.overlay_opaque = Some(active);
        }
        self.render.resize(cw as usize * ch as usize * 4, 0);
        if !active {
            self.render.fill(0);
        }
        if active {
            let c = self.capture.as_ref().unwrap();
            let sw = source.width() as usize;
            let sh = source.height() as usize;
            for y in 0..ch as usize {
                let sy = geometry::pixel_index(y, self.displayed, sh);
                for x in 0..cw as usize {
                    let sx = geometry::pixel_index(x, self.displayed, sw);
                    let src = (sy * sw + sx) * 4;
                    let dest = (y * cw as usize + x) * 4;
                    self.render[dest..dest + 4].copy_from_slice(&c.pixels[src..src + 4]);
                }
            }
            if self.settings.grid && self.displayed >= self.settings.grid_threshold {
                for i in 1..sw {
                    let x = (i as f64 * self.displayed).round() as i32;
                    if x < cw {
                        for y in 0..ch {
                            let n = ((y * cw + x) * 4) as usize;
                            self.render[n..n + 3].copy_from_slice(&[90, 75, 60]);
                        }
                    }
                }
                for i in 1..sh {
                    let y = (i as f64 * self.displayed).round() as i32;
                    if y < ch {
                        for x in 0..cw {
                            let n = ((y * cw + x) * 4) as usize;
                            self.render[n..n + 3].copy_from_slice(&[90, 75, 60]);
                        }
                    }
                }
            }
            let sx =
                (self.target.x.round() as i32 - source.left).clamp(0, source.width() - 1) as usize;
            let sy =
                (self.target.y.round() as i32 - source.top).clamp(0, source.height() - 1) as usize;
            let i = (sy * sw + sx) * 4;
            self.state.lock().unwrap().sample =
                Some(Color::new(c.pixels[i + 2], c.pixels[i + 1], c.pixels[i]));
        }
        if self.settings.crosshair {
            let cx = ((self.target.x.round() - source.left as f64 + 0.5) * self.displayed).round()
                as i32;
            let cy =
                ((self.target.y.round() - source.top as f64 + 0.5) * self.displayed).round() as i32;
            for d in -12_i32..=12 {
                if d.abs() < 3 {
                    continue;
                }
                for (x, y) in [(cx + d, cy), (cx, cy + d)] {
                    if x >= 0 && y >= 0 && x < cw && y < ch {
                        let i = ((y * cw + x) * 4) as usize;
                        self.render[i..i + 3].copy_from_slice(&[255, 220, 190]);
                    }
                }
            }
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: cw,
                biHeight: -ch,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let dc = GetDC(Some(self.overlay));
        let result = StretchDIBits(
            dc,
            0,
            0,
            cw,
            ch,
            0,
            0,
            cw,
            ch,
            Some(self.render.as_ptr() as *const _),
            &info,
            DIB_RGB_COLORS,
            SRCCOPY,
        );
        ReleaseDC(Some(self.overlay), dc);
        if result == 0 {
            return Err(native_error("Present inspection bitmap"));
        }
        if !IsWindowVisible(self.overlay).as_bool() {
            let _ = ShowWindow(self.overlay, SW_SHOWNOACTIVATE);
        }
        Ok(active)
    }
    unsafe fn tick(&mut self, dt: f64) -> Result<(), String> {
        let mut p = POINT::default();
        GetCursorPos(&mut p).map_err(|e| format!("Desktop unavailable: {e}"))?;
        let cursor = Point {
            x: p.x as f64,
            y: p.y as f64,
        };
        self.target = if let Some(pin) = self.pin {
            pin
        } else if self.settings.tracking == "smooth" {
            Point {
                x: geometry::damp(
                    self.target.x,
                    cursor.x,
                    dt,
                    self.settings.smoothing_ms / 1000.,
                ),
                y: geometry::damp(
                    self.target.y,
                    cursor.y,
                    dt,
                    self.settings.smoothing_ms / 1000.,
                ),
            }
        } else {
            cursor
        };
        let mon = MonitorFromPoint(
            POINT {
                x: self.target.x.round() as i32,
                y: self.target.y.round() as i32,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return Err(native_error("GetMonitorInfoW"));
        }
        let bounds = winrect(mi.rcMonitor);
        let mut work = winrect(mi.rcWork);
        let mut dx = 96;
        let mut dy = 96;
        GetDpiForMonitor(mon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy).map_err(|e| e.to_string())?;
        let mut lens_dpi = dx;
        if self.settings.placement == "pinned" {
            let lm = MonitorFromWindow(self.host, MONITOR_DEFAULTTONEAREST);
            let mut li = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(lm, &mut li).as_bool() {
                work = winrect(li.rcWork);
                let mut y = 96;
                GetDpiForMonitor(lm, MDT_EFFECTIVE_DPI, &mut lens_dpi, &mut y)
                    .map_err(|e| e.to_string())?;
            }
        }
        let (mut width, mut height) = (
            geometry::logical_to_physical(self.settings.width, lens_dpi),
            geometry::logical_to_physical(self.settings.height, lens_dpi),
        );
        let mut pos = None;
        if self.settings.placement == "docked" {
            let size = geometry::logical_to_physical(self.settings.dock_size, dx);
            match self.settings.dock_edge.as_str() {
                "left" => {
                    width = size;
                    height = work.height();
                    pos = Some(Point {
                        x: work.left as f64,
                        y: work.top as f64,
                    });
                }
                "right" => {
                    width = size;
                    height = work.height();
                    pos = Some(Point {
                        x: (work.right - size) as f64,
                        y: work.top as f64,
                    });
                }
                "bottom" => {
                    width = work.width();
                    height = size;
                    pos = Some(Point {
                        x: work.left as f64,
                        y: (work.bottom - size) as f64,
                    });
                }
                _ => {
                    width = work.width();
                    height = size;
                    pos = Some(Point {
                        x: work.left as f64,
                        y: work.top as f64,
                    });
                }
            }
        } else if self.settings.placement == "follow" {
            // Lens placement follows the physical pointer independently from the source target.
            let pm = MonitorFromPoint(p, MONITOR_DEFAULTTONEAREST);
            let mut pi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if GetMonitorInfoW(pm, &mut pi).as_bool() {
                let mut pd = 96;
                let mut py = 96;
                GetDpiForMonitor(pm, MDT_EFFECTIVE_DPI, &mut pd, &mut py)
                    .map_err(|e| e.to_string())?;
                work = winrect(pi.rcWork);
                width = geometry::logical_to_physical(self.settings.width, pd);
                height = geometry::logical_to_physical(self.settings.height, pd);
                pos = Some(geometry::placement(
                    cursor,
                    width,
                    height,
                    Point {
                        x: geometry::logical_to_physical(self.settings.offset.x, pd) as f64,
                        y: geometry::logical_to_physical(self.settings.offset.y, pd) as f64,
                    },
                    winrect(pi.rcWork),
                ));
            }
        }
        width = width.min(work.width()).max(1);
        height = height.min(work.height()).max(1);
        let mut wr = RECT::default();
        GetWindowRect(self.host, &mut wr).map_err(|e| e.to_string())?;
        if let Some(pos) = pos {
            if (wr.left, wr.top, wr.right - wr.left, wr.bottom - wr.top)
                != (pos.x as i32, pos.y as i32, width, height)
            {
                SetWindowPos(
                    self.host,
                    Some(HWND_TOPMOST),
                    pos.x as i32,
                    pos.y as i32,
                    width,
                    height,
                    SWP_NOACTIVATE,
                )
                .map_err(|e| e.to_string())?;
            }
        } else if self.layout_dirty {
            SetWindowPos(
                self.host,
                Some(HWND_TOPMOST),
                wr.left
                    .clamp(work.left, (work.right - width).max(work.left)),
                wr.top.clamp(work.top, (work.bottom - height).max(work.top)),
                width,
                height,
                SWP_NOACTIVATE,
            )
            .map_err(|e| e.to_string())?;
        }
        if self.settings.placement == "pinned" && !self.layout_dirty && !self.settings.click_through
        {
            self.settings.width =
                ((wr.right - wr.left) as f64 * 96. / lens_dpi as f64).clamp(160., 1000.);
            self.settings.height =
                ((wr.bottom - wr.top) as f64 * 96. / lens_dpi as f64).clamp(120., 800.);
        }
        let update_geometry = self.layout_dirty;
        self.layout_dirty = false;
        let mut client = RECT::default();
        GetClientRect(self.host, &mut client).map_err(|e| e.to_string())?;
        let b = if self.settings.stream_profile == "minimal" {
            0
        } else {
            self.settings.border as i32
        };
        let cw = (client.right - 2 * b).max(1);
        let ch = (client.bottom - 2 * b).max(1);
        let mut child = RECT::default();
        GetClientRect(self.mag, &mut child).map_err(|e| e.to_string())?;
        if (child.right, child.bottom) != (cw, ch) || update_geometry {
            SetWindowPos(self.mag, None, b, b, cw, ch, SWP_NOZORDER | SWP_NOACTIVATE)
                .map_err(|e| e.to_string())?;
        }
        self.displayed = geometry::damp(self.displayed, self.settings.zoom, dt, 0.055);
        if (self.displayed - self.settings.zoom).abs() < 0.001 {
            self.displayed = self.settings.zoom;
        }
        let source = geometry::source_rect(self.target, cw, ch, self.displayed, bounds);
        // Only the active backend renders. The native source is refreshed in the
        // same tick when inspection fails, so fallback never shows a stale view.
        let inspection = self.paint_overlay(source, bounds, cw, ch, b)?;
        if inspection {
            if IsWindowVisible(self.mag).as_bool() {
                let _ = ShowWindow(self.mag, SW_HIDE);
            }
        } else {
            let z = self.displayed as f32;
            let mut transform = MAGTRANSFORM {
                v: [z, 0., 0., 0., z, 0., 0., 0., 1.],
            };
            if self.applied_transform != self.displayed
                && !MagSetWindowTransform(self.mag, &mut transform).as_bool()
            {
                return Err(native_error("MagSetWindowTransform"));
            }
            self.applied_transform = self.displayed;
            if !MagSetWindowSource(self.mag, rect(source)).as_bool() {
                return Err(native_error("MagSetWindowSource"));
            }
            if !IsWindowVisible(self.mag).as_bool() {
                let _ = ShowWindow(self.mag, SW_SHOWNOACTIVATE);
            }
            let _ = InvalidateRect(Some(self.mag), None, true);
        }
        GetWindowRect(self.host, &mut wr).map_err(|e| e.to_string())?;
        let mut state = self.state.lock().unwrap();
        state.cursor = cursor;
        state.target = self.target;
        state.source = source;
        state.lens = winrect(wr);
        state.monitor = bounds;
        state.dpi = dx;
        if !state.inspector_active && self.last_sample.elapsed() >= Duration::from_millis(120) {
            // GDI samples the original desktop coordinate, not a scaled magnifier bitmap.
            // If the lens covers that point, omit the sample instead of sampling its overlay.
            let sample_point = Point {
                x: self.target.x.round(),
                y: self.target.y.round(),
            };
            let covered = self.exclusions.iter().any(|h| {
                let mut r = RECT::default();
                IsWindowVisible(*h).as_bool()
                    && GetWindowRect(*h, &mut r).is_ok()
                    && winrect(r).contains(sample_point)
            });
            state.sample = if covered {
                None
            } else {
                let dc = GetDC(None);
                let c = GetPixel(dc, sample_point.x as i32, sample_point.y as i32);
                ReleaseDC(None, dc);
                if c.0 == 0xffffffff {
                    None
                } else {
                    Some(Color::new(
                        (c.0 & 255) as u8,
                        ((c.0 >> 8) & 255) as u8,
                        ((c.0 >> 16) & 255) as u8,
                    ))
                }
            };
            self.last_sample = Instant::now();
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            if !self.hook.is_invalid() {
                let _ = UnhookWindowsHookEx(self.hook);
            }
            for (id, _, _) in &self.keys {
                let _ = UnregisterHotKey(Some(self.host), *id);
            }
            if !self.host.is_invalid() {
                let _ = WTSUnRegisterSessionNotification(self.host);
                let _ = DestroyWindow(self.host);
            }
            if !self.brush.is_invalid() {
                let _ = DeleteObject(self.brush.into());
            }
            if !self.timer.is_invalid() {
                let _ = CloseHandle(self.timer);
            }
            if self.initialized {
                let _ = MagUninitialize();
            }
        }
    }
}
