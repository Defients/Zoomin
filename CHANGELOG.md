# Changelog

## 0.1.0 — cursor placement correction, 2026-10-12

- Keep the floating lens at its vertical cursor offset as it scales. Remove the automatic flip above the cursor when the lower edge no longer fits; the lens may extend beyond the display bottom. Explicit negative offsets remain supported.

## 0.1.0 — wheel resizing, 2026-10-12

- Ctrl + Shift + wheel scales floating lens dimensions together by 10% per notch (inverse for wheel down), or adjusts dock thickness. Existing size bounds apply; zoom stays unchanged.
- Keep Ctrl + Alt + wheel zoom behavior and pass unrelated wheel gestures through. Size changes use the existing debounced preferences save.

## 0.1.0 — dock shortcut, 2026-10-12

- Add configurable Ctrl + Alt + D to toggle docked/free placement through the existing dock action. Change it in Controls and apply.
- Existing settings retain their shortcuts; loading a seven-binding file adds the dock binding, using an unused Ctrl + Alt + function key if D is already assigned.

## 0.1.0 — enhancement pass, 2026-10-12

Version-1 settings and existing Tauri command names remain compatible. No new product features were added.

- Preserve an untouched corrupt settings file on exit; share the saved baseline to avoid duplicate writes. Failed saves remain eligible for retry, and rollback failures are reported.
- Keep the native lens and tray available when initial shortcut registration conflicts. Applying available bindings recovers without a restart.
- Stop disconnected native workers, drain queued settings actions on exit and clear stale visible/hold/inspection/measurement status.
- Render through one active backend. Skip hidden magnifier work during inspection, unnecessary overlay positioning/visibility/opacity calls and redundant bitmap clearing. Refresh live fallback in the same tick; clear recovered capture warnings without clearing unrelated errors.
- Preserve edits made during Apply/Reset. Reset still restores untouched fields; overlapping settings requests are guarded.
- Validate action indices and nonfinite zoom inputs; expose active navigation and live status to assistive technology; label hidden inspector values as the last sample.
- Add regressions for persistence failure/retry, shortcut conflict/recovery, renderer fallback, teardown, action validation and delayed settings responses. Refuse UI-test attachment when its debugging port is already occupied.

Recorded inspection benchmark: mean CPU wall time 8.60 → 4.42 ms/update (48.5% lower), source loop 47.9 → 58.4 Hz, on the same stationary synthetic target in short debug runs. These are not compositor FPS or end-to-end latency measurements. See [TESTING.md](TESTING.md) for evidence and outstanding compatibility checks.
