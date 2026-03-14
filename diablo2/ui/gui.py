from __future__ import annotations

import os
import sys
from pathlib import Path


def _configure_tk_environment() -> None:
    base_root = Path(sys.base_prefix)
    candidates = [
        (base_root / 'tcl' / 'tcl8.6', base_root / 'tcl' / 'tk8.6'),
        (base_root / 'Library' / 'lib' / 'tcl8.6', base_root / 'Library' / 'lib' / 'tk8.6'),
    ]
    for tcl_path, tk_path in candidates:
        if tcl_path.exists() and tk_path.exists():
            os.environ.setdefault('TCL_LIBRARY', str(tcl_path))
            os.environ.setdefault('TK_LIBRARY', str(tk_path))
            return


_configure_tk_environment()

import tkinter as tk
from tkinter import messagebox, ttk

import cv2 as cv

from diablo2.common.capture import ScreenCapture, list_windows
from diablo2.common.config import BotConfig, load_config
from diablo2.actions.gem_summing import GemActionEvent, GemSummingSession
from diablo2.actions.loot_pickup import LootEvent, LootPickupSession
from diablo2.actions.recording import RecorderEvent, RecordingSession
from diablo2.actions.run_lifecycle import LifecycleEvent, RunLifecycleSession
from diablo2.runs.summoner_run import resolve_summoner_run


class D2BotControlPanel:
    def __init__(self, config_path: str | Path):
        self.config_path = Path(config_path)
        self.config: BotConfig = load_config(self.config_path)
        self.root = tk.Tk()
        self.root.title("Diablo2 Control Panel")
        self.root.geometry("1080x620")
        self.root.minsize(920, 560)

        self.window_title_var = tk.StringVar(value=self.config.capture.window_title or "Diablo")
        self.backend_var = tk.StringVar(value=self.config.capture.capture_backend)
        self.repeat_count_var = tk.StringVar()
        self.difficulty_var = tk.StringVar(value="hell")
        self.status_var = tk.StringVar(value="Idle")
        self.output_var = tk.StringVar(value="No recording yet")
        self.window_list_var = tk.StringVar()

        self.recording_session = RecordingSession(self.config.capture)
        self.lifecycle_session = RunLifecycleSession(self.config.capture)
        self.gem_session = GemSummingSession(self.config.capture)
        self.loot_session = LootPickupSession(self.config)

        self._build_ui()
        self._refresh_windows(select_current=True)
        self.root.protocol("WM_DELETE_WINDOW", self._on_close)
        self.root.after(250, self._poll_events)

    def _build_ui(self) -> None:
        self.root.columnconfigure(0, weight=1)
        self.root.rowconfigure(2, weight=1)

        header = ttk.Frame(self.root, padding=16)
        header.grid(row=0, column=0, sticky="ew")
        header.columnconfigure(1, weight=1)

        ttk.Label(header, text="Window Title").grid(row=0, column=0, sticky="w")
        self.window_title_box = ttk.Combobox(
            header,
            textvariable=self.window_title_var,
            state="readonly",
            values=(),
        )
        self.window_title_box.grid(row=0, column=1, sticky="ew", padx=(8, 12))
        self.window_title_box.bind("<<ComboboxSelected>>", self._use_selected_window)

        ttk.Label(header, text="Capture Backend").grid(row=0, column=2, sticky="w")
        backend_box = ttk.Combobox(
            header,
            textvariable=self.backend_var,
            state="readonly",
            values=("auto", "window", "screen"),
            width=10,
        )
        backend_box.grid(row=0, column=3, sticky="w", padx=(8, 0))

        recording_actions = ttk.Frame(self.root, padding=(16, 0, 16, 12))
        recording_actions.grid(row=1, column=0, sticky="ew")
        for idx in range(4):
            recording_actions.columnconfigure(idx, weight=1)

        self.record_button = ttk.Button(recording_actions, text="Start Recording", command=self.start_recording)
        self.record_button.grid(row=0, column=0, sticky="ew", padx=(0, 8))

        self.stop_button = ttk.Button(recording_actions, text="Stop Recording", command=self.stop_recording)
        self.stop_button.grid(row=0, column=1, sticky="ew", padx=8)
        self.stop_button.state(["disabled"])

        ttk.Button(recording_actions, text="Capture Snapshot", command=self.capture_snapshot).grid(
            row=0, column=2, sticky="ew", padx=8
        )

        ttk.Button(recording_actions, text="Refresh Windows", command=self._refresh_windows).grid(
            row=0, column=3, sticky="ew", padx=(8, 0)
        )

        main = ttk.Frame(self.root, padding=(16, 0, 16, 16))
        main.grid(row=2, column=0, sticky="nsew")
        main.columnconfigure(0, weight=1)
        main.columnconfigure(1, weight=0)
        main.rowconfigure(0, weight=1)

        self.log_panel = ttk.Labelframe(main, text="Log", padding=12)
        self.log_panel.grid(row=0, column=0, sticky="nsew", padx=(0, 16))
        self.log_panel.columnconfigure(0, weight=1)
        self.log_panel.rowconfigure(0, weight=1)

        self.log_text = tk.Text(self.log_panel, state="disabled", wrap="word")
        self.log_text.grid(row=0, column=0, sticky="nsew")

        play_panel = ttk.Labelframe(main, text="Play Controls", padding=12)
        play_panel.grid(row=0, column=1, sticky="ns")
        play_panel.columnconfigure(0, weight=1)

        self.gem_button = ttk.Button(play_panel, text="Start Gem Summing", command=self.start_gem_summing)
        self.gem_button.grid(row=0, column=0, sticky="ew")

        self.stop_action_button = ttk.Button(play_panel, text="Stop Action (F10)", command=self.stop_current_action)
        self.stop_action_button.grid(row=1, column=0, sticky="ew", pady=(8, 0))
        self.stop_action_button.state(["disabled"])

        repeat_box = ttk.Frame(play_panel, padding=(0, 18, 0, 18))
        repeat_box.grid(row=2, column=0, sticky="ew")
        repeat_box.columnconfigure(1, weight=1)
        ttk.Label(repeat_box, text="Repeat Count").grid(row=0, column=0, sticky="w")
        validate_digits = (self.root.register(self._validate_repeat_count), "%P")
        repeat_entry = ttk.Entry(
            repeat_box,
            textvariable=self.repeat_count_var,
            width=10,
            validate="key",
            validatecommand=validate_digits,
        )
        repeat_entry.grid(row=0, column=1, sticky="ew", padx=(10, 0))

        difficulty_box = ttk.Frame(play_panel)
        difficulty_box.grid(row=3, column=0, sticky="ew", pady=(0, 12))
        difficulty_box.columnconfigure(1, weight=1)

        ttk.Label(difficulty_box, text="Difficulty").grid(row=0, column=0, sticky="w")
        ttk.Separator(difficulty_box, orient="horizontal").grid(row=0, column=1, sticky="ew", padx=(8, 0))

        difficulty_options = ttk.Frame(difficulty_box, padding=(8, 8, 8, 6))
        difficulty_options.grid(row=1, column=0, columnspan=2, sticky="ew")
        ttk.Radiobutton(difficulty_options, text="\ubcf4\ud1b5", value="normal", variable=self.difficulty_var).grid(row=0, column=0, sticky="w")
        ttk.Radiobutton(difficulty_options, text="\uc545\ubabd", value="nightmare", variable=self.difficulty_var).grid(row=0, column=1, sticky="w", padx=(12, 0))
        ttk.Radiobutton(difficulty_options, text="\uc9c0\uc625", value="hell", variable=self.difficulty_var).grid(row=0, column=2, sticky="w", padx=(12, 0))
        ttk.Separator(difficulty_box, orient="horizontal").grid(row=2, column=0, columnspan=2, sticky="ew")

        self.key_button = ttk.Button(play_panel, text="Summoner Run", command=self.start_key_of_hate)
        self.key_button.grid(row=4, column=0, sticky="ew", pady=(8, 0))

        spacer = ttk.Frame(play_panel, height=18)
        spacer.grid(row=5, column=0, sticky="ew")

        self.loot_button = ttk.Button(play_panel, text="Item Looting", command=self.start_loot_pickup)
        self.loot_button.grid(row=6, column=0, sticky="ew")

        self.lifecycle_button = ttk.Button(play_panel, text="Start Room Lifecycle", command=self.start_run_lifecycle)
        self.lifecycle_button.grid(row=7, column=0, sticky="ew", pady=(8, 0))

        footer = ttk.Frame(self.root, padding=(16, 0, 16, 16))
        footer.grid(row=3, column=0, sticky="ew")
        footer.columnconfigure(1, weight=1)

        ttk.Label(footer, text="Status").grid(row=0, column=0, sticky="w")
        ttk.Label(footer, textvariable=self.status_var).grid(row=0, column=1, sticky="w", padx=(8, 0))
        ttk.Label(footer, text="Output").grid(row=1, column=0, sticky="w", pady=(8, 0))
        ttk.Label(footer, textvariable=self.output_var).grid(row=1, column=1, sticky="w", padx=(8, 0), pady=(8, 0))

    def _validate_repeat_count(self, proposed: str) -> bool:
        return proposed.isdigit() or proposed == ""

    def _parse_repeat_count(self) -> int | None:
        value = self.repeat_count_var.get().strip()
        if not value:
            return None
        return int(value)

    def _refresh_windows(self, select_current: bool = False) -> None:
        windows = list_windows()
        titles = [window.title for window in windows]
        self.window_list_var.set(titles)
        self.window_title_box.configure(values=titles)
        if not titles:
            self._append_log("warning", "No visible windows found.")
            return
        if select_current:
            current = self.window_title_var.get().casefold()
            for title in titles:
                if current and current in title.casefold():
                    self.window_title_var.set(title)
                    return
        current_value = self.window_title_var.get().strip()
        if not current_value and titles:
            self.window_title_var.set(titles[0])

    def _use_selected_window(self, event: object | None = None) -> None:
        value = self.window_title_var.get().strip()
        if not value:
            return
        self._append_log("info", f"Selected window: {value}")

    def _apply_runtime_config(self) -> None:
        self.config.capture.window_title = self.window_title_var.get().strip() or None
        self.config.capture.capture_backend = self.backend_var.get().strip() or "auto"
        self.recording_session.capture_config = self.config.capture
        self.lifecycle_session.capture_config = self.config.capture
        self.gem_session.capture_config = self.config.capture
        self.loot_session.update_config(self.config)

    def start_recording(self) -> None:
        self._apply_runtime_config()
        try:
            output_path = self.recording_session.start()
        except Exception as exc:
            messagebox.showerror("Recording Error", str(exc))
            self._append_log("error", str(exc))
            return
        self.status_var.set("Recording")
        self.output_var.set(str(output_path))
        self.record_button.state(["disabled"])
        self.stop_button.state(["!disabled"])

    def stop_recording(self) -> None:
        self.recording_session.stop()
        self.status_var.set("Idle")
        self.record_button.state(["!disabled"])
        self.stop_button.state(["disabled"])

    def start_run_lifecycle(self) -> None:
        self._apply_runtime_config()
        try:
            repeat_count = self._parse_repeat_count()
            self.lifecycle_session.start(repeat_count, self.difficulty_var.get().strip())
        except Exception as exc:
            messagebox.showerror("Run Lifecycle Error", str(exc))
            self._append_log("error", str(exc))
            return
        self.status_var.set("Run Lifecycle")
        self.lifecycle_button.state(["disabled"])
        self.loot_button.state(["disabled"])
        self.key_button.state(["disabled"])
        self.gem_button.state(["disabled"])
        self.stop_action_button.state(["!disabled"])

    def start_loot_pickup(self) -> None:
        self._apply_runtime_config()
        try:
            self.loot_session.start()
        except Exception as exc:
            messagebox.showerror("Item Looting Error", str(exc))
            self._append_log("error", str(exc))
            return
        self.status_var.set("Item Looting")
        self.lifecycle_button.state(["disabled"])
        self.loot_button.state(["disabled"])
        self.key_button.state(["disabled"])
        self.gem_button.state(["disabled"])
        self.stop_action_button.state(["!disabled"])

    def start_key_of_hate(self) -> None:
        self._append_log("info", "Summoner Run is ready in the UI, but the hunting loop is not implemented yet. Use Start Room Lifecycle to test create-exit-remake first.")
        messagebox.showinfo(
            "Summoner Run",
            "Summoner hunting is not implemented yet.\n\nStart with Room Lifecycle to test room create, exit, and remake loops.",
        )

    def start_gem_summing(self) -> None:
        self._apply_runtime_config()
        try:
            self.gem_session.start()
        except Exception as exc:
            messagebox.showerror("Gem Summing Error", str(exc))
            self._append_log("error", str(exc))
            return
        self.status_var.set("Gem Summing")
        self.lifecycle_button.state(["disabled"])
        self.loot_button.state(["disabled"])
        self.key_button.state(["disabled"])
        self.gem_button.state(["disabled"])
        self.stop_action_button.state(["!disabled"])

    def stop_current_action(self) -> None:
        if self.lifecycle_session.is_running:
            self.lifecycle_session.stop()
        if self.gem_session.is_running:
            self.gem_session.stop()
        if self.loot_session.is_running:
            self.loot_session.stop()
        self.status_var.set("Idle")
        self.lifecycle_button.state(["!disabled"])
        self.loot_button.state(["!disabled"])
        self.key_button.state(["!disabled"])
        self.gem_button.state(["!disabled"])
        self.stop_action_button.state(["disabled"])

    def capture_snapshot(self) -> None:
        self._apply_runtime_config()
        snapshots_dir = Path("assets") / "private"
        snapshots_dir.mkdir(parents=True, exist_ok=True)
        snapshot_path = snapshots_dir / "gui_snapshot.png"
        try:
            packet = ScreenCapture(self.config.capture).grab()
            success = cv.imwrite(str(snapshot_path), packet.frame)
        except Exception as exc:
            messagebox.showerror("Snapshot Error", str(exc))
            self._append_log("error", f"Snapshot failed: {exc}")
            return

        if not success:
            messagebox.showerror("Snapshot Error", "Failed to save snapshot.")
            self._append_log("error", "Snapshot failed: image save returned false.")
            return

        self.output_var.set(str(snapshot_path))
        self._append_log("info", f"Snapshot saved: {snapshot_path.name}")

    def _append_log(self, level: str, message: str) -> None:
        self.log_text.configure(state="normal")
        self.log_text.insert("end", f"[{level.upper()}] {message}\n")
        self.log_text.see("end")
        self.log_text.configure(state="disabled")

    def _refresh_action_controls(self) -> None:
        action_running = self.lifecycle_session.is_running or self.gem_session.is_running or self.loot_session.is_running
        if action_running:
            self.lifecycle_button.state(["disabled"])
            self.loot_button.state(["disabled"])
            self.key_button.state(["disabled"])
            self.gem_button.state(["disabled"])
            self.stop_action_button.state(["!disabled"])
            return
        self.lifecycle_button.state(["!disabled"])
        self.loot_button.state(["!disabled"])
        self.key_button.state(["!disabled"])
        self.gem_button.state(["!disabled"])
        self.stop_action_button.state(["disabled"])
        if self.recording_session.is_running:
            self.status_var.set("Recording")
        else:
            self.status_var.set("Idle")

    def _poll_events(self) -> None:
        while not self.recording_session.events.empty():
            event: RecorderEvent = self.recording_session.events.get_nowait()
            self._append_log(event.level, event.message)
            if event.level == "error":
                self.status_var.set("Error")
                self.record_button.state(["!disabled"])
                self.stop_button.state(["disabled"])
        while not self.lifecycle_session.events.empty():
            event: LifecycleEvent = self.lifecycle_session.events.get_nowait()
            self._append_log(event.level, event.message)
            if event.level == "error":
                self.status_var.set("Error")
        while not self.gem_session.events.empty():
            event: GemActionEvent = self.gem_session.events.get_nowait()
            self._append_log(event.level, event.message)
        while not self.loot_session.events.empty():
            event: LootEvent = self.loot_session.events.get_nowait()
            self._append_log(event.level, event.message)
            if event.level == "error":
                self.status_var.set("Error")
        self._refresh_action_controls()
        self.root.after(250, self._poll_events)

    def _on_close(self) -> None:
        if self.recording_session.is_running:
            self.recording_session.stop()
        if self.lifecycle_session.is_running:
            self.lifecycle_session.stop()
        if self.gem_session.is_running:
            self.gem_session.stop()
        if self.loot_session.is_running:
            self.loot_session.stop()
        self.root.destroy()

    def run(self) -> int:
        self.root.mainloop()
        return 0


def run_gui(config_path: str | Path = "config") -> int:
    panel = D2BotControlPanel(config_path)
    return panel.run()
