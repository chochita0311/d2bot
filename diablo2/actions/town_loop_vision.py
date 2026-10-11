"""Screen-only, one-input-at-a-time planner for a bounded town round trip.

This module does not send input. Both the desktop session and supervised
computer-use verification consume the same decisions. Shared calibration
images are owned by assets/ui/town-loop (see the feature documentation).
"""
from __future__ import annotations

import argparse
import hashlib
import json
import time
from pathlib import Path

import cv2 as cv
import numpy as np

from diablo2.common.scene_motion import BackgroundMotionObserver


REFERENCE_SIZE = (1267, 753)
ASSET_DIRECTORY = Path("assets/ui/town-loop/remastered-ko-window")


def calibrate(evidence: Path, directory: Path = ASSET_DIRECTORY) -> None:
    """기준 화면에서 공용 마을/메뉴 템플릿 은행을 재생성한다.

    Coordinates refer to window pixels including the title bar. A different
    layout/language requires reviewed crops, not automatic template adaptation.
    """
    definitions = {
        "play": ("01-character-select.png", [472, 665, 580, 690], [400, 620, 850, 720], 0.84),
        "difficulty_header": ("02-difficulty.png", [589, 278, 678, 297], [520, 255, 735, 320], 0.88),
        "normal": ("02-difficulty.png", [566, 323, 700, 346], [545, 305, 721, 358], 0.86),
        "nightmare": ("02-difficulty.png", [566, 367, 700, 390], [545, 357, 721, 402], 0.86),
        "hell": ("02-difficulty.png", [566, 411, 700, 434], [545, 399, 721, 450], 0.86),
        "hud": ("03-act1-spawn.png", [616, 703, 650, 746], [605, 690, 665, 750], 0.86),
        "area_act1": ("04-act1-map.png", [1168, 70, 1249, 85], [1120, 64, 1257, 93], 0.79),
        "area_act2": ("09-act2-town.png", [1181, 70, 1249, 85], [1120, 64, 1257, 93], 0.79),
        "waypoint_header": ("07-waypoint-act1.png", [230, 123, 315, 144], [190, 110, 355, 152], 0.88),
        "list_act1": ("07-waypoint-act1.png", [198, 240, 277, 306], [190, 234, 290, 312], 0.82),
        "list_act2": ("08-waypoint-act2.png", [198, 240, 277, 306], [190, 234, 290, 312], 0.82),
        "world_act1_waypoint": ("05-act1-waypoint-world.png", [1122, 282, 1211, 322], [420, 110, 1256, 570], 0.72),
        "save_exit": ("11-save-exit-menu.png", [550, 336, 715, 361], [500, 310, 770, 390], 0.86),
    }
    directory.mkdir(parents=True, exist_ok=True)
    manifest_path = directory / "manifest.json"
    previous = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.exists() else {"templates": {}}
    manifest = {"reference_size": list(REFERENCE_SIZE), "source": evidence.as_posix(), "templates": {}}
    for name, (filename, crop, search, threshold) in definitions.items():
        source = cv.imdecode(np.fromfile(str(evidence / "frames/raw" / filename), dtype=np.uint8), cv.IMREAD_COLOR)
        if source is None or (source.shape[1], source.shape[0]) != REFERENCE_SIZE:
            raise ValueError(f"Expected baseline {REFERENCE_SIZE}: {filename}")
        x1, y1, x2, y2 = crop
        asset_filename = previous["templates"].get(name, {}).get("file", f"templates/{name.replace('_', '-')}-ko-{x2 - x1}x{y2 - y1}.png")
        asset_path = directory / asset_filename
        asset_path.parent.mkdir(parents=True, exist_ok=True)
        encoded = cv.imencode(".png", source[y1:y2, x1:x2])[1]
        encoded.tofile(str(asset_path))
        manifest["templates"][name] = {
            "file": asset_filename,
            "crop": crop,
            "search": search,
            "threshold": threshold,
            "source_frame": filename,
            "asset_sha256": hashlib.sha256(encoded.tobytes()).hexdigest(),
        }
    manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")


class TownLoopVision:
    def __init__(self, directory: Path = ASSET_DIRECTORY):
        from diablo2.common.automap_vision import AutomapObserver

        self.automap = AutomapObserver()
        self.templates = {}
        manifest_path = directory / "manifest.json"
        if not manifest_path.exists():
            raise RuntimeError("Shared town-loop calibration is missing. See assets/ui/town-loop/remastered-ko-window/manifest.json.")
        self.manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        for name, definition in self.manifest["templates"].items():
            image = cv.imdecode(np.fromfile(str(directory / definition["file"]), dtype=np.uint8), cv.IMREAD_GRAYSCALE)
            if image is None:
                raise RuntimeError(f"Town loop calibration image is unreadable: {name}")
            self.templates[name] = image
        self.variants = {}
        for name, definition in self.manifest["templates"].items():
            self.variants[name] = [self.templates[name]]
            for filename in definition.get("variants", []):
                image = cv.imdecode(np.fromfile(str(directory / filename), dtype=np.uint8), cv.IMREAD_GRAYSCALE)
                if image is None:
                    raise RuntimeError(f"Town loop calibration variant is unreadable: {filename}")
                self.variants[name].append(image)
        self.motion = BackgroundMotionObserver(minimum_matches=12, minimum_inlier_ratio=0.65, maximum_error=3)
        self.reset_camera()

    def reset_camera(self):
        self.motion.reset()
        self.camera_sequence, self.camera_stable_frames, self.camera_stable_since = 0, 0, None
        self.camera_reading = None

    def camera_stable(self, frame, now):
        # HUD·우측 지도 제목·중앙 캐릭터를 제외한 실제 배경 대응을 확인한다.
        height, width = frame.shape[:2]
        mask = np.zeros((height, width), np.uint8)
        mask[int(height * 0.12) : int(height * 0.76), int(width * 0.08) : int(width * 0.85)] = 255
        mask[int(height * 0.40) : int(height * 0.60), int(width * 0.43) : int(width * 0.57)] = 0
        self.camera_sequence += 1
        reading = self.motion.observe_mask(frame, mask, sequence=self.camera_sequence, captured_at=now)
        self.camera_reading = {"displacement": reading.displacement, "inliers": reading.inliers, "reason": reading.reason}
        if reading.displacement is None or np.linalg.norm(reading.displacement) >= 2:
            self.camera_stable_frames, self.camera_stable_since = 0, None
            if reading.displacement is None:
                # 마을 정지는 연속 화면으로 확인한다. 멀어진 첫 화면에 계속 묶이지 않는다.
                self.motion.reset()
                self.motion.observe_mask(frame, mask, sequence=self.camera_sequence, captured_at=now)
            return False
        self.camera_stable_since = now if self.camera_stable_since is None else self.camera_stable_since
        self.camera_stable_frames += 1
        return self.camera_stable_frames >= 3 and now - self.camera_stable_since >= 0.12

    def normalize(self, frame: np.ndarray) -> np.ndarray:
        height, width = frame.shape[:2]
        expected_ratio = REFERENCE_SIZE[0] / REFERENCE_SIZE[1]
        if width < 800 or height < 450 or abs(width / height / expected_ratio - 1) > 0.03:
            raise RuntimeError(f"Uncalibrated window shape: {width}x{height}; stop and recalibrate instead of guessing.")
        self.color = cv.resize(frame, REFERENCE_SIZE, interpolation=cv.INTER_AREA)
        return cv.cvtColor(self.color, cv.COLOR_BGR2GRAY)

    def find(self, gray: np.ndarray, name: str):
        if name not in self.manifest["templates"]:
            return None
        if name == "area_arcane" and self.automap.visible(self.color, "arcane_sanctuary"):
            # 회색 지도 선이 제목을 통과해도 공통 금색 제목 판독을 재사용한다.
            at = self.automap.last_evidence["title_at"]
            return dict(x=at[0] + 34, y=at[1] + 7, score=self.automap.last_evidence["score"])
        definition = self.manifest["templates"][name]
        if definition.get("blue_pair"):
            return self.find_blue_pair(definition["blue_pair"])
        x1, y1, x2, y2 = definition["search"]
        region = gray[y1:y2, x1:x2]
        best = None
        foreground = (
            lambda patch: cv.subtract(patch, cv.GaussianBlur(patch, (0, 0), 5)) if definition.get("foreground") == "high_pass" else patch
        )
        region = foreground(region)
        region = cv.GaussianBlur(region, (3, 3), 0.8)
        for index, template in enumerate(self.variants[name]):
            if region.shape[0] < template.shape[0] or region.shape[1] < template.shape[1]:
                continue
            scores = cv.matchTemplate(region, cv.GaussianBlur(foreground(template), (3, 3), 0.8), cv.TM_CCOEFF_NORMED)
            _, score, _, location = cv.minMaxLoc(scores)
            if score < definition.get("threshold", 0.88) or (best is not None and best["score"] >= score):
                continue
            filename = definition["file"] if index == 0 else definition["variants"][index - 1]
            offset = definition.get("variant_click_offsets", {}).get(
                filename, definition.get("click_offset", [template.shape[1] / 2, template.shape[0] / 2])
            )
            px, py = x1 + location[0] + offset[0], y1 + location[1] + offset[1]
            if definition.get("blue_nearby"):
                rx, ry = definition["blue_nearby"]
                region_color = self.color[max(0, int(py - ry)) : int(py + ry), max(0, int(px - rx)) : int(px + rx)]
                blue = cv.inRange(cv.cvtColor(region_color, cv.COLOR_BGR2HSV), np.array([95, 55, 120]), np.array([135, 255, 255]))
                if cv.countNonZero(blue) < 40:
                    continue
            best = {"x": px, "y": py, "score": float(score)}
        return best

    def find_blue_pair(self, definition):
        x1, y1, x2, y2 = definition["search"]
        hsv = cv.cvtColor(self.color[y1:y2, x1:x2], cv.COLOR_BGR2HSV)
        mask = cv.inRange(hsv, np.array(definition["hsv_lower"]), np.array(definition["hsv_upper"]))
        mask = cv.morphologyEx(mask, cv.MORPH_CLOSE, np.ones((7, 7), np.uint8))
        _, _, stats, centers = cv.connectedComponentsWithStats(mask)
        components = [
            center
            for (_, _, width, height, area), center in zip(stats[1:], centers[1:])
            if definition["minimum_area"] <= area
            and definition["width"][0] <= width <= definition["width"][1]
            and definition["height"][0] <= height <= definition["height"][1]
            and width / height <= definition["maximum_aspect_ratio"]
        ]
        pairs = [
            (left + right) / 2
            for i, left in enumerate(components)
            for right in components[i + 1 :]
            if definition["separation"][0] <= abs(left[0] - right[0]) <= definition["separation"][1]
            and abs(left[1] - right[1]) <= definition["vertical_difference"]
        ]
        if len(pairs) != 1:
            return None
        x, y = pairs[0] + definition["click_offset"]
        return {"x": x1 + float(x), "y": y1 + float(y), "evidence": "paired_blue_braziers"}


class TownLoopPlanner:
    """Fail closed when a state cannot be positively identified.

    A returned action is consumed once. Its phase is advanced before dispatch;
    the next observation must confirm the postcondition, including after an
    uncertain input outcome. There is no generic blind retry loop.
    """

    def __init__(
        self,
        vision: TownLoopVision,
        repeat_count: int = 1,
        difficulty: str = "hell",
        destination: str = "town",
        *,
        arrival_only: bool = False,
        reset_act2_start: bool = False,
    ):
        if not isinstance(repeat_count, int) or isinstance(repeat_count, bool) or not 1 <= repeat_count <= 3:
            raise ValueError("Supervised town loop requires an explicit repeat count from 1 to 3.")
        if difficulty not in {"normal", "nightmare", "hell"}:
            raise ValueError("Unknown difficulty.")
        if destination not in {"town", "arcane"}:
            raise ValueError("Unknown waypoint destination.")
        if type(arrival_only) is not bool or (arrival_only and (destination != "arcane" or repeat_count != 1)):
            raise ValueError("Arrival-only entry requires exactly one Arcane visit.")
        if type(reset_act2_start) is not bool or (reset_act2_start and difficulty != "hell"):
            raise ValueError("Act 2 start reset requires Hell as the target difficulty.")
        if destination == "arcane" and any(name not in vision.templates for name in ("area_arcane", "world_arcane_waypoint")):
            raise ValueError("Arcane arrival has not been calibrated; use the town destination first.")
        self.vision = vision
        self.data = {
            "phase": "character",
            "repeat_count": repeat_count,
            "difficulty": difficulty,
            "destination": destination,
            "arrival_only": arrival_only,
            "completed": 0,
            "started": None,
            "deadline": None,
            "ready_at": 0.0,
            "probes": 0,
            "waypoint_attempts": 0,
            "return_attempts": 0,
            "map_toggled": False,
            "action_number": 0,
            "holding": False,
            "move_until": 0,
            "npc_recoveries": 0,
            "town_layout": None,
            "reset_act2_start": reset_act2_start,
            "difficulty_reset_used": False,
            "reset_stage": None,
            "preparation_rooms": 0,
        }

    def restore(self, data: dict) -> None:
        self.data.update(data)

    def _transition(self, phase: str, now: float, timeout: float = 25.0, settle: float = 0.7) -> None:
        self.data.update(phase=phase, deadline=now + timeout, ready_at=now + settle)

    def _action(self, phase: str, now: float, reason: str, point=None, key=None, settle=0.12, kind=None):
        self._transition(phase, now, settle=settle)
        self.data["action_number"] += 1
        result = {"kind": kind or ("key" if key else "click"), "reason": reason, "phase": phase, "number": self.data["action_number"]}
        if key:
            result["key"] = key
        elif point is not None:
            result["point"] = [int(round(point[0])), int(round(point[1]))]
        return result

    def _move(self, now, reason, point, duration=2.4, phase="act1_moving"):
        self.data.update(holding=True, move_until=now + duration)
        return self._action(phase, now, reason, point, kind="hold", settle=0)

    def _release(self, now, reason, phase="act1_waypoint"):
        self.data["holding"] = False
        self.data.update(camera_settling=True, camera_deadline=now + 4)
        self.vision.reset_camera()
        return self._action(phase, now, reason, kind="release", settle=0.08)

    def step(self, frame: np.ndarray, now: float | None = None):
        now = time.time() if now is None else now
        data = self.data
        if data["phase"] == "done":
            return None
        if data["started"] is None:
            data["started"] = now
            data["deadline"] = now + 25
        if now - data["started"] > 900:
            raise RuntimeError("Town loop exceeded its 15 minute limit.")
        if now > data["deadline"]:
            raise RuntimeError(f"Town loop timed out in {data['phase']}; no further input was sent.")
        if now < data["ready_at"]:
            return None
        gray = self.vision.normalize(frame)
        find = lambda name: self.vision.find(gray, name)
        phase = data["phase"]
        play = find("play")
        menu = find("waypoint_header")
        hud = find("hud")
        act1, act2 = find("area_act1"), find("area_act2")

        if data.get("camera_settling"):
            stable = self.vision.camera_stable(frame, now)
            data["camera_reading"] = getattr(self.vision, "camera_reading", None)
            data["town_current"] = bool(hud and (act1 or act2))
            if menu or find("npc_cancel"):
                data["camera_settling"] = False
            elif hud and (act1 or act2) and stable:
                data["camera_settling"] = False
            else:
                if now >= data["camera_deadline"]:
                    raise RuntimeError("Town camera did not settle after release; no further input")
                return None

        if phase in {"character", "after_exit"}:
            # 뒤의 Play가 보여도 난이도 모달이 있으면 생성 입력을 보내지 않는다.
            if not play or find("difficulty_header"):
                return None
            if phase == "after_exit":
                if data["reset_stage"] == "leave_initial_hell":
                    data.update(difficulty="nightmare", reset_stage="nightmare")
                elif data["reset_stage"] == "leave_nightmare":
                    data.update(difficulty="hell", reset_stage="restored_hell", preparation_rooms=data["preparation_rooms"] + 1)
                else:
                    data["completed"] += 1
                    if data["completed"] >= data["repeat_count"]:
                        self._transition("done", now)
                        return None
                data.update(probes=0, waypoint_attempts=0, return_attempts=0, map_toggled=False, npc_recoveries=0)
            return self._action("difficulty", now, "Open difficulty for the selected game character", (play["x"], play["y"]))

        if phase == "difficulty":
            button = find(data["difficulty"])
            if find("difficulty_header") and button:
                return self._action("act1_spawn", now, f"Create {data['difficulty']} room", (button["x"], button["y"]), settle=1.4)
            return None

        if phase == "act1_spawn":
            if not hud or menu or find("save_exit"):
                return None
            if data["reset_stage"] == "nightmare" and (act1 or act2):
                data["reset_stage"] = "leave_nightmare"
                return self._action("exit_menu", now, "Preparation Nightmare town observed: save and return to Hell", key="esc")
            if data["reset_stage"] == "restored_hell":
                if act2:
                    raise RuntimeError("Difficulty reset did not restore Act 1; human review required")
                if act1:
                    data["reset_stage"] = None
            if act1:
                data["town_layout"] = "tent" if find("layout_tent") else "upper_probe"
                self._transition("act1_waypoint", now, settle=0)
                phase = "act1_waypoint"
            elif act2:
                if data["reset_act2_start"]:
                    if data["difficulty_reset_used"]:
                        raise RuntimeError("Act 2 starting point reset already used; human review required")
                    data.update(difficulty_reset_used=True, reset_stage="leave_initial_hell")
                    return self._action("exit_menu", now, "Hell Act 2 start: prepare Nightmare/Hell reset to Act 1", key="esc")
                self._transition("act2_spawn", now, settle=0)
                phase = "act2_spawn"
            elif not data["map_toggled"]:
                data["map_toggled"] = True
                return self._action("act1_spawn", now, "Reveal automap area title", key="tab")
            else:
                return None

        if phase in {"act2_spawn", "act2_moving"}:
            if data["holding"]:
                reason = (
                    "waypoint panel"
                    if menu
                    else "NPC dialogue"
                    if find("npc_cancel")
                    else "town observation lost"
                    if not (act2 and hud)
                    else "waypoint visible"
                    if find("world_act2_waypoint")
                    else "movement limit"
                    if now >= data["move_until"]
                    else None
                )
                if reason:
                    return self._release(now, "Act 2 approach: release on " + reason, phase="act2_spawn")
                return None
            if menu and find("list_act2"):
                self._transition("act2_list", now, settle=0)
                phase = "act2_list"
            elif not (act2 and hud) or menu or find("save_exit"):
                return None
            elif find("npc_cancel"):
                if data["npc_recoveries"] >= 3:
                    raise RuntimeError("NPC dialogue recovery limit reached.")
                data["npc_recoveries"] += 1
                return self._action("act2_spawn", now, "Close detected Act 2 town dialogue", key="esc", settle=0.2)
            else:
                waypoint = find("world_act2_waypoint")
                if waypoint:
                    if data["waypoint_attempts"] >= 3:
                        raise RuntimeError("Act 2 waypoint did not open after three observed approaches.")
                    data["waypoint_attempts"] += 1
                    return self._action("act2_spawn", now, "Open detected Act 2 waypoint", (waypoint["x"], waypoint["y"]), settle=0.7)
                if find("layout_act2_palace") and not data.get("palace_approach_used"):
                    data["palace_approach_used"] = True
                    return self._move(
                        now, "Current palace facade: bounded northeast waypoint approach", (900, 220), 2.4, phase="act2_moving"
                    )
                if data["town_layout"] != "act2_spawn":
                    if find("layout_act2_spawn"):
                        data["town_layout"] = "act2_spawn"
                    else:
                        raise RuntimeError("Act 2 starting position needs reviewed waypoint approach; no blind town movement")
                # 고정 마을의 검토한 시작 배치에만 제한된 탐색을 적용한다. 도착은 실제 발판을 다시 찾는다.
                route = (((633, 150), 2.0), ((900, 220), 1.8), ((430, 200), 2.4))
                index = data.get("act2_probes", 0)
                if index >= len(route):
                    raise RuntimeError("Act 2 waypoint absent after three reviewed spawn probes")
                point, seconds = route[index]
                data["act2_probes"] = index + 1
                return self._move(now, "Reviewed Act 2 spawn: reveal and reacquire waypoint", point, seconds, phase="act2_moving")

        if phase in {"act1_waypoint", "act1_moving", "wait_act1_panel", "npc_recovery"}:
            npc = find("npc_cancel")
            if npc:
                if data["holding"]:
                    return self._release(now, "NPC dialogue interrupted movement: release mouse")
                if data["npc_recoveries"] >= 3:
                    raise RuntimeError("NPC dialogue recovery limit reached.")
                data["npc_recoveries"] += 1
                return self._action("npc_recovery", now, "Close detected NPC dialogue", key="esc")
            if menu and find("list_act1"):
                if data["holding"]:
                    return self._release(now, "Waypoint panel detected: release mouse")
                return self._action("act2_list", now, "Switch waypoint list to Act 2", (216, 178))
            if not (act1 and hud) or menu or find("save_exit"):
                if data["holding"]:
                    return self._release(now, "Town confirmation lost: release mouse and re-observe")
                return None
            waypoint = find("world_act1_waypoint")
            if phase == "act1_moving":
                if waypoint or now >= data["move_until"]:
                    return self._release(now, "Waypoint detected while moving" if waypoint else "Bounded movement elapsed")
                return None
            if phase == "npc_recovery":
                self._transition("act1_waypoint", now, settle=0)
            if phase == "wait_act1_panel":
                if now < data.get("approach_until", 0):
                    return None
                if data["waypoint_attempts"] >= 3:
                    raise RuntimeError("Act 1 waypoint did not open after three observed approaches.")
                # 첫 클릭이 접근만 했다면 도착한 발판을 다시 클릭한다.
                if waypoint is None:
                    return self._move(now, "Waypoint approach blocked: lower-right detour", (950, 539), duration=1.0)
            if waypoint:
                data["waypoint_attempts"] += 1
                data["approach_until"] = now + 2.0
                return self._action("wait_act1_panel", now, "Open detected world waypoint", (waypoint["x"], waypoint["y"]), settle=0.08)
            if data["probes"] >= 3:
                raise RuntimeError("Act 1 waypoint was not visible after three bounded town probes.")
            data["probes"] += 1
            route = (
                [((949, 438), 1.8), ((800, 539), 0.8), ((633, 160), 1.8)]
                if data["town_layout"] == "tent"
                else [((800, 160), 1.2), ((949, 438), 1.5), ((800, 539), 1.2)]
            )
            point, duration = route[data["probes"] - 1]
            return self._move(now, "Observe while moving to reveal waypoint", point, duration)

        if phase == "act2_list":
            if menu and find("list_act2"):
                point = (265, 498) if data["destination"] == "arcane" else (265, 211)
                return self._action("destination_arrival", now, f"Travel to {data['destination']}", point, settle=1.4)
            return None

        if phase == "destination_arrival":
            destination = act2 if data["destination"] == "town" else find("area_arcane")
            if destination and hud and not menu and not find("save_exit"):
                waypoint = find("world_act2_waypoint" if data["destination"] == "town" else "world_arcane_waypoint")
                if waypoint is None:
                    return None
                if data["arrival_only"]:
                    data["completed"] = 1
                    self._transition("done", now)
                    return None
                point = (waypoint["x"], waypoint["y"])
                data["return_attempts"] += 1
                return self._action("return_panel", now, "Arrival confirmed: reopen waypoint for Act 1 return", point, settle=0.7)
            return None

        if phase == "return_panel":
            if menu and find("list_act2"):
                return self._action("return_list", now, "Switch return list to Act 1", (158, 178))
            destination = act2 if data["destination"] == "town" else find("area_arcane")
            if destination and hud and not menu and not find("save_exit"):
                if data["destination"] == "town" and find("npc_cancel"):
                    if data["npc_recoveries"] >= 3:
                        raise RuntimeError("NPC dialogue recovery limit reached.")
                    data["npc_recoveries"] += 1
                    return self._action("return_panel", now, "Close detected return-town NPC dialogue", key="esc", settle=0.2)
                waypoint = find("world_act2_waypoint" if data["destination"] == "town" else "world_arcane_waypoint")
                if waypoint:
                    if data["return_attempts"] >= 3:
                        raise RuntimeError("Return waypoint did not open after three observed approaches.")
                    data["return_attempts"] += 1
                    return self._action(
                        "return_panel",
                        now,
                        "Reacquire return waypoint from the latest town frame",
                        (waypoint["x"], waypoint["y"]),
                        settle=0.7,
                    )
            return None
        if phase == "return_list":
            if menu and find("list_act1"):
                return self._action("act1_return", now, "Return to Rogue Encampment", (265, 211), settle=1.4)
            return None
        if phase == "act1_return":
            if act1 and hud and not menu and not find("save_exit"):
                return self._action("exit_menu", now, "Act 1 finish confirmed: open exit menu", key="esc")
            return None
        if phase == "exit_menu":
            button = find("save_exit")
            if button:
                return self._action(
                    "after_exit",
                    now,
                    "Save and exit; require character selection before next room",
                    (button["x"], button["y"]),
                    settle=0.12,
                )
        return None


def calibrate_scene(frame_path: Path, scene: str, directory: Path = ASSET_DIRECTORY) -> None:
    """Add variants from a human-reviewed scene; never learn while running."""
    names = {
        "character": ["play"],
        "difficulty": ["difficulty_header", "normal", "nightmare", "hell"],
        "act1": ["hud", "area_act1"],
        "act2": ["hud", "area_act2"],
        "waypoint1": ["waypoint_header", "list_act1"],
        "waypoint2": ["waypoint_header", "list_act2"],
        "exit": ["save_exit"],
        "world_waypoint1": ["world_act1_waypoint"],
    }
    if scene not in names:
        raise ValueError("Unknown calibration scene.")
    vision = TownLoopVision(directory)
    frame = cv.imdecode(np.fromfile(str(frame_path), dtype=np.uint8), cv.IMREAD_COLOR)
    if frame is None:
        raise ValueError("Calibration frame is unreadable.")
    gray = vision.normalize(frame)
    for name in names[scene]:
        definition = vision.manifest["templates"][name]
        x1, y1, x2, y2 = definition["search"]
        region = cv.GaussianBlur(gray[y1:y2, x1:x2], (3, 3), 0.8)
        best = None
        for template in vision.variants[name]:
            _, score, _, location = cv.minMaxLoc(cv.matchTemplate(region, cv.GaussianBlur(template, (3, 3), 0.8), cv.TM_CCOEFF_NORMED))
            if best is None or score > best[0]:
                best = (score, location, template.shape)
        _, (px, py), (height, width) = best
        crop = gray[y1 + py : y1 + py + height, x1 + px : x1 + px + width]
        variants = definition.setdefault("variants", [])
        filename = next(
            (filename for suffix in ("-reviewed-window", "-window") for filename in variants if Path(filename).stem.endswith(suffix)),
            f"templates/{name.replace('_', '-')}-window-ko-{width}x{height}.png",
        )
        asset_path = directory / filename
        asset_path.parent.mkdir(parents=True, exist_ok=True)
        encoded = cv.imencode(".png", crop)[1]
        encoded.tofile(str(asset_path))
        if filename not in variants:
            variants.append(filename)
        definition.setdefault("variant_asset_sha256", {})[filename] = hashlib.sha256(encoded.tobytes()).hexdigest()
        definition.setdefault("variant_sources", []).append(
            {"file": frame_path.as_posix(), "scene": scene, "crop": [x1 + px, y1 + py, x1 + px + width, y1 + py + height]}
        )
    (directory / "manifest.json").write_text(json.dumps(vision.manifest, indent=2), encoding="utf-8")


def main() -> int:
    """File bridge for supervised input adapters; never performs UI input."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--calibrate-from")
    parser.add_argument("--calibrate-frame")
    parser.add_argument("--scene")
    parser.add_argument("--capture-frame", help="Capture one frame through the product backend; sends no input")
    parser.add_argument("--frame")
    parser.add_argument("--state")
    parser.add_argument("--repeat", type=int, default=1)
    parser.add_argument("--difficulty", default="hell")
    parser.add_argument("--destination", default="town")
    args = parser.parse_args()
    if args.capture_frame:
        import ctypes
        from diablo2.common.capture import ScreenCapture
        from diablo2.common.config import CaptureConfig

        ctypes.windll.shcore.SetProcessDpiAwareness(2)
        capture = ScreenCapture(CaptureConfig(window_title="Diablo II: Resurrected", window_title_mode="exact", capture_backend="window"))
        frame = capture.grab().frame
        path = Path(args.capture_frame)
        path.parent.mkdir(parents=True, exist_ok=True)
        cv.imencode(".png", frame)[1].tofile(str(path))
        capture.close()
        print(json.dumps({"frame": path.as_posix(), "size": [frame.shape[1], frame.shape[0]]}))
        return 0
    if args.calibrate_from:
        calibrate(Path(args.calibrate_from))
        print(json.dumps({"calibration": ASSET_DIRECTORY.as_posix()}))
        return 0
    if args.calibrate_frame:
        calibrate_scene(Path(args.calibrate_frame), args.scene)
        print(json.dumps({"calibrated_scene": args.scene, "frame": args.calibrate_frame}))
        return 0
    if not args.frame or not args.state:
        parser.error("--frame and --state are required for the observation bridge")
    planner = TownLoopPlanner(TownLoopVision(), args.repeat, args.difficulty, args.destination)
    state_path = Path(args.state)
    if state_path.exists():
        planner.restore(json.loads(state_path.read_text(encoding="utf-8")))
    frame = cv.imdecode(np.fromfile(args.frame, dtype=np.uint8), cv.IMREAD_COLOR)
    if frame is None:
        raise RuntimeError("Input frame is unreadable.")
    action = planner.step(frame)
    state_path.parent.mkdir(parents=True, exist_ok=True)
    state_path.write_text(json.dumps(planner.data, indent=2), encoding="utf-8")
    if action and action["kind"] in {"click", "hold"}:
        action["point"] = [
            round(action["point"][0] * frame.shape[1] / REFERENCE_SIZE[0]),
            round(action["point"][1] * frame.shape[0] / REFERENCE_SIZE[1]),
        ]
    print(json.dumps({"action": action, "state": planner.data}))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
