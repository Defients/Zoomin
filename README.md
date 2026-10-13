# Zoomin'

See closer. See more.

Zoomin' is a Windows desktop magnifier built with Tauri 2, React, TypeScript, Tailwind CSS and Rust. The live lens is a real Win32 Magnification control, independent of the settings WebView. It magnifies changing desktop content around the pointer. No screen frames travel through React or Tauri IPC.

This is a functional **0.1.0 development release**, not a feature-complete 1.0 release. See [CHANGELOG.md](CHANGELOG.md) for improvements, [TESTING.md](TESTING.md) for executed checks and [ROADMAP.md](ROADMAP.md) for release gates.

## Requirements

- Windows 11 x64 recommended. Windows 10 version 2004+ is the intended minimum for capture exclusion; Windows 10 runtime has not been tested here.
- WDDM graphics driver and Microsoft Edge WebView2 Runtime. The installer can download WebView2 if missing; normal use works offline.
- Development: Node.js 22.12+ (tested 24.21), Rust stable MSVC (tested 1.99), Visual Studio C++ Build Tools and Windows SDK.
- No Administrator or UIAccess privileges required.

## Build and run

From a PowerShell terminal in this folder:

```powershell
npm ci
npm run tauri dev
```

Production executable and per-user installer:

```powershell
npm run package
```

The workspace puts output in `target/release/zoomin.exe` and `target/release/bundle/nsis/Zoomin'_0.1.0_x64-setup.exe`. The executable requires WebView2 to be installed. The installer is unsigned.

Other commands:

```powershell
npm run build
npm run lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run test:native
npm run check:native
cargo run -p zoomin-engine --bin native-check -- --no-input
npm run test:ui
```

`check:native` creates temporary synthetic test windows and sends test shortcuts. Close any running Zoomin' instance first; keep modifiers released and the desktop idle. The `--no-input` variant uses native window messages and marks the physical shortcut checks NOT TESTED. It cleans up resources and saves numerical results to `artifacts/native-check.json`, without saving user desktop images. The UI test launches and closes its own release app through a temporary local WebView2 test transport and saves settings-only screenshots. `npm run dev` alone serves the frontend; it intentionally shows a native-app notice in ordinary browsers.

## Use

| Default control | Action |
|---|---|
| Ctrl + Alt + Z | Show / hide lens |
| Ctrl + Alt + X, held | Temporary magnification |
| Ctrl + Alt + mouse wheel | Change zoom |
| Ctrl + Shift + mouse wheel | Scale lens size up/down (10% per notch) |
| Ctrl + Alt + 1 / 2 / 3 | 2× / 4× / 8× presets |
| Ctrl + Alt + P | Pin / unpin source target |
| Ctrl + Alt + G | Toggle inspection grid |
| Ctrl + Alt + D | Toggle docked / free (cursor-following) lens |
| Escape | Hide lens or cancel measurement |

Settings edits take effect with **Apply changes**. Registration conflicts preserve the previous working shortcuts. If a shortcut is unavailable at startup, the lens stays available from settings and the tray; change the conflicting binding in Controls and apply to retry without restarting. Use Ctrl, Alt or Win plus A–Z, 0–9 or F1–F24; Shift can be an additional modifier. Zoom uses Ctrl + Alt + wheel; size uses Ctrl + Shift + wheel. These wheel modifiers are fixed in this version.

The lens can follow the pointer, stay pinned as a window, or dock to a work-area edge. Tracking can be instant, smoothed, or fixed. For dragging/resizing, choose **Pinned window**, turn off **Click-through**, then drag the center or edges. Closing settings leaves the utility in the tray. Use **Exit Zoomin'** to terminate it.

Ctrl + Shift + wheel up makes the lens bigger; wheel down makes it smaller. Floating dimensions scale together within their size limits, preserving proportions. In docked mode it scales dock thickness. Zoom stays unchanged; size changes are saved automatically. Ctrl + Alt takes precedence when Alt and Shift are both held.

Following lenses keep their vertical cursor offset while resizing. With the default positive offset the lens stays below the cursor; its lower edge may extend beyond the display instead of flipping above the cursor.

Pixel-perfect mode uses DXGI Desktop Duplication, a GPU-side desktop cache, region-only readback, and native nearest-neighbor presentation. Keep the source clear of Zoomin' windows. Original pixel colors appear as HEX, RGB and HSL and can be copied. Ruler and region tools use an explicit two-click desktop selection; Escape or right-click cancels.

Settings use versioned, validated JSON at `%APPDATA%\com.zoomin.desktop\settings.json`. Writes use a synced temporary file and atomic replacement. Invalid files produce a warning and defaults; simply opening and closing the app preserves the original file. Explicit Apply/Reset or a saved runtime preference change replaces it. Edits made while Apply or Reset is pending remain unsaved in the draft. Startup is optional and disabled by default.

## Limitations

- Fullscreen OS magnification, spotlight, circular/rounded composition, glow/shadow controls, light theme, selectable monitor UI and custom wheel modifiers are not implemented.
- The native lens has no zoom-label HUD yet; zoom is shown in settings. Pinned window positions do not persist across restarts.
- Native geometry and API behavior are tested at 2560×1440, 100% scaling on this machine. Physical mixed-DPI and multi-monitor, sleep/resume, lock/unlock and monitor-disconnection verification remain outstanding.
- Pixel inspection has a native CPU presentation step; it is not an all-GPU renderer. Large lenses at low zoom cost more. Rotated DXGI outputs fall back to live magnification. HDR and color-managed accuracy are not guaranteed.
- Zoomin' windows are excluded from the live magnifier. DXGI cannot reveal the original pixels behind its own overlapping overlay; that case falls back to the live renderer. Color samples covered by app windows are omitted.
- Protected content, secure desktops and exclusive fullscreen applications can be unavailable. No privilege bypass is attempted.
- Local-only capture uses `WDA_EXCLUDEFROMCAPTURE`, independently from the magnifier filter. Capture programs differ; OBS Display, Window and Game Capture have not been verified.
- Source edges are clamped to a single nearest monitor. A lens does not stitch pixels from two source monitors. The dock overlays existing windows without reserving work area.
- Diagnostics report native update-loop rate and CPU wall time, not compositor FPS, GPU latency or end-to-end cursor latency.

No telemetry, input history, network service, persistent screen recording or screen uploads are implemented.

