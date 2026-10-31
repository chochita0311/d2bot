# GUI Maintenance

This document holds GUI-specific maintenance notes that are referenced from `AGENTS.md`.

## Main Window

- The Rust executable's panel lives in `src/ui/`: `model` owns pure state, `runtime` owns sessions, `native` adapts Windows effects, `host` owns the event loop/close protocol and `view` draws it. Its default executable entry is connected under FEAT-0027; the session-preview example uses fake effects only. Final package/native acceptance is tracked by FEAT-0028.
- Historical Tkinter layout evidence remains in the pinned Git baseline; current maintenance targets the Rust panel and its tests.

## DPI And Capture Selection

- Configure Windows DPI awareness before creating a native window (before the Rust event loop). Late setup by capture code can change the helper window's size on first interaction.
- Exclude the `Diablo2 Helper` panel from game-window candidates when a game window is available. Preserve configured-title and preferred-Diablo matching in the Rust model.
- Verify helper geometry and the selected capture target together after changing startup or capture initialization.
- Rust startup/minimum dimensions are physical pixels. `host::native_options` queries system DPI after awareness setup; `view` applies actual viewport DPI and updates minimum size on monitor changes without resetting the user's size. Do not pass raw physical dimensions as egui logical points.
- Rust reads Segoe UI from Windows when available and falls back to configured system/bundled fonts. Do not copy or redistribute the OS font.

## Window Size Controls

Rust startup/minimum size is controlled by `INITIAL_SIZE` and `MINIMUM_SIZE` in `src/ui/model.rs`, converted for DPI by `host::native_options`. Change those constants rather than scattering geometry values.

## Layout Rule

- Preserve the existing layout rule that the log panel expands while the right control panel stays close to its natural width.
- Keep Stop visible. At high DPI/minimum size, the lower play controls scroll in a bounded area. The log scrolls independently: follow new content while at the bottom, preserve the reader's scroll while above it, and resume following when the reader returns to the bottom. Use egui's sticky-bottom behavior rather than forcing an infinite offset on every log revision. The configured retained-line limit still removes old entries. Literal log text is not Markdown.
- Closing requests cancellation without joining workers on the UI thread. Disable new work while closing and permit final window close only after every owned session finishes; report explicit cleanup errors.
