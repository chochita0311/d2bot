# GUI Maintenance

This document holds GUI-specific maintenance notes that are referenced from `AGENTS.md`.

## Main Window

- The main desktop window lives in `diablo2/ui/gui.py`.
- Keep `diablo2/ui/gui.py` as the main reference point for GUI layout and window-size tuning guidance.

## DPI And Capture Selection

- Configure Windows DPI awareness before creating the Tk root window. Late setup by capture code can change the helper window's size on first interaction.
- Exclude the `Diablo2 Helper` panel from game-window candidates when a game window is available. Preserve the configured title and preferred Diablo title matching in `_choose_default_window`.
- Verify helper geometry and the selected capture target together after changing startup or capture initialization.

## Window Size Controls

Default startup size is controlled by:

- `DEFAULT_WINDOW_WIDTH`
- `DEFAULT_WINDOW_HEIGHT`
- `MIN_WINDOW_WIDTH`
- `MIN_WINDOW_HEIGHT`

When adjusting the main window, prefer changing those constants instead of scattering raw geometry strings.

## Layout Rule

- Preserve the existing layout rule that the log panel expands while the right control panel stays close to its natural width.
