//! Explicit, temporary desktop selection. No global click interception outside it.
use crate::geometry::{measurement, Point};
use serde::Serialize;
use windows::{
    core::w,
    Win32::{
        Foundation::*, Graphics::Gdi::*, System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::*,
    },
};
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeasurementResult {
    pub kind: String,
    pub start: Point,
    pub end: Point,
    pub width: i32,
    pub height: i32,
    pub distance: f64,
}
struct Selection {
    kind: String,
    start: Option<Point>,
    end: Point,
    complete: bool,
    cancel: bool,
}
pub struct Measurement {
    hwnd: HWND,
    state: Box<Selection>,
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_NCCREATE {
        let create = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
    }
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Selection;
    if !ptr.is_null() {
        let s = &mut *ptr;
        if msg == WM_MOUSEACTIVATE {
            return LRESULT(MA_NOACTIVATE as isize);
        }
        if msg == WM_LBUTTONDOWN {
            // Use the event's coordinate, which remains accurate if the pointer
            // moves between the click and this window's message dispatch.
            let mut p = POINT {
                x: lp.0 as u16 as i16 as i32,
                y: (lp.0 >> 16) as u16 as i16 as i32,
            };
            if ClientToScreen(hwnd, &mut p).as_bool() {
                let point = Point {
                    x: p.x as f64,
                    y: p.y as f64,
                };
                if s.start.is_none() {
                    s.start = Some(point)
                } else {
                    s.end = point;
                    s.complete = true
                }
            }
            return LRESULT(0);
        }
        if msg == WM_RBUTTONDOWN {
            s.cancel = true;
            return LRESULT(0);
        }
        if msg == WM_PAINT {
            let mut ps = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut ps);
            let mut client = RECT::default();
            let _ = GetClientRect(hwnd, &mut client);
            let black = HBRUSH(GetStockObject(BLACK_BRUSH).0);
            FillRect(dc, &client, black);
            let white = CreatePen(PS_SOLID, 2, COLORREF(0xffffff));
            let old = SelectObject(dc, white.into());
            let old_brush = SelectObject(dc, GetStockObject(HOLLOW_BRUSH));
            if let Some(start) = s.start {
                let mut origin = POINT::default();
                let _ = ClientToScreen(hwnd, &mut origin);
                let (x1, y1) = (start.x as i32 - origin.x, start.y as i32 - origin.y);
                let (x2, y2) = (s.end.x as i32 - origin.x, s.end.y as i32 - origin.y);
                if s.kind == "region" {
                    let _ = Rectangle(dc, x1.min(x2), y1.min(y2), x1.max(x2), y1.max(y2));
                } else {
                    let _ = MoveToEx(dc, x1, y1, None);
                    let _ = LineTo(dc, x2, y2);
                }
            }
            SetTextColor(dc, COLORREF(0xffffff));
            SetBkMode(dc, TRANSPARENT);
            let mut text: Vec<u16> = format!(
                "Zoomin' {} • Click two points • Esc or right-click to cancel",
                s.kind
            )
            .encode_utf16()
            .collect();
            let mut label = RECT {
                left: 35,
                top: 35,
                right: client.right - 30,
                bottom: 80,
            };
            DrawTextW(dc, &mut text, &mut label, DT_LEFT);
            SelectObject(dc, old);
            SelectObject(dc, old_brush);
            let _ = DeleteObject(white.into());
            let _ = EndPaint(hwnd, &ps);
            return LRESULT(0);
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}
impl Measurement {
    /// # Safety
    /// Must run on the native window thread with a pumping Win32 message loop.
    pub unsafe fn start(kind: String) -> Result<Self, String> {
        if kind != "ruler" && kind != "region" {
            return Err("Unknown measurement mode".into());
        }
        let module = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: module.into(),
            lpszClassName: w!("ZoominMeasurement"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let mut state = Box::new(Selection {
            kind,
            start: None,
            end: Point::default(),
            complete: false,
            cancel: false,
        });
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("ZoominMeasurement"),
            w!("Zoomin' measurement"),
            WS_POPUP,
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
            None,
            None,
            Some(module.into()),
            Some(&mut *state as *mut _ as *const _),
        )
        .map_err(|e| e.to_string())?;
        let result = Self { hwnd, state };
        SetLayeredWindowAttributes(hwnd, COLORREF(0), 170, LWA_ALPHA).map_err(|e| e.to_string())?;
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        Ok(result)
    }
    /// # Safety
    /// Must be called on the thread that created this selection window.
    pub unsafe fn poll(&mut self) -> (bool, Option<MeasurementResult>) {
        if self.state.cancel {
            return (true, None);
        }
        if self.state.complete {
            let start = self.state.start.unwrap();
            let end = self.state.end;
            let (width, height, distance) = measurement(start, end);
            return (
                true,
                Some(MeasurementResult {
                    kind: self.state.kind.clone(),
                    start,
                    end,
                    width,
                    height,
                    distance,
                }),
            );
        }
        let mut p = POINT::default();
        if GetCursorPos(&mut p).is_ok() {
            self.state.end = Point {
                x: p.x as f64,
                y: p.y as f64,
            };
        }
        let _ = InvalidateRect(Some(self.hwnd), None, false);
        (false, None)
    }
    pub fn kind(&self) -> String {
        self.state.kind.clone()
    }
}
impl Drop for Measurement {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
