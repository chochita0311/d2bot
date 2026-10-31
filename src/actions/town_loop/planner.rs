use super::{
    state::{InspectionStatus, MapInspection, ResetStage},
    *,
};
use crate::actions::ActionError;

/// Implementations must read only the current frame prepared by normalize().
pub trait TownSight {
    fn normalize(&mut self) -> Result<(), ActionError>;
    fn find(&mut self, name: &str) -> Result<Option<TownPoint>, ActionError>;
    fn reset_camera(&mut self);
    fn camera_stable(&mut self, now: f64) -> Result<bool, ActionError>;
    fn camera_reading(&self) -> serde_json::Value;
}
pub struct TownPlanner {
    pub data: TownState,
}
enum PlannedInput<'a> {
    Click([f64; 2]),
    Key(&'a str),
    Hold([f64; 2]),
    Release,
}
impl TownPlanner {
    pub fn new(
        options: TownOptions,
        calibrated: impl Fn(&str) -> bool,
    ) -> Result<Self, ActionError> {
        if !(1..=3).contains(&options.repeat_count) {
            return Err(ActionError::runtime(
                "Supervised town loop requires an explicit repeat count from 1 to 3.",
            ));
        }
        if options.arrival_only
            && (options.destination != Destination::Arcane || options.repeat_count != 1)
        {
            return Err(ActionError::runtime(
                "Arrival-only entry requires exactly one Arcane visit.",
            ));
        }
        if options.reset_act2_start && options.difficulty != Difficulty::Hell {
            return Err(ActionError::runtime(
                "Act 2 start reset requires Hell as the target difficulty.",
            ));
        }
        if options.inspect_arcane_map
            && (options.destination != Destination::Arcane || options.arrival_only)
        {
            return Err(ActionError::runtime(
                "Temporary Arcane map inspection requires a return waypoint flow.",
            ));
        }
        if options.destination == Destination::Arcane
            && (!calibrated("area_arcane") || !calibrated("world_arcane_waypoint"))
        {
            return Err(ActionError::runtime(
                "Arcane arrival has not been calibrated; use the town destination first.",
            ));
        }
        if options.destination == Destination::Canyon
            && (!calibrated("area_canyon") || !calibrated("world_canyon_waypoint"))
        {
            return Err(ActionError::runtime(
                "Canyon arrival has not been calibrated.",
            ));
        }
        Ok(Self {
            data: TownState::new(options),
        })
    }
    /// Restores known fields from a saved planner snapshot without resetting it.
    pub fn restore(
        &mut self,
        update: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<(), ActionError> {
        let mut merged = serde_json::to_value(&self.data)?;
        let object = merged
            .as_object_mut()
            .ok_or_else(|| ActionError::runtime("town state is not an object"))?;
        object.extend(update.clone());
        self.data = serde_json::from_value(merged)?;
        Ok(())
    }
    fn transition(&mut self, phase: TownPhase, now: f64, settle: f64) {
        self.data.phase = phase;
        self.data.deadline = Some(now + 25.0);
        self.data.ready_at = now + settle;
    }
    fn action(
        &mut self,
        phase: TownPhase,
        now: f64,
        reason: impl Into<String>,
        input: PlannedInput<'_>,
        settle: f64,
    ) -> Result<Option<TownAction>, ActionError> {
        self.transition(phase, now, settle);
        self.data.action_number = self
            .data
            .action_number
            .checked_add(1)
            .ok_or_else(|| ActionError::runtime("town action number exhausted"))?;
        let (kind, point, key) = match input {
            PlannedInput::Click(point) => (TownActionKind::Click, Some(point), None),
            PlannedInput::Key(key) => (TownActionKind::Key, None, Some(key)),
            PlannedInput::Hold(point) => (TownActionKind::Hold, Some(point), None),
            PlannedInput::Release => (TownActionKind::Release, None, None),
        };
        let point = point.map(|p| p.map(|v| v.round_ties_even() as i64));
        Ok(Some(TownAction {
            kind,
            reason: reason.into(),
            phase,
            number: self.data.action_number,
            key: key.map(str::to_owned),
            point,
        }))
    }
    fn click(
        &mut self,
        phase: TownPhase,
        now: f64,
        reason: impl Into<String>,
        point: [f64; 2],
        settle: f64,
    ) -> Result<Option<TownAction>, ActionError> {
        self.action(phase, now, reason, PlannedInput::Click(point), settle)
    }
    fn key(
        &mut self,
        phase: TownPhase,
        now: f64,
        reason: impl Into<String>,
        key: &str,
        settle: f64,
    ) -> Result<Option<TownAction>, ActionError> {
        self.action(phase, now, reason, PlannedInput::Key(key), settle)
    }
    fn movement(
        &mut self,
        phase: TownPhase,
        now: f64,
        reason: &str,
        point: [f64; 2],
        duration: f64,
    ) -> Result<Option<TownAction>, ActionError> {
        self.data.holding = true;
        self.data.move_until = now + duration;
        self.action(phase, now, reason, PlannedInput::Hold(point), 0.0)
    }
    fn release(
        &mut self,
        vision: &mut impl TownSight,
        phase: TownPhase,
        now: f64,
        reason: impl Into<String>,
    ) -> Result<Option<TownAction>, ActionError> {
        self.data.holding = false;
        self.data.camera_settling = Some(true);
        self.data.camera_deadline = Some(now + 4.0);
        vision.reset_camera();
        self.action(phase, now, reason, PlannedInput::Release, 0.08)
    }
    pub fn step(
        &mut self,
        vision: &mut impl TownSight,
        now: f64,
    ) -> Result<Option<TownAction>, ActionError> {
        use TownPhase::*;
        if !now.is_finite() {
            return Err(ActionError::runtime(
                "finite town observation time required",
            ));
        }
        if self.data.phase == Done {
            return Ok(None);
        }
        if self.data.started.is_none() {
            self.data.started = Some(now);
            self.data.deadline = Some(now + 25.0);
        }
        if now - self.data.started.unwrap_or(now) > 900.0 {
            return Err(ActionError::runtime(
                "Town loop exceeded its 15 minute limit.",
            ));
        }
        if now > self.data.deadline.unwrap_or(now) {
            return Err(ActionError::runtime(format!(
                "Town loop timed out in {}; no further input was sent.",
                self.data.phase.name()
            )));
        }
        if now < self.data.ready_at {
            return Ok(None);
        }
        vision.normalize()?;
        let mut phase = self.data.phase;
        let play = vision.find("play")?;
        let menu = vision.find("waypoint_header")?.is_some();
        let hud = vision.find("hud")?.is_some();
        let act1 = vision.find("area_act1")?.is_some();
        let act2 = vision.find("area_act2")?.is_some();
        if self.data.camera_settling == Some(true) {
            let stable = vision.camera_stable(now)?;
            self.data.camera_reading = Some(vision.camera_reading());
            self.data.town_current = Some(hud && (act1 || act2));
            if menu || vision.find("npc_cancel")?.is_some() || hud && (act1 || act2) && stable {
                self.data.camera_settling = Some(false);
            } else {
                if now >= self.data.camera_deadline.unwrap_or(now) {
                    return Err(ActionError::runtime(
                        "Town camera did not settle after release; no further input",
                    ));
                }
                return Ok(None);
            }
        }
        if matches!(phase, Character | AfterExit) {
            let Some(play) = play else {
                return Ok(None);
            };
            if vision.find("difficulty_header")?.is_some() {
                return Ok(None);
            }
            if phase == AfterExit {
                match self.data.reset_stage {
                    Some(ResetStage::LeaveInitialHell) => {
                        self.data.difficulty = super::Difficulty::Nightmare;
                        self.data.reset_stage = Some(ResetStage::Nightmare);
                    }
                    Some(ResetStage::LeaveNightmare) => {
                        self.data.difficulty = super::Difficulty::Hell;
                        self.data.reset_stage = Some(ResetStage::RestoredHell);
                        self.data.preparation_rooms += 1;
                    }
                    _ => {
                        self.data.completed += 1;
                        if self.data.completed >= u64::from(self.data.repeat_count) {
                            self.transition(Done, now, 0.7);
                            return Ok(None);
                        }
                    }
                }
                self.data.probes = 0;
                self.data.waypoint_attempts = 0;
                self.data.act1_waypoint_point = None;
                self.data.waypoint_detour_used = false;
                self.data.waypoint_detour_until = 0.0;
                self.data.return_attempts = 0;
                self.data.map_toggled = false;
                self.data.npc_recoveries = 0;
                self.data.arcane_map_inspection = None;
            }
            return self.click(
                Difficulty,
                now,
                "Open difficulty for the selected game character",
                play.point(),
                0.12,
            );
        }
        if phase == Difficulty {
            let button = vision.find(self.data.difficulty.name())?;
            if vision.find("difficulty_header")?.is_some()
                && let Some(button) = button
            {
                return self.click(
                    Act1Spawn,
                    now,
                    format!("Create {} room", self.data.difficulty.name()),
                    button.point(),
                    0.12,
                );
            }
            return Ok(None);
        }
        if phase == Act1Spawn {
            if !hud || menu || vision.find("save_exit")?.is_some() {
                return Ok(None);
            }
            if matches!(self.data.reset_stage, Some(ResetStage::Nightmare)) && (act1 || act2) {
                self.data.reset_stage = Some(ResetStage::LeaveNightmare);
                return self.key(
                    ExitMenu,
                    now,
                    "Preparation Nightmare town observed: save and return to Hell",
                    "esc",
                    0.12,
                );
            }
            if matches!(self.data.reset_stage, Some(ResetStage::RestoredHell)) {
                if act2 {
                    return Err(ActionError::runtime(
                        "Difficulty reset did not restore Act 1; human review required",
                    ));
                }
                if act1 {
                    self.data.reset_stage = None;
                }
            }
            if act1 {
                self.data.town_layout = Some(
                    if vision.find("layout_tent")?.is_some() {
                        "tent"
                    } else {
                        "upper_probe"
                    }
                    .into(),
                );
                self.transition(Act1Waypoint, now, 0.0);
                phase = Act1Waypoint;
            } else if act2 {
                if self.data.reset_act2_start {
                    if self.data.difficulty_reset_used {
                        return Err(ActionError::runtime(
                            "Act 2 starting point reset already used; human review required",
                        ));
                    }
                    self.data.difficulty_reset_used = true;
                    self.data.reset_stage = Some(ResetStage::LeaveInitialHell);
                    return self.key(
                        ExitMenu,
                        now,
                        "Hell Act 2 start: prepare Nightmare/Hell reset to Act 1",
                        "esc",
                        0.12,
                    );
                }
                self.transition(Act2Spawn, now, 0.0);
                phase = Act2Spawn;
            } else if !self.data.map_toggled {
                self.data.map_toggled = true;
                return self.key(Act1Spawn, now, "Reveal automap area title", "tab", 0.12);
            } else {
                return Ok(None);
            }
        }
        if matches!(phase, Act2Spawn | Act2Moving) {
            if self.data.holding {
                let reason = if menu {
                    Some("waypoint panel")
                } else if vision.find("npc_cancel")?.is_some() {
                    Some("NPC dialogue")
                } else if !(act2 && hud) {
                    Some("town observation lost")
                } else if vision.find("world_act2_waypoint")?.is_some() {
                    Some("waypoint visible")
                } else if now >= self.data.move_until {
                    Some("movement limit")
                } else {
                    None
                };
                return if let Some(reason) = reason {
                    self.release(
                        vision,
                        Act2Spawn,
                        now,
                        format!("Act 2 approach: release on {reason}"),
                    )
                } else {
                    Ok(None)
                };
            }
            if menu && vision.find("list_act2")?.is_some() {
                self.transition(Act2List, now, 0.0);
                phase = Act2List;
            } else if !(act2 && hud) || menu || vision.find("save_exit")?.is_some() {
                return Ok(None);
            } else if vision.find("npc_cancel")?.is_some() {
                if self.data.npc_recoveries >= 3 {
                    return Err(ActionError::runtime("NPC dialogue recovery limit reached."));
                }
                self.data.npc_recoveries += 1;
                return self.key(
                    Act2Spawn,
                    now,
                    "Close detected Act 2 town dialogue",
                    "esc",
                    0.2,
                );
            } else {
                if let Some(waypoint) = vision.find("world_act2_waypoint")? {
                    if self.data.waypoint_attempts >= 3 {
                        return Err(ActionError::runtime(
                            "Act 2 waypoint did not open after three observed approaches.",
                        ));
                    }
                    self.data.waypoint_attempts += 1;
                    return self.click(
                        Act2Spawn,
                        now,
                        "Open detected Act 2 waypoint",
                        waypoint.point(),
                        0.7,
                    );
                }
                if vision.find("layout_act2_palace")?.is_some()
                    && self.data.palace_approach_used != Some(true)
                {
                    self.data.palace_approach_used = Some(true);
                    return self.movement(
                        Act2Moving,
                        now,
                        "Current palace facade: bounded northeast waypoint approach",
                        [900.0, 220.0],
                        2.4,
                    );
                }
                if self.data.town_layout.as_deref() != Some("act2_spawn") {
                    if vision.find("layout_act2_spawn")?.is_some() {
                        self.data.town_layout = Some("act2_spawn".into());
                    } else {
                        return Err(ActionError::runtime(
                            "Act 2 starting position needs reviewed waypoint approach; no blind town movement",
                        ));
                    }
                }
                let route = [
                    ([633.0, 150.0], 2.0),
                    ([900.0, 220.0], 1.8),
                    ([430.0, 200.0], 2.4),
                ];
                let index = self.data.act2_probes.unwrap_or(0);
                let Some((point, seconds)) = route.get(index) else {
                    return Err(ActionError::runtime(
                        "Act 2 waypoint absent after three reviewed spawn probes",
                    ));
                };
                self.data.act2_probes = Some(index + 1);
                return self.movement(
                    Act2Moving,
                    now,
                    "Reviewed Act 2 spawn: reveal and reacquire waypoint",
                    *point,
                    *seconds,
                );
            }
        }
        if matches!(
            phase,
            Act1Waypoint | Act1Moving | WaitAct1Panel | NpcRecovery
        ) {
            if vision.find("npc_cancel")?.is_some() {
                if self.data.holding {
                    return self.release(
                        vision,
                        Act1Waypoint,
                        now,
                        "NPC dialogue interrupted movement: release mouse",
                    );
                }
                if self.data.npc_recoveries >= 3 {
                    return Err(ActionError::runtime("NPC dialogue recovery limit reached."));
                }
                self.data.npc_recoveries += 1;
                return self.key(NpcRecovery, now, "Close detected NPC dialogue", "esc", 0.12);
            }
            if menu && vision.find("list_act1")?.is_some() {
                if self.data.holding {
                    return self.release(
                        vision,
                        Act1Waypoint,
                        now,
                        "Waypoint panel detected: release mouse",
                    );
                }
                return self.click(
                    Act2List,
                    now,
                    "Switch waypoint list to Act 2",
                    [216.0, 178.0],
                    0.12,
                );
            }
            if !(act1 && hud) || menu || vision.find("save_exit")?.is_some() {
                if self.data.holding {
                    return self.release(
                        vision,
                        Act1Waypoint,
                        now,
                        "Town confirmation lost: release mouse and re-observe",
                    );
                }
                return Ok(None);
            }
            let waypoint = vision.find("world_act1_waypoint")?;
            if phase == Act1Moving {
                if now < self.data.waypoint_detour_until {
                    return Ok(None);
                }
                return if waypoint.is_some() || now >= self.data.move_until {
                    self.release(
                        vision,
                        Act1Waypoint,
                        now,
                        if waypoint.is_some() {
                            "Waypoint detected while moving"
                        } else {
                            "Bounded movement elapsed"
                        },
                    )
                } else {
                    Ok(None)
                };
            }
            if phase == NpcRecovery {
                self.transition(Act1Waypoint, now, 0.0);
            }
            if phase == WaitAct1Panel {
                if now < self.data.approach_until.unwrap_or(0.0) {
                    return Ok(None);
                }
                if self.data.waypoint_attempts >= 3 {
                    return Err(ActionError::runtime(
                        "Act 1 waypoint did not open after three observed approaches.",
                    ));
                }
                if let (Some(waypoint), Some(previous)) = (&waypoint, self.data.act1_waypoint_point)
                    && self.data.waypoint_attempts >= 2
                    && !self.data.waypoint_detour_used
                {
                    let current = waypoint.point();
                    let foot = [
                        REFERENCE_SIZE[0] as f64 / 2.0,
                        REFERENCE_SIZE[1] as f64 * 0.51,
                    ];
                    if norm(current, previous) <= 12.0 && norm(current, foot) > 180.0 {
                        self.data.waypoint_detour_used = true;
                        self.data.waypoint_detour_until = now + 1.0;
                        let detour = if current[0] < foot[0] {
                            [
                                (current[0] + foot[0]) / 2.0,
                                (current[1].min(foot[1]) - 80.0).max(80.0),
                            ]
                        } else {
                            [950.0, 539.0]
                        };
                        return self.movement(
                            Act1Moving,
                            now,
                            "Visible waypoint approach stalled: bounded waypoint-side detour",
                            detour,
                            1.0,
                        );
                    }
                }
                if waypoint.is_none() {
                    return self.movement(
                        Act1Moving,
                        now,
                        "Waypoint approach blocked: lower-right detour",
                        [950.0, 539.0],
                        1.0,
                    );
                }
            }
            if let Some(waypoint) = waypoint {
                self.data.waypoint_attempts += 1;
                self.data.approach_until = Some(now + 2.0);
                self.data.act1_waypoint_point = Some(waypoint.point());
                return self.click(
                    WaitAct1Panel,
                    now,
                    "Open detected world waypoint",
                    waypoint.point(),
                    0.08,
                );
            }
            if self.data.probes >= 3 {
                return Err(ActionError::runtime(
                    "Act 1 waypoint was not visible after three bounded town probes.",
                ));
            }
            self.data.probes += 1;
            let route = if self.data.town_layout.as_deref() == Some("tent") {
                [
                    ([949.0, 438.0], 1.8),
                    ([800.0, 539.0], 0.8),
                    ([633.0, 160.0], 1.8),
                ]
            } else {
                [
                    ([800.0, 160.0], 1.2),
                    ([949.0, 438.0], 1.5),
                    ([800.0, 539.0], 1.2),
                ]
            };
            let (point, duration) = route[self.data.probes - 1];
            return self.movement(
                Act1Moving,
                now,
                "Observe while moving to reveal waypoint",
                point,
                duration,
            );
        }
        if phase == Act2List {
            if menu && vision.find("list_act2")?.is_some() {
                let y = match self.data.destination {
                    Destination::Town => 211.0,
                    Destination::Arcane => 498.0,
                    Destination::Canyon => 539.0,
                };
                return self.click(
                    DestinationArrival,
                    now,
                    format!("Travel to {}", self.data.destination.name()),
                    [265.0, y],
                    0.12,
                );
            }
            return Ok(None);
        }
        if phase == DestinationArrival {
            return self.arrival(vision, now, menu, hud, act2);
        }
        if phase == ReturnPanel {
            if menu && vision.find("list_act2")?.is_some() {
                return self.click(
                    ReturnList,
                    now,
                    "Switch return list to Act 1",
                    [158.0, 178.0],
                    0.12,
                );
            }
            let mut destination = if self.data.destination == Destination::Town {
                act2
            } else {
                vision.find(self.data.destination.area())?.is_some()
            };
            let mut native_waypoint = None;
            if let Some(inspection) = &self.data.arcane_map_inspection
                && matches!(
                    inspection.status,
                    InspectionStatus::Hidden | InspectionStatus::OnNativeReacquired
                )
            {
                native_waypoint = vision.find("world_arcane_waypoint")?;
                destination = native_waypoint
                    .as_ref()
                    .is_some_and(TownPoint::is_native_hub)
                    && (inspection.status == InspectionStatus::Hidden || destination);
            }
            if destination && hud && !menu && vision.find("save_exit")?.is_none() {
                if self.data.destination == Destination::Town
                    && vision.find("npc_cancel")?.is_some()
                {
                    if self.data.npc_recoveries >= 3 {
                        return Err(ActionError::runtime("NPC dialogue recovery limit reached."));
                    }
                    self.data.npc_recoveries += 1;
                    return self.key(
                        ReturnPanel,
                        now,
                        "Close detected return-town NPC dialogue",
                        "esc",
                        0.2,
                    );
                }
                let waypoint = if native_waypoint.is_some() {
                    native_waypoint
                } else {
                    vision.find(self.data.destination.waypoint())?
                };
                if let Some(waypoint) = waypoint {
                    if self.data.return_attempts >= 3 {
                        return Err(ActionError::runtime(
                            "Return waypoint did not open after three observed approaches.",
                        ));
                    }
                    self.data.return_attempts += 1;
                    return self.click(
                        ReturnPanel,
                        now,
                        "Reacquire return waypoint from the latest town frame",
                        waypoint.point(),
                        0.7,
                    );
                }
            }
            return Ok(None);
        }
        if phase == ReturnList {
            if menu && vision.find("list_act1")?.is_some() {
                return self.click(
                    Act1Return,
                    now,
                    "Return to Rogue Encampment",
                    [265.0, 211.0],
                    0.12,
                );
            }
            return Ok(None);
        }
        if phase == Act1Return {
            if self
                .data
                .arcane_map_inspection
                .as_ref()
                .is_some_and(|i| i.status == InspectionStatus::Hidden)
                && hud
                && !menu
                && vision.find("save_exit")?.is_none()
            {
                if let Some(i) = &mut self.data.arcane_map_inspection {
                    i.status = InspectionStatus::RestoreRequested;
                }
                return self.key(
                    Act1Return,
                    now,
                    "Restore automap after waypoint return",
                    "tab",
                    0.12,
                );
            }
            if act1 && hud && !menu && vision.find("save_exit")?.is_none() {
                if let Some(i) = &mut self.data.arcane_map_inspection
                    && i.status == InspectionStatus::RestoreRequested
                {
                    i.status = InspectionStatus::Act1OnObserved;
                }
                return self.key(
                    ExitMenu,
                    now,
                    "Act 1 finish confirmed: open exit menu",
                    "esc",
                    0.12,
                );
            }
            return Ok(None);
        }
        if phase == ExitMenu
            && let Some(button) = vision.find("save_exit")?
        {
            return self.click(
                AfterExit,
                now,
                "Save and exit; require character selection before next room",
                button.point(),
                0.12,
            );
        }
        Ok(None)
    }
    fn arrival(
        &mut self,
        vision: &mut impl TownSight,
        now: f64,
        menu: bool,
        hud: bool,
        act2: bool,
    ) -> Result<Option<TownAction>, ActionError> {
        let mut destination = if self.data.destination == Destination::Town {
            act2
        } else {
            vision.find(self.data.destination.area())?.is_some()
        };
        let mut waypoint = None;
        let status = self.data.arcane_map_inspection.as_ref().map(|i| i.status);
        if status == Some(InspectionStatus::Hidden) {
            waypoint = vision.find("world_arcane_waypoint")?;
            destination = waypoint.as_ref().is_some_and(TownPoint::is_native_hub);
            let requested = self
                .data
                .arcane_map_inspection
                .as_ref()
                .map_or(now, |i| i.requested_at);
            if !destination && now - requested >= 1.5 {
                if !hud
                    || menu
                    || vision.find("save_exit")?.is_some()
                    || vision.find("npc_cancel")?.is_some()
                {
                    return Err(ActionError::runtime(
                        "Arcane map restoration requires current HUD without a panel.",
                    ));
                }
                if let Some(i) = &mut self.data.arcane_map_inspection {
                    i.status = InspectionStatus::RestoreForReacquisition;
                    i.restore_requested_at = Some(now);
                }
                let deadline = self.data.deadline;
                let action = self.key(
                    TownPhase::DestinationArrival,
                    now,
                    "Restore Arcane automap for fresh waypoint reacquisition",
                    "tab",
                    0.12,
                )?;
                self.data.deadline = deadline.zip(self.data.deadline).map(|(a, b)| a.min(b));
                return Ok(action);
            }
        } else if matches!(
            status,
            Some(InspectionStatus::RestoreForReacquisition | InspectionStatus::OnReacquiring)
        ) {
            let restored = self
                .data
                .arcane_map_inspection
                .as_ref()
                .and_then(|i| i.restore_requested_at)
                .ok_or_else(|| ActionError::runtime("map restoration timestamp missing"))?;
            if now - restored >= 6.0 {
                return Err(ActionError::runtime(
                    "Current Arcane waypoint missing after bounded map restoration.",
                ));
            }
            waypoint = vision.find("world_arcane_waypoint")?;
            let on_observed = destination
                && hud
                && !menu
                && vision.find("save_exit")?.is_none()
                && vision.find("npc_cancel")?.is_none();
            if on_observed
                && status == Some(InspectionStatus::RestoreForReacquisition)
                && let Some(i) = &mut self.data.arcane_map_inspection
            {
                i.status = InspectionStatus::OnReacquiring;
                i.restored_at = Some(now);
            }
            destination = on_observed && waypoint.as_ref().is_some_and(TownPoint::is_native_hub);
            if destination && let Some(i) = &mut self.data.arcane_map_inspection {
                i.status = InspectionStatus::OnNativeReacquired;
            }
        }
        if destination && hud && !menu && vision.find("save_exit")?.is_none() {
            if waypoint.is_none() {
                waypoint = vision.find(self.data.destination.waypoint())?;
            }
            let Some(waypoint) = waypoint else {
                if self.data.inspect_arcane_map
                    && self.data.arcane_map_inspection.is_none()
                    && vision.find("npc_cancel")?.is_none()
                {
                    self.data.arcane_map_inspection = Some(MapInspection {
                        status: InspectionStatus::Hidden,
                        requested_at: now,
                        restore_requested_at: None,
                        restored_at: None,
                    });
                    return self.key(
                        TownPhase::DestinationArrival,
                        now,
                        "Inspect current Arcane waypoint with automap hidden",
                        "tab",
                        0.12,
                    );
                }
                return Ok(None);
            };
            if self.data.arrival_only {
                self.data.completed = 1;
                self.transition(TownPhase::Done, now, 0.7);
                return Ok(None);
            }
            self.data.return_attempts += 1;
            return self.click(
                TownPhase::ReturnPanel,
                now,
                "Arrival confirmed: reopen waypoint for Act 1 return",
                waypoint.point(),
                0.7,
            );
        }
        Ok(None)
    }
}
fn norm(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}
