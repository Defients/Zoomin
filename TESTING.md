# Verification

Results dated 2026-10-12 (America/New_York). PASS means executed; unexecuted work is explicitly NOT TESTED or BLOCKED.

## Environment

- Windows 11 Home Insider Preview, build 26340, x86_64.
- Intel Core i7-13700F; NVIDIA GeForce RTX 4060 Ti, driver 32.0.16.1742; approximately 32 GiB RAM. A spacedesk graphics adapter is also installed.
- Native synthetic tests target the 2560×1440 primary display at 96 DPI / 100%. The settings UI also reported a 1920×1080 source monitor during cursor-following use. The formal mixed-DPI/topology matrix was not run.
- Node 24.21.0, Rust 1.99.0 MSVC, Windows SDK 10.0.26100.0. Versions are locked in Cargo.lock and package-lock.json.

## Automated checks

| Check | Status | Evidence |
|---|---|---|
| Strict TypeScript and Vite production build | PASS | `npm run build` |
| ESLint | PASS | `npm run lint` |
| Rust formatting | PASS | `cargo fmt --all -- --check` |
| Rust Clippy with warnings denied | PASS | `cargo clippy --workspace --all-targets -- -D warnings` |
| Rust unit tests | PASS | 22 tests across the workspace: geometry, DPI, zoom, damping, pixel mapping, measurements, chords, migration/atomic persistence, corrupt-file preservation, save retry, action validation and color conversion |
| Native runtime, deterministic final run | PASS | 31 executed checks; 6 physical input checks NOT TESTED in this run. `artifacts/native-check.json` |
| Packaged/installed UI integration | PASS | One integration scenario, 11.4 seconds, zero failures/retries: real WebView2/Rust app, toggle/presets, settings, shortcut capture, validation, profiles, six pages, overflow, delayed Apply/Reset and active navigation. `artifacts/ui-tests.json` |
| Per-user NSIS build | PASS | Executable and installer generated; final paths below |
| Installer install/uninstall lifecycle | PASS | `artifacts/installer-check.json` records exit codes, executable, registry and shortcut target/cleanup |

The final deterministic native run checks changing red/green/blue desktop content, source geometry, actual rendered scaling, DXGI nearest rendering, original-pixel grid boundaries, dock geometry, hiding, explicit region selection/cancellation, conflict validation, Windows click-through hit routing, drag/resize callback dispatch, direct session/power notification dismissal and clean teardown. It uses live desktop test windows, not screenshot substitutes. A physical 8-pixel white bar renders as 16 and 64 pixels at 2× and 8× respectively. The 40×30 region produces a 50-pixel point distance.

Enhancement regressions also verify that inspection suspends the duplicate native renderer, source overlap restores live fallback, capture recovery clears its warning, startup shortcut conflicts preserve engine initialization, applying available shortcuts recovers without restart, shutdown clears transient status and queued preference changes survive teardown. The dock shortcut is exercised through native WM_HOTKEY dispatch in both directions; legacy binding migration also has a unit regression. A placement regression sweeps lens heights across the previous vertical-flip threshold and verifies a stable cursor offset, including negative desktop coordinates and explicit negative offsets. Wheel-size native actions verify floating growth/shrink and dock thickness; unit tests verify modifier routing, proportions, size limits and unchanged zoom. The new physical Ctrl + Shift wheel check is NOT TESTED in the final no-input run. Persistence tests use owned temporary files and deliberately failed writes, preserving an untouched corrupt file and retrying after failure.

The installed UI test delays real Apply and Reset replies by 900 ms. A newer width edit survives Apply; a newer height edit survives Reset while the untouched width returns to its default. Both pending edits subsequently save through the real native commands. The active navigation label passes its ARIA assertion. Install/uninstall exit codes are zero; the installed executable SHA-256 matches the release executable, and the smoke-test executable, registry entry and Start Menu shortcut are removed. Starting preferences are restored.

Earlier physical-input runs exercised actual registered toggle and preset shortcuts, Ctrl + Alt + wheel, modifier-first hold release and Escape successfully. Later runs on an active desktop were inconsistent; the saved `artifacts/native-check-physical.json` records failures rather than hiding them. A requested idle run reproduced the center-drag defect. The child hit-test/gesture routing was subsequently corrected and exact native dispatch now passes. Physical manual drag/resize stress remains NOT TESTED on the final binary; deterministic dispatch is not claimed as a substitute for that matrix.

Two release issues were caught and fixed: Common Controls v6 was restored to the custom PMv2 manifest after an entry-point startup failure, and NSIS shortcut COM macros use apostrophe-safe quoting for the product name. The executable and installer were successfully run after these fixes.

## Performance sample

The enhancement comparison uses the same stationary synthetic source, 8× zoom, default floating dimensions and debug engine. After two seconds of settling, twenty CPU wall-time observations are collected at 50 ms intervals. No release build ran during these two recorded samples.

| Inspection measurement | Before | After |
|---|---|---|
| Mean native CPU wall time per update | 8.597 ms | 4.425 ms |
| Native source-loop update rate | 47.891 Hz | 58.412 Hz |

The recorded mean CPU wall time is **48.5% lower**. Raw evidence is `artifacts/enhancement-before.json` and `artifacts/enhancement-after.json`. Desktop activity causes run-to-run variability. These short samples are not a statistical benchmark, compositor FPS, GPU time or end-to-end cursor latency. The sustained 60 Hz goal has **not** been demonstrated. CPU utilization, GPU utilization, complete process-tree memory, continuous motion latency and release-build performance remain NOT TESTED.

## Reproduce

```powershell
npm ci
npm run build
npm run lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p zoomin-engine --bin native-check -- --no-input
```

For physical shortcut checks, close Zoomin', keep the desktop idle and modifiers released, then run `npm run check:native`. Synthetic test windows and selection overlays appear temporarily. Normal input is not logged. Run an installed or packaged UI test with:

```powershell
npm run package
npm run test:ui
# Or choose an installed test executable:
powershell -NoProfile -File scripts/test-ui.ps1 -Executable C:\path\to\zoomin.exe
```

The UI launcher refuses to replace an already running Zoomin' instance or attach when debugging port 9222 is already occupied. Its local debugging transport is enabled only through the test process environment; normal runs do not open that port. The tests restore starting preferences and hide the lens. Screenshots capture the settings WebView only.

## Manual Windows matrix

| Scenario | Status | Reproducible acceptance |
|---|---|---|
| 1440p, 100% native desktop magnification | PASS, automated synthetic content | Change source colors and inspect 2× / 8× rendered bar widths |
| Single-display 1080p / 4K | NOT TESTED | Native lens follows text and changing applications; verify original source extents |
| 125%, 150%, 200% scaling | NOT TESTED | Logical lens sizes convert once; sampled coordinates remain physical |
| Dual-display mixed scaling | NOT TESTED | Cross both directions, pin source on one display, move lens on the other |
| Negative origins / display above primary | NOT TESTED physically; unit math PASS | Check source, placement, local coordinates and edge clamping |
| Display boundary motion | NOT TESTED formally | No jump in zoom, stale source or lost lens; clamp edges predictably |
| Physical lens dragging and resizing | NOT TESTED on final binary; native dispatch PASS | Pinned placement, click-through off; drag center, resize all corners, dismiss during gesture |
| Click-through in ordinary applications | Synthetic native target PASS earlier; final hit routing PASS | Click buttons and scroll a browser through the lens |
| Browser text / continuous motion / fractional zoom | NOT TESTED manually | Exercise 1–12×, retarget zoom during transition, compare a known pixel test chart |
| Hold release / modifier ordering | PASS in an earlier idle run; final idle repeat required | Release modifier first, alt-tab, dismiss while held, then press/release again |
| Lock/unlock and secure desktop | NOT TESTED | Lens dismisses, holds clear, no input trap after return |
| Sleep/resume | NOT TESTED | Capture is recreated and shortcuts remain usable |
| Explorer restart | NOT TESTED | Tray recovery and clean exit remain available |
| Monitor unplug/reconnect / resolution changes | NOT TESTED | No offscreen trap; nearest source and capture recover |
| Fullscreen / elevated application content | NOT TESTED | Available content magnifies; unavailable content fails safely |
| HDR / color management | NOT TESTED | Compare original values with a reference; document SDR conversion limits |
| Rotated monitor | NOT TESTED | Pixel inspection reports unsupported rotation and falls back safely |
| Startup at sign-in | NOT TESTED | Enable, sign out/in, app starts hidden in tray; disable removes entry |
| Duplicate instance | NOT TESTED formally | A second launch opens settings and does not create another engine/hook |
| Clean engine teardown | PASS | Native check closes windows, removes hook/hotkeys and uninitializes |
| Windows 10 baseline runtime | BLOCKED by available OS | Repeat full matrix on Windows 10 2004+ x64 |

## OBS compatibility plan

OBS is **NOT TESTED**. For Display Capture, Window Capture and Game Capture separately: make a local recording with each visibility profile, show/hide the lens, move and dock it, enable crosshair/pixel mode, and review the recorded result. Check both overlay visibility and original content near it. Include a fullscreen application and a display crossing. Local-only uses Windows capture affinity but does not guarantee invisibility in all capture paths. Do not broadcast until the chosen capture method has been checked.

## Artifacts and release blockers

- `target/release/zoomin.exe`
- `target/release/bundle/nsis/Zoomin'_0.1.0_x64-setup.exe`
- Numerical runtime and installer reports in `artifacts/`; settings screenshots in `artifacts/*.png`.

Remaining broad-release blockers: manual mixed-DPI/window/lifecycle matrix, repeatable final physical-input stress, achieving/measuring the 60 Hz target, OBS/HDR/fullscreen compatibility, and code signing. Advanced unimplemented visuals and configurability are listed in ROADMAP.md. This release must not be described as feature-complete.


