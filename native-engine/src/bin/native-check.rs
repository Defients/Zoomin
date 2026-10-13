//! Tests synthetic native desktop content only; no user screen images saved.
use std::{thread, time::Duration};
use windows::{
    core::w,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
};
use zoomin_engine::{geometry::Point, settings::Settings, Action, Engine};

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, wp, lp)
}
unsafe fn color_at(x: i32, y: i32) -> u32 {
    let dc = GetDC(None);
    let c = GetPixel(dc, x, y).0;
    ReleaseDC(None, dc);
    c
}
unsafe fn paint(hwnd: HWND, color: u32) {
    let dc = GetDC(Some(hwnd));
    let brush = CreateSolidBrush(COLORREF(color));
    let mut r = RECT::default();
    GetClientRect(hwnd, &mut r).unwrap();
    FillRect(dc, &r, brush);
    let _ = DeleteObject(brush.into());
    ReleaseDC(Some(hwnd), dc);
}
fn pack(p: POINT) -> LPARAM {
    LPARAM(((p.x as u16 as u32) | ((p.y as u16 as u32) << 16)) as isize)
}
fn settle() {
    thread::sleep(Duration::from_millis(450));
}
unsafe fn key(vk: VIRTUAL_KEY, down: bool) {
    let i = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: if down {
                    KEYBD_EVENT_FLAGS(0)
                } else {
                    KEYEVENTF_KEYUP
                },
                ..Default::default()
            },
        },
    };
    assert_eq!(SendInput(&[i], std::mem::size_of::<INPUT>() as i32), 1);
}
unsafe fn chord(vk: VIRTUAL_KEY) {
    key(VK_CONTROL, true);
    key(VK_MENU, true);
    key(vk, true);
    settle();
    key(vk, false);
    key(VK_MENU, false);
    key(VK_CONTROL, false);
    settle();
}
fn main() {
    unsafe {
        let physical_input = !std::env::args().any(|a| a == "--no-input");
        let mut initial_cursor = POINT::default();
        GetCursorPos(&mut initial_cursor).unwrap();
        if physical_input {
            SetCursorPos(1400, 900).unwrap();
        }
        let _ = windows::Win32::UI::HiDpi::SetThreadDpiAwarenessContext(
            windows::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
        let module = GetModuleHandleW(None).unwrap();
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: module.into(),
            lpszClassName: w!("ZoominTestPattern"),
            ..Default::default()
        };
        RegisterClassW(&class);
        let stimulus = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            w!("ZoominTestPattern"),
            w!("Zoomin native test pattern"),
            WS_POPUP | WS_VISIBLE,
            100,
            100,
            400,
            300,
            None,
            None,
            Some(module.into()),
            None,
        )
        .unwrap();
        let mut settings = Settings {
            placement: "pinned".into(),
            tracking: "fixed".into(),
            ..Settings::default()
        };

        let engine = Engine::start(settings.clone(), vec![]).expect("Native initialization");
        engine
            .action(Action::PinAt(Point { x: 300., y: 250. }))
            .unwrap();
        engine
            .action(Action::Position(Point { x: 650., y: 200. }))
            .unwrap();
        engine.action(Action::Show(true)).unwrap();
        let mut checks = vec![];
        for (name, color) in [
            ("live red", 0x0000ff),
            ("live green", 0x00ff00),
            ("live blue", 0xff0000),
        ] {
            paint(stimulus, color);
            settle();
            let s = engine.snapshot();
            let rendered = color_at(
                (s.lens.left + s.lens.right) / 2,
                (s.lens.top + s.lens.bottom) / 2,
            );
            checks.push(serde_json::json!({"test":name,"expected":color,"rendered":rendered,"pass":rendered==color}));
        }
        engine.action(Action::Zoom(8.)).unwrap();
        settle();
        let high = engine.snapshot();
        engine.action(Action::Zoom(2.)).unwrap();
        settle();
        let low = engine.snapshot();
        checks.push(serde_json::json!({"test":"zoom source geometry","pass":low.source.width()>high.source.width()*3,"at8x":high.source,"at2x":low.source}));
        paint(stimulus, 0);
        let dc = GetDC(Some(stimulus));
        let brush = CreateSolidBrush(COLORREF(0xffffff));
        FillRect(
            dc,
            &RECT {
                left: 200,
                top: 0,
                right: 208,
                bottom: 300,
            },
            brush,
        );
        let _ = DeleteObject(brush.into());
        ReleaseDC(Some(stimulus), dc);
        let mut counts = vec![];
        for z in [2., 8.] {
            engine.action(Action::Zoom(z)).unwrap();
            settle();
            let s = engine.snapshot();
            let y = (s.lens.top + s.lens.bottom) / 2;
            let n = (s.lens.left + 15..s.lens.right - 15)
                .filter(|x| color_at(*x, y) == 0xffffff)
                .count();
            counts.push(n);
        }
        checks.push(serde_json::json!({"test":"actual rendered scale","whitePixelsAt2x":counts[0],"whitePixelsAt8x":counts[1],"pass":counts[0]>=12 && counts[1]>=counts[0]*3}));
        settings = engine.snapshot().settings;
        settings.pixel_perfect = true;
        engine.apply(settings.clone()).unwrap();
        engine.action(Action::Zoom(8.)).unwrap();
        settle();
        let pixel = engine.snapshot();
        let y = (pixel.lens.top + pixel.lens.bottom) / 2;
        let n = (pixel.lens.left + 15..pixel.lens.right - 15)
            .filter(|x| color_at(*x, y) == 0xffffff)
            .count();
        checks.push(serde_json::json!({"test":"DXGI nearest original pixels","pass":pixel.inspector_active&&n==64,"whitePixels":n,"backend":pixel.backend,"error":pixel.error}));
        // Sample the existing inspection loop separately from live mode.
        thread::sleep(Duration::from_secs(2));
        let mut inspection_samples = Vec::new();
        for _ in 0..20 {
            inspection_samples.push(engine.snapshot().update_ms);
            thread::sleep(Duration::from_millis(50));
        }
        let inspection_hz = engine.snapshot().update_hz;
        settings.grid = true;
        settings.grid_threshold = 8.;
        engine.apply(settings.clone()).unwrap();
        settle();
        checks.push(serde_json::json!({"test":"grid backend active","pass":engine.snapshot().inspector_active}));
        let host = FindWindowW(w!("ZoominNativeLens"), None).unwrap();
        let mag = FindWindowExW(Some(host), None, w!("Magnifier"), None).unwrap();
        checks.push(serde_json::json!({"test":"inspection suspends duplicate native renderer","pass":!IsWindowVisible(mag).as_bool()}));
        let mut origin = POINT::default();
        let _ = ClientToScreen(mag, &mut origin);
        let grid_color = color_at(origin.x + 16, origin.y + 17);
        checks.push(serde_json::json!({"test":"original-pixel grid boundary","pass":grid_color==0x5a4b3c,"rendered":grid_color}));
        engine
            .action(Action::Position(Point { x: 200., y: 150. }))
            .unwrap();
        settle();
        let fallback = engine.snapshot();
        checks.push(serde_json::json!({"test":"inspection overlap restores live fallback","pass":!fallback.inspector_active&&IsWindowVisible(mag).as_bool()&&fallback.error.as_ref().is_some_and(|e|e.contains("away from the source"))}));
        engine
            .action(Action::Position(Point { x: 650., y: 200. }))
            .unwrap();
        settle();
        let recovered_capture = engine.snapshot();
        checks.push(serde_json::json!({"test":"inspection recovery clears capture warning","pass":recovered_capture.inspector_active&&!IsWindowVisible(mag).as_bool()&&recovered_capture.error.is_none()}));
        settings.pixel_perfect = false;
        settings.grid = false;
        engine.apply(settings.clone()).unwrap();
        engine.action(Action::Dock).unwrap();
        settle();
        checks.push(serde_json::json!({"test":"live renderer restored after inspection","pass":IsWindowVisible(mag).as_bool()}));
        let dock = engine.snapshot();
        checks.push(serde_json::json!({"test":"dock geometry","pass":dock.lens.width()>dock.lens.height()&&dock.visible}));
        // Default bindings register at IDs 100..107; exercise the actual
        // WM_HOTKEY dispatcher without injecting keys into another app.
        PostMessageW(Some(host), WM_HOTKEY, WPARAM(107), LPARAM(0)).unwrap();
        settle();
        let free = engine.snapshot();
        PostMessageW(Some(host), WM_HOTKEY, WPARAM(107), LPARAM(0)).unwrap();
        settle();
        checks.push(serde_json::json!({"test":"dock shortcut dispatch toggles free and docked","pass":free.settings.placement=="follow"&&engine.snapshot().settings.placement=="docked"}));
        engine.action(Action::Show(false)).unwrap();
        settle();
        checks.push(serde_json::json!({"test":"hide","pass":!engine.snapshot().visible}));
        if physical_input {
            chord(VIRTUAL_KEY(b'Z' as u16));
            checks.push(
            serde_json::json!({"test":"registered toggle hotkey","pass":engine.snapshot().visible}),
        );
            chord(VIRTUAL_KEY(b'1' as u16));
            checks.push(
            serde_json::json!({"test":"preset hotkey","pass":engine.snapshot().settings.zoom==2.}),
        );
            key(VK_CONTROL, true);
            key(VK_MENU, true);
            let wheel = INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        mouseData: 120,
                        dwFlags: MOUSEEVENTF_WHEEL,
                        ..Default::default()
                    },
                },
            };
            SendInput(&[wheel], std::mem::size_of::<INPUT>() as i32);
            settle();
            key(VK_MENU, false);
            key(VK_CONTROL, false);
            checks.push(serde_json::json!({"test":"modifier wheel","pass":engine.snapshot().settings.zoom==2.5}));
            let before_size = engine.snapshot().settings;
            key(VK_CONTROL, true);
            key(VK_SHIFT, true);
            SendInput(&[wheel], std::mem::size_of::<INPUT>() as i32);
            settle();
            key(VK_SHIFT, false);
            key(VK_CONTROL, false);
            let after_size = engine.snapshot().settings;
            checks.push(serde_json::json!({"test":"Ctrl+Shift wheel resize","pass":after_size.dock_size>before_size.dock_size&&after_size.zoom==before_size.zoom}));
            engine.action(Action::Show(false)).unwrap();
            settle();
            key(VK_CONTROL, true);
            key(VK_MENU, true);
            key(VIRTUAL_KEY(b'X' as u16), true);
            settle();
            let held = engine.snapshot().holding && engine.snapshot().visible;
            key(VK_CONTROL, false);
            settle();
            let released = !engine.snapshot().holding && !engine.snapshot().visible;
            key(VIRTUAL_KEY(b'X' as u16), false);
            key(VK_MENU, false);
            checks.push(serde_json::json!({"test":"hold release with modifier released first","pass":held&&released}));
            chord(VIRTUAL_KEY(b'Z' as u16));
            key(VK_ESCAPE, true);
            settle();
            key(VK_ESCAPE, false);
            checks.push(
                serde_json::json!({"test":"Escape dismissal","pass":!engine.snapshot().visible}),
            );
        } else {
            for name in [
                "registered toggle hotkey",
                "preset hotkey",
                "modifier wheel",
                "Ctrl+Shift wheel resize",
                "hold release with modifier released first",
                "Escape dismissal",
            ] {
                checks.push(serde_json::json!({"test":name,"pass":null,"status":"NOT TESTED","reason":"--no-input avoids physical input on an active desktop"}));
            }
        }
        engine.action(Action::Measure("region".into())).unwrap();
        settle();
        let measurement = FindWindowW(w!("ZoominMeasurement"), None).unwrap();
        let mut a = POINT { x: 150, y: 150 };
        ScreenToClient(measurement, &mut a).ok().unwrap();
        let mut b = POINT { x: 190, y: 180 };
        ScreenToClient(measurement, &mut b).ok().unwrap();
        SendMessageW(measurement, WM_LBUTTONDOWN, Some(WPARAM(1)), Some(pack(a)));
        SendMessageW(measurement, WM_LBUTTONDOWN, Some(WPARAM(1)), Some(pack(b)));
        settle();
        let result = engine.snapshot().measurement;
        checks.push(serde_json::json!({"test":"explicit region selection","pass":result.as_ref().is_some_and(|r|r.width==40&&r.height==30&&r.distance==50.),"result":result}));
        engine.action(Action::Measure("ruler".into())).unwrap();
        settle();
        engine.action(Action::CancelMeasure).unwrap();
        settle();
        checks.push(serde_json::json!({"test":"measurement cancellation","pass":engine.snapshot().measuring.is_none()}));
        let before = engine.snapshot().settings;
        let mut invalid = before.clone();
        invalid.shortcuts[1].chord = invalid.shortcuts[0].chord.clone();
        let rejected = engine.apply(invalid).is_err();
        checks.push(serde_json::json!({"test":"conflict rollback","pass":rejected&&engine.snapshot().settings==before}));
        settings = engine.snapshot().settings;
        settings.placement = "pinned".into();
        settings.click_through = true;
        engine.apply(settings.clone()).unwrap();
        engine
            .action(Action::Position(Point { x: 650., y: 200. }))
            .unwrap();
        engine.action(Action::Show(true)).unwrap();
        settle();
        // Native hit testing and exact gesture messages avoid timing-sensitive
        // physical mouse races while exercising the same Win32 callback path.
        SetWindowPos(
            stimulus,
            Some(HWND_NOTOPMOST),
            650,
            200,
            400,
            300,
            SWP_NOACTIVATE,
        )
        .unwrap();
        let s = engine.snapshot();
        let p = POINT {
            x: (s.lens.left + s.lens.right) / 2,
            y: (s.lens.top + s.lens.bottom) / 2,
        };
        checks.push(serde_json::json!({"test":"native click-through hit routing","pass":WindowFromPoint(p)==stimulus}));
        settings.click_through = false;
        engine.apply(settings.clone()).unwrap();
        settle();
        checks.push(serde_json::json!({"test":"interactive lens hit routing","pass":WindowFromPoint(p)!=stimulus}));
        let old = engine.snapshot().lens;
        let mut local = p;
        ScreenToClient(mag, &mut local).ok().unwrap();
        SendMessageW(mag, WM_LBUTTONDOWN, Some(WPARAM(1)), Some(pack(local)));
        let mut moved_point = POINT {
            x: p.x + 30,
            y: p.y + 20,
        };
        ScreenToClient(host, &mut moved_point).ok().unwrap();
        SendMessageW(host, WM_MOUSEMOVE, Some(WPARAM(1)), Some(pack(moved_point)));
        SendMessageW(host, WM_LBUTTONUP, Some(WPARAM(0)), Some(pack(moved_point)));
        settle();
        let moved = engine.snapshot().lens;
        checks.push(serde_json::json!({"test":"native lens drag dispatch","pass":moved.left==old.left+30&&moved.top==old.top+20,"before":old,"after":moved}));
        let corner = POINT {
            x: moved.right - 2,
            y: moved.bottom - 2,
        };
        SendMessageW(
            host,
            WM_NCLBUTTONDOWN,
            Some(WPARAM(HTBOTTOMRIGHT as usize)),
            Some(pack(corner)),
        );
        let mut resize_point = POINT {
            x: corner.x + 40,
            y: corner.y + 30,
        };
        ScreenToClient(host, &mut resize_point).ok().unwrap();
        SendMessageW(
            host,
            WM_MOUSEMOVE,
            Some(WPARAM(1)),
            Some(pack(resize_point)),
        );
        SendMessageW(
            host,
            WM_LBUTTONUP,
            Some(WPARAM(0)),
            Some(pack(resize_point)),
        );
        settle();
        let resized = engine.snapshot().lens;
        checks.push(serde_json::json!({"test":"native lens resize dispatch","pass":resized.width()==moved.width()+40&&resized.height()==moved.height()+30,"before":moved,"after":resized}));
        let size_before = engine.snapshot();
        engine.action(Action::Resize(1.)).unwrap();
        settle();
        let bigger = engine.snapshot();
        engine.action(Action::Resize(-1.)).unwrap();
        settle();
        let smaller = engine.snapshot();
        checks.push(serde_json::json!({"test":"wheel resize action scales floating lens without zoom","pass":bigger.lens.width()>size_before.lens.width()&&bigger.lens.height()>size_before.lens.height()&&(smaller.lens.width()-size_before.lens.width()).abs()<=1&&(smaller.lens.height()-size_before.lens.height()).abs()<=1&&bigger.settings.zoom==size_before.settings.zoom}));
        engine.action(Action::Dock).unwrap();
        settle();
        let dock_before = engine.snapshot();
        engine.action(Action::Resize(1.)).unwrap();
        settle();
        let dock_bigger = engine.snapshot();
        engine.action(Action::Resize(-1.)).unwrap();
        settle();
        checks.push(serde_json::json!({"test":"wheel resize action scales dock thickness","pass":dock_bigger.lens.height()>dock_before.lens.height()&&dock_bigger.lens.width()==dock_before.lens.width()&&(engine.snapshot().settings.dock_size-dock_before.settings.dock_size).abs()<1e-6}));
        settings.click_through = true;
        engine.apply(settings).unwrap();
        thread::sleep(Duration::from_secs(2));
        let performance = engine.snapshot();
        SendMessageW(host, WM_WTSSESSION_CHANGE, Some(WPARAM(7)), Some(LPARAM(0)));
        settle();
        checks.push(serde_json::json!({"test":"direct session notification dismissal","pass":!engine.snapshot().visible}));
        engine.action(Action::Show(true)).unwrap();
        settle();
        SendMessageW(
            host,
            WM_POWERBROADCAST,
            Some(WPARAM(PBT_APMSUSPEND as usize)),
            Some(LPARAM(0)),
        );
        settle();
        checks.push(serde_json::json!({"test":"direct power notification dismissal","pass":!engine.snapshot().visible}));
        let snapshot = engine.snapshot();
        engine.stop();
        let stopped = engine.snapshot();
        checks.push(serde_json::json!({"test":"shutdown clears transient status","pass":!stopped.visible&&!stopped.holding&&!stopped.inspector_active&&stopped.measuring.is_none()&&stopped.update_hz==0.}));
        // Simulate a real OS registration conflict without keyboard injection.
        RegisterHotKey(
            Some(stimulus),
            900,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            b'Z' as u32,
        )
        .unwrap();
        let recovered = Engine::start(Settings::default(), vec![]).unwrap();
        let blocked = recovered.snapshot();
        checks.push(serde_json::json!({"test":"shortcut conflict preserves native initialization","pass":blocked.initialized&&blocked.error.as_ref().is_some_and(|e|e.contains("Ctrl+Alt+Z"))}));
        UnregisterHotKey(Some(stimulus), 900).unwrap();
        recovered.apply(Settings::default()).unwrap();
        checks.push(serde_json::json!({"test":"shortcut registration recovers without restart","pass":recovered.snapshot().error.is_none()}));
        recovered.action(Action::Zoom(6.)).unwrap();
        recovered.stop();
        checks.push(serde_json::json!({"test":"shutdown drains queued settings changes","pass":recovered.snapshot().settings.zoom==6.}));
        checks.push(
            serde_json::json!({"test":"clean shutdown","pass":!engine.snapshot().initialized}),
        );
        let _ = DestroyWindow(stimulus);
        if physical_input {
            SetCursorPos(initial_cursor.x, initial_cursor.y).unwrap();
        }
        let passed = checks
            .iter()
            .filter(|v| !v["pass"].is_null())
            .all(|v| v["pass"] == true);
        let report = serde_json::json!({"passed":passed,"checks":checks,"environment":{"arch":std::env::consts::ARCH,"monitor":snapshot.monitor,"dpi":snapshot.dpi},"performance":{"nativeUpdateHz":performance.update_hz,"lastCpuUpdateMs":performance.update_ms,"inspectionUpdateHz":inspection_hz,"inspectionMeanCpuMs":inspection_samples.iter().sum::<f64>()/inspection_samples.len() as f64,"note":"Short debug-build stationary-target sample, not compositor FPS or end-to-end latency"},"snapshot":snapshot});
        std::fs::create_dir_all("artifacts").unwrap();
        std::fs::write(
            "artifacts/native-check.json",
            serde_json::to_string_pretty(&report).unwrap(),
        )
        .unwrap();
        println!("{}", serde_json::to_string_pretty(&report).unwrap());
        if !passed {
            std::process::exit(1);
        }
    }
}
