from __future__ import annotations

from dataclasses import dataclass, field

from diablo2.common.config import FarmProfile


@dataclass(frozen=True)
class RunPayloadState:
    key: str
    description: str
    success_signal: str
    next_action: str


@dataclass(frozen=True)
class RunPort:
    key: str
    kind: str
    description: str
    status: str = "planned"
    reusable: bool = True
    notes: tuple[str, ...] = ()


@dataclass(frozen=True)
class RunRouteSegment:
    key: str
    label: str
    direction: str
    status: str = "planned"
    notes: tuple[str, ...] = ()


@dataclass
class RunDefinition:
    profile_id: str
    profile: FarmProfile
    payload_states: tuple[RunPayloadState, ...]
    route_segments: tuple[RunRouteSegment, ...] = ()
    decision_ports: tuple[RunPort, ...] = ()
    shared_ports: tuple[RunPort, ...] = ()
    metadata: dict[str, str] = field(default_factory=dict)

    def summarize(self) -> list[str]:
        hunting = self.profile.hunting
        target_monsters = ", ".join(hunting.target_monsters) or "(not configured)"
        target_areas = ", ".join(hunting.target_areas) or "(not configured)"
        waypoint = (
            f"Act {hunting.waypoint_act} / {hunting.waypoint_name}"
            if hunting.waypoint_act and hunting.waypoint_name
            else "(not configured)"
        )
        active_route = ", ".join(segment.label for segment in self.route_segments if segment.status == "ready") or "(route pieces not ready)"
        ports = ", ".join(port.key for port in (*self.decision_ports, *self.shared_ports)) or "(none yet)"
        return [
            f"goal: {self.profile.goal}",
            f"waypoint: {waypoint}",
            f"targets: {target_monsters}",
            f"areas: {target_areas}",
            f"fight style: {hunting.fight_style}",
            f"search timeout: {hunting.search_timeout_seconds}s",
            f"ready route pieces: {active_route}",
            f"shared runtime pieces: {ports}",
            "wrapper: room create/exit stays outside this payload",
        ]
