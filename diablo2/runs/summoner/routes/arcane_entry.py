from __future__ import annotations

import time

from diablo2.common.capture import ScreenCapture
from diablo2.runs.base import RunPort
from diablo2.runs.summoner.routes.common.arcane_common import ARCANE_SANCTUARY_RATIO, settle_arcane_entry
from diablo2.runs.summoner.runtime.runtime_helpers import (
    apply_offset,
    approach_waypoint,
    click_match_center,
    click_panel_ratio,
    click_relative_ratio,
    hold_move_until_waypoint,
    locate_best_template,
    locate_current_waypoint_list,
    locate_template,
    locate_waypoint_panel,
    sleep_range,
    wait_for_optional_any_template,
    wait_for_optional_template,
    wait_for_waypoint_panel,
)

PORT = RunPort(
    key="arcane_entry",
    kind="payload",
    description="Travel from Act 1 town to the Arcane Sanctuary entry and settle the character for route execution.",
    status="ready",
    notes=(
        "Owns the current waypoint-finding and Arcane Sanctuary travel flow.",
        "This is the staging piece that should run before north_go or other wing route pieces.",
    ),
)


def run_arcane_entry(session, capture: ScreenCapture) -> None:
    session.events.put(session.event_class("info", "Summoner: assuming the minimap is already open in Act 1 town."))
    sleep_range(session, *session.MINIMAP_SLEEP)

    map_match = find_minimap_waypoint_with_scouting(session, capture)
    session.events.put(
        session.event_class(
            "info",
            f"Summoner: found the Act 1 waypoint marker on the minimap (score={map_match.score:.3f}); using it only as direction guidance.",
        )
    )
    session.events.put(session.event_class("info", "Summoner: moving to the town center-right anchor before waypoint search."))
    session._current_scout_anchor = session._center_right_anchor()
    click_relative_ratio(session, capture, *session._current_scout_anchor, apply_jitter=False)
    sleep_range(session, *session.SCOUT_MOVE_SETTLE)
    session.events.put(
        session.event_class(
            "info",
            "Summoner: scanning the world view for the real waypoint object with the center-right vertical sweep.",
        )
    )

    act1_panel = open_act1_waypoint_list(session, capture)
    session.events.put(session.event_class("info", "Summoner: Act 1 waypoint list is open."))
    travel_to_arcane_sanctuary(session, capture, act1_panel)
    session.events.put(session.event_class("info", "Summoner: switched to Act 2 and clicked Arcane Sanctuary."))
    settle_arcane_entry(session, capture)


def find_minimap_waypoint_with_scouting(session, capture):
    best_match = None
    initial = wait_for_optional_any_template(session, capture, session._act1_map_templates, session.MAP_THRESHOLD, session.MAP_SCAN_TIMEOUT)
    if initial is not None:
        session._last_detection_route = "direct"
        return initial
    best_match = locate_best_template(session, capture.grab().frame, session._act1_map_templates, 0.0)
    for index, point in enumerate(session.MINIMAP_SCOUT_POINTS, start=1):
        session.events.put(session.event_class("info", f"Summoner: waypoint marker is faint from spawn view, scouting right ({index}/{len(session.MINIMAP_SCOUT_POINTS)})."))
        click_relative_ratio(session, capture, point[0], point[1], apply_jitter=False)
        sleep_range(session, *session.SCOUT_MOVE_SETTLE)
        match = wait_for_optional_any_template(session, capture, session._act1_map_templates, session.MAP_THRESHOLD, session.MAP_SCAN_TIMEOUT)
        if match is not None:
            session._last_detection_route = "vertical_sweep"
            return match
        fallback = locate_best_template(session, capture.grab().frame, session._act1_map_templates, 0.0)
        if fallback is not None and (best_match is None or fallback.score > best_match.score):
            best_match = fallback
    best_score = -1.0 if best_match is None else best_match.score
    raise RuntimeError(f"Timed out waiting for Act 1 waypoint on minimap. Best match score was {best_score:.3f}.")


def refresh_visible_waypoint(session, capture, fallback):
    packet = capture.grab()
    refreshed = locate_best_template(session, packet.frame, [session._act1_waypoint_hover_template, session._act1_waypoint_template], session.WAYPOINT_THRESHOLD)
    return refreshed or fallback


def find_world_waypoint_with_scouting(session, capture):
    best_match = None
    initial = wait_for_optional_any_template(session, capture, [session._act1_waypoint_hover_template, session._act1_waypoint_template], session.WAYPOINT_THRESHOLD, session.WAYPOINT_SCAN_TIMEOUT)
    if initial is not None:
        return initial
    best_match = locate_best_template(session, capture.grab().frame, [session._act1_waypoint_hover_template, session._act1_waypoint_template], 0.0)
    vertical_anchor = (0.5, 0.5)
    upward_point = apply_offset(vertical_anchor, session.WORLD_SCOUT_UP_HOLD_POINT)
    downward_point = apply_offset(vertical_anchor, session.WORLD_SCOUT_DOWN_HOLD_POINT)
    session.events.put(session.event_class("info", "Summoner: waypoint not visible yet, holding upward movement from the Diablo window center."))
    match = hold_move_until_waypoint(session, capture, upward_point[0], upward_point[1], session.WORLD_SCOUT_UP_HOLD_SECONDS)
    sleep_range(session, *session.SCOUT_UP_SETTLE)
    if match is None:
        match = wait_for_optional_any_template(session, capture, [session._act1_waypoint_hover_template, session._act1_waypoint_template], session.WAYPOINT_THRESHOLD, session.WAYPOINT_SCAN_TIMEOUT)
    if match is not None:
        session.events.put(session.event_class("info", "Summoner: waypoint became visible during the upward hold; moving to it now."))
        session._last_detection_route = "vertical_sweep"
        return match
    fallback = locate_best_template(session, capture.grab().frame, [session._act1_waypoint_hover_template, session._act1_waypoint_template], 0.0)
    if fallback is not None and (best_match is None or fallback.score > best_match.score):
        best_match = fallback
    session.events.put(session.event_class("info", "Summoner: waypoint not visible yet, holding downward movement from the Diablo window center."))
    match = hold_move_until_waypoint(session, capture, downward_point[0], downward_point[1], session.WORLD_SCOUT_DOWN_HOLD_SECONDS)
    sleep_range(session, *session.SCOUT_RIGHT_SETTLE)
    if match is None:
        match = wait_for_optional_any_template(session, capture, [session._act1_waypoint_hover_template, session._act1_waypoint_template], session.WAYPOINT_THRESHOLD, session.WAYPOINT_SCAN_TIMEOUT)
    if match is not None:
        session.events.put(session.event_class("info", "Summoner: waypoint became visible during the downward hold; moving to it now."))
        session._last_detection_route = "vertical_sweep"
        return match
    fallback = locate_best_template(session, capture.grab().frame, [session._act1_waypoint_hover_template, session._act1_waypoint_template], 0.0)
    if fallback is not None and (best_match is None or fallback.score > best_match.score):
        best_match = fallback
    right_point = apply_offset((0.5, 0.5), session.WORLD_SCOUT_RIGHT_HOLD_POINT)
    session.events.put(session.event_class("info", "Summoner: vertical sweep missed; holding rightward movement from the Diablo window center."))
    match = hold_move_until_waypoint(session, capture, right_point[0], right_point[1], session.WORLD_SCOUT_RIGHT_HOLD_SECONDS)
    sleep_range(session, *session.SCOUT_HOLD_SETTLE)
    if match is None:
        match = wait_for_optional_any_template(session, capture, [session._act1_waypoint_hover_template, session._act1_waypoint_template], session.WAYPOINT_THRESHOLD, session.WAYPOINT_SCAN_TIMEOUT)
    if match is not None:
        session.events.put(session.event_class("info", "Summoner: waypoint became visible after the rightward hold; moving to it now."))
        session._last_detection_route = "right_probe"
        return match
    fallback = locate_best_template(session, capture.grab().frame, [session._act1_waypoint_hover_template, session._act1_waypoint_template], 0.0)
    if fallback is not None and (best_match is None or fallback.score > best_match.score):
        best_match = fallback
    best_score = -1.0 if best_match is None else best_match.score
    raise RuntimeError(f"Timed out waiting for Act 1 waypoint object. Best match score was {best_score:.3f}.")


def open_act1_waypoint_list(session, capture):
    for attempt in range(1, session.OPEN_ATTEMPTS + 1):
        open_match = locate_current_waypoint_list(session, capture)
        if open_match is not None:
            panel_match = locate_waypoint_panel(session, capture, session._act1_list_panel_template)
            if panel_match is not None:
                return panel_match
            return open_match
        target_match = find_world_waypoint_with_scouting(session, capture)
        target_match = refresh_visible_waypoint(session, capture, target_match)
        session.events.put(session.event_class("info", f"Summoner: found the real waypoint object on screen (attempt {attempt}/{session.OPEN_ATTEMPTS}, score={target_match.score:.3f})."))
        approach_waypoint(session, capture, target_match)
        sleep_range(session, *session.CLICK_SETTLE)
        hover_match = wait_for_optional_template(session, capture, session._act1_waypoint_hover_template, session.WAYPOINT_HOVER_THRESHOLD, session.HOVER_WAIT_SECONDS)
        click_target = hover_match or target_match
        session.events.put(session.event_class("info", f"Summoner: opening the waypoint list (attempt {attempt}/{session.OPEN_ATTEMPTS})."))
        click_match_center(session, capture, click_target)
        sleep_range(session, *session.ACTION_SLEEP)
        panel_match = wait_for_waypoint_list_open(session, capture, session.WAYPOINT_LIST_WAIT_SECONDS)
        if panel_match is not None:
            return panel_match
    raise RuntimeError("Could not open the Act 1 waypoint list from town.")


def wait_for_waypoint_list_open(session, capture, timeout_seconds: float):
    end_time = time.time() + timeout_seconds
    best_match = None
    templates = [session._act1_list_panel_template, session._act1_list_template]
    thresholds = [session.LIST_PANEL_THRESHOLD, session.LIST_THRESHOLD]
    while time.time() < end_time:
        packet = capture.grab()
        for template, threshold in zip(templates, thresholds):
            match = locate_template(session, packet.frame, template, threshold)
            if match is not None:
                return match
            fallback = locate_template(session, packet.frame, template, 0.0)
            if fallback is not None and (best_match is None or fallback.score > best_match.score):
                best_match = fallback
        time.sleep(0.03)
    return None


def travel_to_arcane_sanctuary(session, capture, act1_panel) -> None:
    session.events.put(session.event_class("info", "Summoner: clicking the Act 2 tab in the waypoint list."))
    click_panel_ratio(session, capture, act1_panel, session.ACT2_TAB_RATIO)
    sleep_range(session, *session.ACTION_SLEEP)
    act2_panel = wait_for_waypoint_panel(session, capture, session._act2_list_panel_template, session.LIST_PANEL_THRESHOLD, 2.5, "Act 2 waypoint list")
    session.events.put(session.event_class("info", "Summoner: Act 2 waypoint list is open."))
    session.events.put(session.event_class("info", "Summoner: clicking Arcane Sanctuary in the Act 2 list."))
    click_panel_ratio(session, capture, act2_panel, ARCANE_SANCTUARY_RATIO)
    sleep_range(session, *session.ACTION_SLEEP)
