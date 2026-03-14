from __future__ import annotations

import logging
import time

import cv2 as cv

from diablo2.common.capture import ScreenCapture, SessionRecorder
from diablo2.common.config import BotConfig
from diablo2.common.controller import BotController
from diablo2.common.detectors import TemplateMatcher, draw_overlay


class DiabloBot:
    def __init__(self, config: BotConfig):
        self.config = config
        self.log = logging.getLogger("diablo2.bot")
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
        self.log.info("Loaded farm profile: %s", self.config.farm.name)
        self.log.info("Farm goal: %s", self.config.farm.goal)
        self.log.info("Hunting objective: %s", self.config.farm.hunting.objective)
        fixed_labels = [item.label for item in self.config.shared_loot.fixed_items] + self.config.farm.loot.keep_labels
        self.log.info("Loot keep labels: %s", ", ".join(fixed_labels))
        self.log.info("Run-specific rules: %s", self.config.farm.run_specific_rules)

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
                    cv.imshow("Diablo2 Preview", scaled)
                    if cv.waitKey(1) & 0xFF == 27:
                        self.controller.request_stop()

                recorder.write(display_frame)
                elapsed = time.time() - started
                time.sleep(max(0.0, frame_delay - elapsed))
        finally:
            recorder.close()
            cv.destroyAllWindows()

        return 0

