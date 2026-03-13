from __future__ import annotations

import logging
import time

import cv2 as cv

from d2bot.capture import ScreenCapture, SessionRecorder
from d2bot.config import BotConfig
from d2bot.controller import BotController
from d2bot.detectors import TemplateMatcher, draw_overlay


class DiabloBot:
    def __init__(self, config: BotConfig):
        self.config = config
        self.log = logging.getLogger("d2bot.bot")
        self.capture = ScreenCapture(config.capture)
        self.controller = BotController(dry_run=config.dry_run)
        self.matcher = TemplateMatcher(config.farm.templates)
        self.state = "watching"

    def _status_text(self) -> str:
        mode = "DRY-RUN" if self.config.dry_run else "LIVE"
        pause_state = "PAUSED" if self.controller.paused else "RUNNING"
        return f"{self.config.farm.name} | {self.state} | {mode} | {pause_state}"

    def _apply_matches(self, matches) -> None:
        for match in matches:
            self.log.info("Matched %s at %.3f -> %s", match.name, match.confidence, match.action)
            if self.controller.paused:
                return
            if match.action == "click_center":
                center_x = (match.top_left[0] + match.bottom_right[0]) // 2
                center_y = (match.top_left[1] + match.bottom_right[1]) // 2
                absolute_x = self.capture.target["left"] + center_x
                absolute_y = self.capture.target["top"] + center_y
                self.controller.click(absolute_x, absolute_y)

    def run(self) -> int:
        first_packet = self.capture.grab()
        height, width = first_packet.frame.shape[:2]
        recorder = SessionRecorder(self.config.recording, (width, height))
        recorder.start()
        self.controller.bind_hotkeys(
            self.config.hotkeys.pause,
            self.config.hotkeys.stop,
        )

        frame_delay = 1.0 / max(1, self.config.capture.fps)
        self.log.info("Watching monitor region: %s", self.capture.target)
        self.log.info("Farm goal: %s", self.config.farm.goal)
        self.log.info("Loot whitelist: %s", ", ".join(self.config.farm.loot_whitelist))

        try:
            while not self.controller.stop_requested:
                started = time.time()
                packet = self.capture.grab()
                matches = self.matcher.scan(packet.frame)
                self._apply_matches(matches)

                display_frame = packet.frame
                if self.config.overlay:
                    display_frame = draw_overlay(display_frame, matches, self._status_text())
                    scaled = cv.resize(
                        display_frame,
                        None,
                        fx=self.config.capture.preview_scale,
                        fy=self.config.capture.preview_scale,
                    )
                    cv.imshow("d2bot preview", scaled)
                    if cv.waitKey(1) & 0xFF == 27:
                        self.controller.request_stop()

                recorder.write(display_frame)
                elapsed = time.time() - started
                time.sleep(max(0.0, frame_delay - elapsed))
        finally:
            recorder.close()
            cv.destroyAllWindows()

        return 0
