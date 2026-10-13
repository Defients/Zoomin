# Architecture

## Native core

`native-engine` is a standalone Rust crate, reused by `src-tauri`. The initial proof evolved directly into the application engine. `Engine` owns a command sender, shared small status snapshot and join handle. A single native thread owns initialization, HWNDs, hooks, registered shortcuts, Direct3D objects, measurement overlays and teardown. No HWND is operated on by React or Tauri's main thread.

The live path initializes `MagInitialize`, creates an opaque layered, topmost, nonactivating tool window with a `Magnifier` child, applies `MagSetWindowTransform`, updates `MagSetWindowSource`, and uses `MW_FILTERMODE_EXCLUDE` for app windows. Teardown unregisters shortcuts, removes the wheel hook, destroys windows, releases GDI resources and calls `MagUninitialize`. Partial initialization also uses the same RAII cleanup.

The host requires a layered style. Click-through uses native `WS_EX_TRANSPARENT`; it is independent from DOM interaction. Interaction mode uses native caption/resize hit tests. A separate owned, nonactivating layered window presents inspection pixels or a transparent crosshair. Escape and hold reconciliation use current key-state polling. The app never changes OS fullscreen magnification or input transforms.

The Tauri manifest retains Common Controls v6, requests Per-Monitor V2 awareness, and runs asInvoker without UIAccess. The native thread also sets PMv2 explicitly. x64 is required: Microsoft's Magnification API does not support WOW64.

## Coordinates and timing

`geometry.rs` contains the conversions and math. All cursor, monitor bounds, source rectangles and measurements are **physical desktop pixels**, including negative origins. User lens dimensions/offsets are logical pixels, converted at the placement monitor's DPI. Source tracking and lens positioning are independent.

Source extents are `ceil(client_size / displayed_zoom)`. Rectangles shift within the nearest source monitor at edges; the control still uses the requested fractional transform, with any extra final pixel clipped by its client area. The target crosshair maps the original sampled pixel's center, including when edge clamping shifts the source rectangle. Inspection uses `floor(destination_pixel / displayed_zoom)` to choose an original source pixel. Grid boundaries derive from `round(source_pixel_boundary * displayed_zoom)`.

Zoom target and displayed zoom are distinct. Time-based exponential damping avoids overshoot and can be retargeted. Pointer smoothing uses its own time constant. A high-resolution waitable timer schedules visible updates toward 60 Hz; hidden mode pumps messages and reconciles releases at 40 Hz without magnifier rendering. UI status events run at 4 Hz. Update rate is an observation, not a compositor frame-rate promise.

## Input

Native `RegisterHotKey` is suitable for this Windows-only engine and keeps registration, dispatch and release reconciliation on the same owning thread. A narrowly scoped `WH_MOUSE_LL` hook handles Ctrl + Alt + wheel for zoom and Ctrl + Shift + wheel for size; it consumes only those wheel events and immediately passes all other events through. There is no keyboard hook or input logging.

Settings validation canonicalizes chords and rejects internal conflicts. Registration stages new keys before releasing prior registrations; failed new registration removes staged additions and retains the previous set. Initial registration failure reports a warning while leaving the engine and tray available for recovery. Native window configuration is checked before prior bindings are released. Hold state clears when any required key is up, and Escape suppresses it until release. Display/power/session messages dismiss transient overlays and invalidate capture. Secure-desktop cursor failures hide the lens. A disconnected command channel terminates the worker; shutdown drains queued actions and clears transient status.

Shortcut capture uses the browser's key event and physical key code. An already registered combination is owned by Windows and may fire its existing action instead of reaching the capture field. Fresh combinations can be captured; this limitation is a follow-up item.

## Inspection and measurement

Microsoft now recommends Windows.Graphics.Capture or Desktop Duplication for new capture work. Its magnifier remains the smallest working live-control path here, so it is preserved rather than replaced. An isolated optional DXGI/D3D11 backend supplies precise pixels. It selects the adapter/output for the source monitor, acquires/release-pairs frames, keeps a full desktop **GPU** texture for static-screen panning, and copies only the source region into a reusable staging texture. Native nearest-neighbor presentation uses an in-memory bitmap, without Tauri IPC.

This is a deliberate early tradeoff: inspection has a small region readback plus CPU scaling/GDI presentation, rather than a D3D shader/swap-chain compositor. Live mode remains GPU/OS-managed. Unsupported rotated outputs, capture invalidation or app-window overlap falls back to the live magnifier and reports why. Device/output errors release the capture objects and retry after a delay. Display changes also discard capture.

Only the selected backend renders each tick. Active inspection hides the magnifier child and skips its source/transform/invalidation work. Fallback refreshes that child in the same tick before showing it. Overlay placement, opacity mode and visibility change only when necessary; the inspection buffer is completely overwritten without a redundant clear. Successful capture recovery clears its own warning while preserving unrelated errors.

Grid and crosshair are drawn from original source geometry. DXGI colors come from unscaled region bytes. Live mode uses GDI for a single source pixel and omits app-covered points. Neither path promises HDR/color-management accuracy or protected-content access.

Measurement uses a temporary dim native window across the virtual desktop. Only this explicit mode receives desktop clicks. Its two points are physical coordinates, and its state has no input history. Completion, right-click, Escape, session or topology changes destroy the selection window. Dimensions and distance derive from original desktop coordinates.

## Settings and UI boundary

Rust is authoritative. Typed `get_state`, `perform_action`, `apply_settings`, `reset_settings`, `copy_text` and `exit_app` commands carry small values. React holds a draft while editing; runtime events do not overwrite unsaved edits. An edit revision and request guard protect newer edits from late Apply/Reset replies; reset replaces only fields not edited during the request. Explicit apply validates and registers the native configuration, updates optional startup and persists; failures roll back and report rollback failures. Preset actions reject fractional/out-of-range indices, and zoom actions reject nonfinite input.

Hotkey/tray-driven preference changes are saved with a debounce. A transaction lock prevents competing temporary-file writes. `SettingsStore` shares one successfully saved baseline across explicit apply, debounce and exit, avoiding duplicate writes and preserving untouched corrupt files. Failed saves stay dirty for retry. Exit stops the native worker before flushing its final preferences. Configuration and transient native/hold/capture state stay separate. Command names and version-1 settings remain unchanged.

The Tauri shell supplies single-instance handling, tray menus, startup integration, clipboard access and hide-on-close. Basic use has no network dependency. The installer uses current-user NSIS, with a small shortcut-COM quote override to preserve the apostrophe in the product name. Packaging skips Tauri's per-bundle updater marker patch, since no updater is implemented; the installed executable is byte-identical to the release executable.

## Primary API references

- [Microsoft Magnification API overview](https://learn.microsoft.com/en-us/previous-versions/windows/desktop/magapi/magapi-intro)
- [Magnifier exclusion filtering](https://learn.microsoft.com/en-us/windows/win32/api/magnification/nf-magnification-magsetwindowfilterlist)
- [Desktop Duplication API](https://learn.microsoft.com/en-us/windows/win32/direct3ddxgi/desktop-dup-api)
- [Window capture affinity](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowdisplayaffinity)
- [Tauri tray](https://v2.tauri.app/learn/system-tray/) and [startup](https://v2.tauri.app/plugin/autostart/)
