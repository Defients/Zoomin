# Roadmap and release gates

Status is for the 0.1.0 development release. Runtime evidence comes from synthetic native content and input on one Windows environment, not the full compatibility matrix.

| Milestone | Implemented outcome | Acceptance / remaining work |
|---|---|---|
| A — real native lens | PASS: changing native desktop pixels, source tracking and real 2×/8× rendering verified | Repeat on baseline Windows 10/11 hardware |
| B — input and zoom | PASS on this desktop: actual registered toggle/presets, modifier wheel, hold release, Escape; typed settings | More conflict/repeat/focus/session coverage; improve capture of already registered chords |
| C — floating/docked | Native placement modes, top/bottom/left/right overlay dock, click-through, manual resize hit tests | Physical mixed-DPI, dragging/resize and monitor topology matrix |
| D — Tauri interface | Six settings pages, draft/apply, onboarding, tray, startup, clipboard, persistence | Packaged UI / installer verification results in TESTING.md |
| E — inspection | DXGI original-pixel nearest-neighbor renderer, grid, color formats, crosshair, ruler/region selection | More pixel/color validation at fractional zoom, HDR, rotated-output support, GPU presentation |
| F — hardening/visuals | Capture profiles, update diagnostics, release recovery paths | Spotlight; rounded/circular composition; glow/shadow; opacity customization; OBS validation |
| G — packaging | Release build and per-user NSIS configuration | Results and artifact paths in TESTING.md; signing and broader install matrix |

## Release gates

- **A Actual magnification: PASS** on the tested machine. The live lens visibly responds to synthetic desktop color changes and a physical bar scales from 16 to 64 rendered pixels at 2× vs 8×.
- **B Reliable control: PASS for executed native tests**, pending physical lock/sleep/focus stress and repeated usage. Hold reconciliation and Escape are exercised.
- **C Window quality: PARTIAL**. Native docking and dismissal pass; full manual DPI/window interaction matrix remains required.
- **D Inspection accuracy: PARTIAL**. DXGI nearest rendering and original-coordinate region measurement pass; extended fractional grid/color/HDR cases remain.
- **E Release hardening: PARTIAL** until the manual compatibility matrix and installer lifecycle checks listed in TESTING.md are satisfied.

Do not label this feature-complete or broadly release-verified.

## BACKLOG / RFC — deferred, not implemented in this enhancement pass

The current pass fixes existing persistence, shortcut recovery, rendering and draft behavior. The items below remain future work; they expand functionality or require a separate compatibility/performance investigation.

1. Finish physical multi-monitor/mixed-DPI and lifecycle compatibility testing; address observed failures before release.
2. Add a shader-based inspection presenter to remove CPU scaling/readback where justified by measured performance.
3. Persist pinned window positions; provide monitor selection, dock collapse and richer placement controls.
4. Improve shortcut capture for currently registered combinations; allow configurable wheel modifiers and more key families.
5. Add a native zoom-label HUD, adjustable grid/crosshair opacity and colors, rounded/circular regions with verified native composition, and restrained shadow/glow.
6. Implement spotlight as a safely dismissible native overlay after geometry/capture compatibility is established.
7. Validate OBS Display/Window/Game Capture separately; document HDR, protected content, fullscreen and rotated displays.
8. Add light/system themes, fuller keyboard/screen-reader testing and localization.
9. Sign installers for public distribution; verify install, update and uninstall on clean Windows 10 and 11 images.
