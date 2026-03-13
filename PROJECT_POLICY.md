# Project Policy

This project is a Windows GUI automation and vision-learning project for Diablo 2 style gameplay analysis and assistance.

## Allowed technical boundaries

- Screen capture only
- Computer vision only, including OpenCV template matching and OCR
- Standard Windows mouse and keyboard input only
- Config-driven rules, overlays, logging, recording, and replay analysis
- Dry-run and human-supervised workflows

## Disallowed technical boundaries

- Memory reading
- Packet inspection or packet injection
- DLL injection
- Code injection
- Game client modification
- Reverse engineering for hidden/internal state access
- Process hooking or API hooking into the game client
- Anything intended to bypass platform, account, or game enforcement

## Safety and design rules

- Prefer vision-first decisions over internal-process access
- Keep features testable on screenshots or recorded video when possible
- Default new behaviors to dry-run before enabling live input
- Keep pause and stop hotkeys available in any live automation mode
- Log important decisions so behavior can be reviewed after a run
- Favor stable, explicit rules over opaque behavior

## Policy on evasion

This project must not add features whose purpose is to avoid detection, imitate human behavior to escape enforcement, or otherwise reduce the chance of being caught by game policy systems.

## Practical goal

The intended direction is a GUI-level assistant or automation scaffold that can:

- Observe the game window
- Detect UI and loot states from images
- Track farm runs and outcomes
- Optionally perform ordinary mouse and keyboard actions

All of that must stay within the boundaries above.
