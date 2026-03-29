# Summoner Route Lessons

This document records route-specific lessons learned while building and tuning the Summoner flow.

## Waypoint entry behavior

- In Act 1 town, the minimap waypoint marker is useful as direction guidance only; the bot should never click the minimap marker itself.
- The real interaction target must be the world waypoint object on the game screen.
- For this route, minimap is assumed already open on room entry.
- A smoother search pattern works better than repeated stop-and-go clicks:
  - move slightly right from town center
  - hold upward briefly
  - if still missing, hold downward longer
  - if still missing, hold right briefly
- During hold-based movement, waypoint detection should run continuously so the bot can release early as soon as the waypoint becomes visible.
- After hold-based detection, the bot should re-acquire the waypoint from the current frame before moving to click it, otherwise it may use a stale on-the-way coordinate.

## Waypoint panel behavior

- After opening the Act 1 waypoint list, switching to Act 2 and clicking Arcane Sanctuary is stable enough for the first Summoner payload milestone.
- Panel clicks are safer as panel-relative ratios than fixed pixel offsets, because Diablo window size can change.

## Desktop GUI behavior

- The control panel window resized on first interaction because DPI awareness was being enabled later by screen capture startup.
- DPI awareness must be configured before Tk creates the GUI window.
- The helper window itself should be excluded from auto-window selection; prefer the real Diablo II: Resurrected window by title match first.
