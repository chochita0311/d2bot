from __future__ import annotations

import ctypes
import time
from dataclasses import dataclass
from ctypes import wintypes
from pathlib import Path

import cv2 as cv
import mss
import numpy as np

from d2bot.config import CaptureConfig, RecordingConfig


SRCCOPY = 0x00CC0020
DIB_RGB_COLORS = 0
PW_RENDERFULLCONTENT = 0x00000002
BI_RGB = 0
USER32 = ctypes.windll.user32
GDI32 = ctypes.windll.gdi32


class BITMAPINFOHEADER(ctypes.Structure):
    _fields_ = [
        ("biSize", wintypes.DWORD),
        ("biWidth", wintypes.LONG),
        ("biHeight", wintypes.LONG),
        ("biPlanes", wintypes.WORD),
        ("biBitCount", wintypes.WORD),
        ("biCompression", wintypes.DWORD),
        ("biSizeImage", wintypes.DWORD),
        ("biXPelsPerMeter", wintypes.LONG),
        ("biYPelsPerMeter", wintypes.LONG),
        ("biClrUsed", wintypes.DWORD),
        ("biClrImportant", wintypes.DWORD),
    ]


class BITMAPINFO(ctypes.Structure):
    _fields_ = [
        ("bmiHeader", BITMAPINFOHEADER),
        ("bmiColors", wintypes.DWORD * 3),
    ]


@dataclass
class WindowInfo:
    handle: int
    title: str
    left: int
    top: int
    width: int
    height: int


@dataclass
class FramePacket:
    frame: np.ndarray
    timestamp: float


class ScreenCapture:
    def __init__(self, config: CaptureConfig):
        self.config = config
        self._sct = mss.mss()
        self._target = self._resolve_target()
        self._window: WindowInfo | None = None
        if self.config.window_title:
            self._window = self._resolve_window()

    def _resolve_window(self) -> WindowInfo:
        window = find_window(self.config.window_title, self.config.window_title_mode)
        if window is None:
            raise RuntimeError(
                f"Could not find window with title "
                f"{self.config.window_title_mode} '{self.config.window_title}'."
            )
        return window

    def _resolve_target(self) -> dict[str, int]:
        if self.config.window_title:
            window = find_window(self.config.window_title, self.config.window_title_mode)
            if window is None:
                raise RuntimeError(
                    f"Could not find window with title "
                    f"{self.config.window_title_mode} '{self.config.window_title}'."
                )
            return {
                "left": window.left,
                "top": window.top,
                "width": window.width,
                "height": window.height,
            }
        if self.config.region:
            return self.config.region
        return dict(self._sct.monitors[self.config.monitor_index])

    @property
    def target(self) -> dict[str, int]:
        return self._target

    def grab(self) -> FramePacket:
        if self.config.window_title and self.config.follow_window:
            self._window = self._resolve_window()
            self._target = {
                "left": self._window.left,
                "top": self._window.top,
                "width": self._window.width,
                "height": self._window.height,
            }

        frame = None
        if self._window and self.config.capture_backend in {"auto", "window"}:
            frame = capture_window_image(self._window.handle, self._window.width, self._window.height)
            if frame is None and self.config.capture_backend == "window":
                raise RuntimeError("Named window capture failed for the target window.")

        if frame is None:
            shot = np.array(self._sct.grab(self._target))
            frame = cv.cvtColor(shot, cv.COLOR_BGRA2BGR)

        return FramePacket(frame=frame, timestamp=time.time())


class SessionRecorder:
    def __init__(self, config: RecordingConfig, frame_size: tuple[int, int]):
        self.config = config
        self.frame_size = frame_size
        self._writer: cv.VideoWriter | None = None

    def start(self) -> None:
        if not self.config.enabled:
            return
        width, height = self.frame_size
        output_path = Path(self.config.output_path)
        output_path.parent.mkdir(parents=True, exist_ok=True)
        fourcc = cv.VideoWriter_fourcc(*self.config.codec)
        self._writer = cv.VideoWriter(
            str(output_path),
            fourcc,
            8.0,
            (width, height),
        )

    def write(self, frame: np.ndarray) -> None:
        if self._writer is not None:
            self._writer.write(frame)

    def close(self) -> None:
        if self._writer is not None:
            self._writer.release()
            self._writer = None


def capture_window_image(hwnd: int, width: int, height: int) -> np.ndarray | None:
    hwnd_dc = USER32.GetWindowDC(hwnd)
    if not hwnd_dc:
        return None

    mem_dc = GDI32.CreateCompatibleDC(hwnd_dc)
    bitmap = GDI32.CreateCompatibleBitmap(hwnd_dc, width, height)
    if not mem_dc or not bitmap:
        _release_capture_objects(hwnd, hwnd_dc, mem_dc, bitmap, None)
        return None

    old_obj = GDI32.SelectObject(mem_dc, bitmap)
    success = USER32.PrintWindow(hwnd, mem_dc, PW_RENDERFULLCONTENT)
    if not success:
        success = GDI32.BitBlt(mem_dc, 0, 0, width, height, hwnd_dc, 0, 0, SRCCOPY)

    frame = None
    if success:
        bitmap_info = BITMAPINFO()
        bitmap_info.bmiHeader.biSize = ctypes.sizeof(BITMAPINFOHEADER)
        bitmap_info.bmiHeader.biWidth = width
        bitmap_info.bmiHeader.biHeight = -height
        bitmap_info.bmiHeader.biPlanes = 1
        bitmap_info.bmiHeader.biBitCount = 32
        bitmap_info.bmiHeader.biCompression = BI_RGB

        buffer_len = width * height * 4
        buffer = ctypes.create_string_buffer(buffer_len)
        rows = GDI32.GetDIBits(
            mem_dc,
            bitmap,
            0,
            height,
            buffer,
            ctypes.byref(bitmap_info),
            DIB_RGB_COLORS,
        )
        if rows == height:
            bgra = np.frombuffer(buffer, dtype=np.uint8).reshape((height, width, 4))
            frame = cv.cvtColor(bgra, cv.COLOR_BGRA2BGR)
            if int(frame.max()) == 0:
                frame = None

    _release_capture_objects(hwnd, hwnd_dc, mem_dc, bitmap, old_obj)
    return frame


def _release_capture_objects(hwnd: int, hwnd_dc: int, mem_dc: int, bitmap: int, old_obj: int | None) -> None:
    if mem_dc and old_obj:
        GDI32.SelectObject(mem_dc, old_obj)
    if bitmap:
        GDI32.DeleteObject(bitmap)
    if mem_dc:
        GDI32.DeleteDC(mem_dc)
    if hwnd_dc:
        USER32.ReleaseDC(hwnd, hwnd_dc)


def list_windows() -> list[WindowInfo]:
    windows: list[WindowInfo] = []

    @ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, wintypes.LPARAM)
    def enum_proc(hwnd: int, lparam: int) -> bool:
        if not USER32.IsWindowVisible(hwnd):
            return True
        length = USER32.GetWindowTextLengthW(hwnd)
        if length == 0:
            return True

        title_buffer = ctypes.create_unicode_buffer(length + 1)
        USER32.GetWindowTextW(hwnd, title_buffer, length + 1)
        title = title_buffer.value.strip()
        if not title:
            return True

        rect = wintypes.RECT()
        USER32.GetWindowRect(hwnd, ctypes.byref(rect))
        width = rect.right - rect.left
        height = rect.bottom - rect.top
        if width <= 0 or height <= 0:
            return True

        windows.append(
            WindowInfo(
                handle=int(hwnd),
                title=title,
                left=rect.left,
                top=rect.top,
                width=width,
                height=height,
            )
        )
        return True

    USER32.EnumWindows(enum_proc, 0)
    return windows


def find_window(title: str, mode: str = "contains") -> WindowInfo | None:
    title_casefold = title.casefold()
    for window in list_windows():
        current_title = window.title.casefold()
        if mode == "exact" and current_title == title_casefold:
            return window
        if mode != "exact" and title_casefold in current_title:
            return window
    return None
