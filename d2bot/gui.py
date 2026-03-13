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

from d2bot.capture import ScreenCapture, list_windows
from d2bot.config import BotConfig, load_config
from d2bot.recording import RecorderEvent, RecordingSession


class D2BotControlPanel:
    def __init__(self, config_path: str | Path):
        self.config_path = Path(config_path)
        self.config: BotConfig = load_config(self.config_path)
        self.root = tk.Tk()
        self.root.title("d2bot Control Panel")
        self.root.geometry("760x520")
        self.root.minsize(700, 480)

        self.window_title_var = tk.StringVar(value=self.config.capture.window_title or "Diablo")
        self.backend_var = tk.StringVar(value=self.config.capture.capture_backend)
        self.status_var = tk.StringVar(value="Idle")
        self.output_var = tk.StringVar(value="No recording yet")
        self.window_list_var = tk.StringVar()

        self.recording_session = RecordingSession(self.config.capture)

        self._build_ui()
        self._refresh_windows(select_current=True)
        self.root.protocol("WM_DELETE_WINDOW", self._on_close)
        self.root.after(250, self._poll_recorder_events)

    def _build_ui(self) -> None:
        self.root.columnconfigure(0, weight=1)
        self.root.rowconfigure(2, weight=1)

        header = ttk.Frame(self.root, padding=16)
        header.grid(row=0, column=0, sticky="ew")
        header.columnconfigure(1, weight=1)

        ttk.Label(header, text="Window Title").grid(row=0, column=0, sticky="w")
        title_entry = ttk.Entry(header, textvariable=self.window_title_var)
        title_entry.grid(row=0, column=1, sticky="ew", padx=(8, 12))

        ttk.Label(header, text="Capture Backend").grid(row=0, column=2, sticky="w")
        backend_box = ttk.Combobox(
            header,
            textvariable=self.backend_var,
            state="readonly",
            values=("auto", "window", "screen"),
            width=10,
        )
        backend_box.grid(row=0, column=3, sticky="w")

        actions = ttk.Frame(self.root, padding=(16, 0, 16, 12))
        actions.grid(row=1, column=0, sticky="ew")
        for idx in range(4):
            actions.columnconfigure(idx, weight=1)

        self.record_button = ttk.Button(actions, text="Start Recording", command=self.start_recording)
        self.record_button.grid(row=0, column=0, sticky="ew", padx=(0, 8))

        self.stop_button = ttk.Button(actions, text="Stop Recording", command=self.stop_recording)
        self.stop_button.grid(row=0, column=1, sticky="ew", padx=8)
        self.stop_button.state(["disabled"])

        ttk.Button(actions, text="Capture Snapshot", command=self.capture_snapshot).grid(
            row=0, column=2, sticky="ew", padx=8
        )
        ttk.Button(actions, text="Refresh Windows", command=self._refresh_windows).grid(
            row=0, column=3, sticky="ew", padx=(8, 0)
        )

        content = ttk.Panedwindow(self.root, orient=tk.HORIZONTAL)
        content.grid(row=2, column=0, sticky="nsew", padx=16, pady=(0, 16))

        left = ttk.Labelframe(content, text="Visible Windows", padding=12)
        right = ttk.Labelframe(content, text="Recorder Log", padding=12)
        content.add(left, weight=1)
        content.add(right, weight=1)

        left.columnconfigure(0, weight=1)
        left.rowconfigure(0, weight=1)
        self.window_listbox = tk.Listbox(left, listvariable=self.window_list_var, height=14)
        self.window_listbox.grid(row=0, column=0, sticky="nsew")
        self.window_listbox.bind("<<ListboxSelect>>", self._use_selected_window)

        right.columnconfigure(0, weight=1)
        right.rowconfigure(0, weight=1)
        self.log_text = tk.Text(right, state="disabled", wrap="word")
        self.log_text.grid(row=0, column=0, sticky="nsew")

        footer = ttk.Frame(self.root, padding=(16, 0, 16, 16))
        footer.grid(row=3, column=0, sticky="ew")
        footer.columnconfigure(1, weight=1)

        ttk.Label(footer, text="Status").grid(row=0, column=0, sticky="w")
        ttk.Label(footer, textvariable=self.status_var).grid(row=0, column=1, sticky="w", padx=(8, 0))
        ttk.Label(footer, text="Output").grid(row=1, column=0, sticky="w", pady=(8, 0))
        ttk.Label(footer, textvariable=self.output_var).grid(row=1, column=1, sticky="w", padx=(8, 0), pady=(8, 0))

    def _refresh_windows(self, select_current: bool = False) -> None:
        windows = list_windows()
        titles = [window.title for window in windows]
        self.window_list_var.set(titles)
        if not titles:
            self._append_log("warning", "No visible windows found.")
            return
        if select_current:
            current = self.window_title_var.get().casefold()
            for index, title in enumerate(titles):
                if current and current in title.casefold():
                    self.window_listbox.selection_clear(0, tk.END)
                    self.window_listbox.selection_set(index)
                    self.window_listbox.activate(index)
                    break

    def _use_selected_window(self, event: object | None = None) -> None:
        selection = self.window_listbox.curselection()
        if not selection:
            return
        value = self.window_listbox.get(selection[0])
        self.window_title_var.set(value)
        self._append_log("info", f"Selected window: {value}")

    def _apply_runtime_config(self) -> None:
        self.config.capture.window_title = self.window_title_var.get().strip() or None
        self.config.capture.capture_backend = self.backend_var.get().strip() or "auto"
        self.recording_session.capture_config = self.config.capture

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

    def _poll_recorder_events(self) -> None:
        while not self.recording_session.events.empty():
            event: RecorderEvent = self.recording_session.events.get_nowait()
            self._append_log(event.level, event.message)
            if event.level == "error":
                self.status_var.set("Error")
                self.record_button.state(["!disabled"])
                self.stop_button.state(["disabled"])
        self.root.after(250, self._poll_recorder_events)

    def _on_close(self) -> None:
        if self.recording_session.is_running:
            self.recording_session.stop()
        self.root.destroy()

    def run(self) -> int:
        self.root.mainloop()
        return 0


def run_gui(config_path: str | Path = "config.example.json") -> int:
    panel = D2BotControlPanel(config_path)
    return panel.run()
