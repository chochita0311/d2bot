use super::*;
use crate::runs::summoner::steering::{ArcaneRouteFlow, RouteFlowOptions};
impl State {
    fn select_route<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &self,
        c: &mut Context<'_, B, E, V>,
        f: &Frame<V::View>,
        excluded: &[[f64; 4]],
    ) -> Result<PathChoice, RunError> {
        c.vision.graph_choose(
            &f.view,
            RouteChoiceRequest {
                wing: c.route_wing,
                now: c.now(),
                excluded,
                landings: &f.landings,
                hub: if c.request.return_from.is_none() {
                    f.landmarks.hub
                } else {
                    None
                },
                returning: c.request.return_from.is_some(),
            },
        )
    }
    pub(super) fn movement<B: FieldInputBackend, E: ContinuousEnvironment, V: ContinuousVision>(
        &mut self,
        c: &mut Context<'_, B, E, V>,
        f: &mut Frame<V::View>,
    ) -> Result<Flow, RunError> {
        let floor_flow = c.vision.has_graph()
            && c.request.return_from.is_none()
            && f.landmarks.hub.is_none()
            && !c.vision.graph_position_current();
        if !c.vision.has_graph() {
            let visited = if self.position_current {
                self.breadcrumbs()
            } else {
                vec![]
            };
            f.choice = c.vision.flow_choose(
                &f.view,
                c.route_wing,
                &f.choice,
                c.now(),
                RouteFlowOptions {
                    position: self.position(),
                    visited: &visited,
                    excluded: &self.blocked(),
                },
            )?;
        } else {
            f.choice = self.select_route(c, f, &self.blocked())?;
            self.record("route_graph", c.vision.graph_summary());
            if f.choice.reason.starts_with("current_hub_departure_")
                && let Some(aim) = self.last_aim
            {
                let direction = delta(aim.map(f64::from), f.player.map(f64::from));
                if ArcaneRouteFlow::axis(direction) != c.route_wing.heading() {
                    if c.snapshot()?.held.contains(&c.inputs.inputs().keys()[0]) {
                        c.release()?;
                    }
                    c.vision.clear_steering_aim();
                }
            }
        }
        let (detour, flow_reason) = c.vision.flow_state();
        self.record("choice", &f.choice);
        self.record("flow_reason", &flow_reason);
        self.record("route_detour", detour);
        if floor_flow {
            self.record("route_selection_source", "current_floor_flow");
        }
        self.record("processed_at", c.now());
        let now = c.now();
        if !fresh(now, f.acquired) || now >= c.deadline {
            c.release()?;
            self.action("stale_hold");
            return Ok(Flow::Next);
        }
        let Some(aim) = f.choice.aim else {
            c.release()?;
            self.action("path_hold");
            if self.aim_unavailable_since.is_none()
                || self
                    .previous_row()
                    .is_some_and(|row| row["action"] != "path_hold")
            {
                self.aim_unavailable_since = Some(f.acquired);
            }
            if self
                .aim_unavailable_since
                .is_some_and(|since| now - since >= 1.5)
            {
                return Ok(self.pause(f.choice.reason.clone()));
            }
            return Ok(Flow::Next);
        };
        self.aim_unavailable_since = None;
        let direction = delta(aim.map(f64::from), f.player.map(f64::from));
        let heading = c.route_wing.heading();
        // 경로 선택과 같은 등각 투영 각도로 검사해 현재 굴곡을 서로 다르게 판정하지 않는다.
        if f.landmarks.hub.is_none()
            && !ArcaneRouteFlow::follows(direction, heading, 150.0)
            && !matches!(
                flow_reason.as_str(),
                "follow_observed_bend" | "observed_corner_turn" | "observed_corner_probe"
            )
            && (!c.vision.has_graph() || floor_flow)
        {
            c.release()?;
            self.action("opposite_direction_hold");
            return Ok(self.pause("opposite_wing_aim_rejected"));
        }
        if !f.threats.hover.is_empty() {
            c.release()?;
            let hovered = self.last_aim.unwrap_or(aim);
            self.exclude(hovered, c.vision.clearance());
            let mut exclusions = self.blocked();
            exclusions.extend(
                f.threats
                    .hover
                    .iter()
                    .map(|candidate| candidate.evidence_box.map(|p| p as f64)),
            );
            let alternate = if c.vision.has_graph() {
                self.select_route(c, f, &exclusions)?
            } else {
                c.vision.choose(
                    &f.view,
                    c.route_wing,
                    ChooseOptions {
                        excluded: &exclusions,
                        allow_detour: true,
                        heading: detour,
                        prefer_frontier: true,
                        ..Default::default()
                    },
                )?
            };
            let Some(next) = alternate.aim else {
                return Ok(self.pause("hover_path_unobserved"));
            };
            c.inputs
                .inputs()
                .backend()
                .move_pointer(c.env.frame_to_desktop(next)?)?;
            self.last_aim = Some(next);
            c.vision.clear_steering_aim();
            self.progress["last_aim"] = json!(next);
            self.action("hover_reaim_only");
            self.record("hovered_aim_excluded", hovered);
            self.record("hover_alternate", alternate);
            return Ok(Flow::Next);
        }
        let current = c.vision.comparison(&f.frame)?;
        let changed = if let Some(previous) = &self.comparison {
            if previous.len() != current.len() || current.is_empty() {
                return Err(RunError::Runtime(
                    "comparison frame dimensions differ".into(),
                ));
            }
            let total: u64 = previous
                .iter()
                .zip(&current)
                .map(|(a, b)| u64::from(a.abs_diff(*b)))
                .sum();
            total as f64 / current.len() as f64 >= 3.0
        } else {
            true
        };
        if changed {
            self.last_changed_at = now;
            self.last_changed_casts = c.dispatches(0)?;
            self.comparison = Some(current);
        }
        let dispatched = c.snapshot()?;
        let movement_casts = dispatched
            .dispatches
            .get(&c.inputs.inputs().keys()[0])
            .copied()
            .unwrap_or(0);
        if (now - self.last_changed_at >= 2.0 && movement_casts > self.last_changed_casts)
            || (self
                .stationary_since
                .is_some_and(|since| now - since >= 2.0)
                && movement_casts > self.stationary_casts)
        {
            c.release()?;
            if self.recoveries >= 3 || self.last_aim.is_none() {
                self.action("stuck_hold");
                return Ok(self.pause("similar_frames_stuck"));
            }
            let failed = dispatched
                .cast_pointer
                .map(|p| c.env.desktop_to_frame(p))
                .transpose()?
                .ok_or_else(|| RunError::Runtime("failed cast pointer missing".into()))?;
            self.exclude(failed, c.vision.clearance());
            let alternate = if c.vision.has_graph() {
                c.vision.graph_fail_landing(Some(failed), f.player)?;
                self.select_route(c, f, &self.blocked())?
            } else {
                c.vision.choose(
                    &f.view,
                    c.route_wing,
                    ChooseOptions {
                        excluded: &self.blocked(),
                        heading: detour,
                        allow_detour: detour.is_none(),
                        prefer_frontier: detour.is_none(),
                        ..Default::default()
                    },
                )?
            };
            self.record("recovery_choice", &alternate);
            if alternate.aim.is_none() {
                c.release()?;
                self.record("blocked_aims", self.blocked());
                if alternate.reason == "graph_entry_retreat_exhausted"
                    && c.request.return_from.is_some()
                    && f.landmarks.hub.is_none()
                    && self.progress["route_graph_reacquired"] != true
                    && fresh(c.now(), f.acquired)
                    && c.vision
                        .reobserve_route_graph(&f.view, f.player, f.acquired)?
                {
                    // 실패 조준 제외와 원래 시한은 보존한다. 이동은 다음 새 HUD/바닥에서만 판단한다.
                    self.progress["route_graph_reacquired"] = json!(true);
                    self.recoveries += 1;
                    self.last_aim = None;
                    self.progress["last_aim"] = Value::Null;
                    self.accepted_detour = None;
                    self.last_changed_at = c.now();
                    self.last_changed_casts = movement_casts;
                    self.stationary_since = None;
                    self.stationary_casts = movement_casts;
                    self.comparison = None;
                    self.record("route_graph_reacquisition", json!({"captured_at":f.acquired,"requested_at":c.now(),"failed_aim":failed,"next_frame_required":true}));
                    self.action("route_graph_reacquire_hold");
                    return Ok(Flow::Next);
                }
                if alternate.reason == "graph_frame_stale" {
                    self.recoveries += 1;
                    c.vision.clear_steering_aim();
                    self.last_changed_at = now;
                    self.last_changed_casts = movement_casts;
                    self.stationary_since = None;
                    self.action("stuck_replan_fresh_frame");
                    return Ok(Flow::Next);
                }
                self.action("stuck_hold");
                return Ok(self.pause("stuck_no_observed_escape"));
            }
            self.recoveries += 1;
            f.choice = alternate;
            c.vision.clear_steering_aim();
            self.last_changed_at = now;
            self.stationary_since = None;
            self.record("choice", &f.choice);
            self.record("recovery", "blocked_aim_excluded");
            self.record("blocked_aims", self.blocked());
            self.retain(
                format!(
                    "continuous-{:04}-reaim.png",
                    self.offset + f.sequence as usize
                ),
                &f.frame,
            );
            c.env
                .emit("NORTH_RECOVERY: fresh observed alternate floor; blocked aim excluded");
        }
        let (steer, reason) = c.vision.steer(&f.view, &f.choice, now)?;
        if reason == "opposite_floor_recheck" {
            c.release()?;
            self.action("turn_recheck_hold");
            self.record("steering_changed", false);
            self.record("steering_reason", reason);
            return Ok(Flow::Next);
        }
        let travel_aim = if reason == "hold_current_aim" {
            c.vision.steering_aim()
        } else {
            f.choice.aim
        };
        let aim = if steer || c.snapshot()?.held.is_empty() {
            Some(c.env.frame_to_desktop(
                travel_aim.ok_or_else(|| RunError::Runtime("travel aim missing".into()))?,
            )?)
        } else {
            None
        };
        let requested = c.now();
        c.inputs.inputs().apply_observed(
            &FieldDirective::new(
                FieldAction::Travel,
                "continuous_observed_floor",
                None,
                aim,
                Some((f.acquired + 0.25).min(c.deadline)),
                false,
            )?,
            requested,
            InputObservation::new(f.sequence, f.acquired, requested)?,
        )?;
        if !c.snapshot()?.held.is_empty() {
            self.accepted_detour = c.vision.flow_state().0;
        }
        if steer {
            self.last_aim = f.choice.aim;
            self.progress["last_aim"] = json!(self.last_aim);
            if matches!(flow_reason.as_str(), "bend_entered" | "forward_reopened")
                || reason == "previous_floor_closed"
                || (c.vision.has_graph()
                    && self
                        .previous_row()
                        .is_none_or(|row| row["choice"]["reason"] != f.choice.reason))
            {
                self.recent_turns.push_back(EncounterEvidence {
                    filename: format!(
                        "continuous-{:04}-turn.png",
                        self.offset + f.sequence as usize
                    ),
                    frame: f.frame.clone(),
                });
                // Native은 최근 두 회전만 진단한다. 재생 경로의 원래 증거 계약은 유지한다.
                let limit = if c.env.evidence_streaming() { 2 } else { 8 };
                while self.recent_turns.len() > limit {
                    self.recent_turns.pop_front();
                }
            }
        }
        let after = c.snapshot()?;
        self.action(if after.held.contains(&c.inputs.inputs().keys()[0]) {
            "hold_teleport"
        } else {
            "teleport_aim_settling"
        });
        self.record("input_after", after);
        self.record("steering_changed", steer);
        self.record("steering_reason", reason);
        if f.sequence == 1 || f.sequence % 10 == 0 {
            let action = self
                .row
                .as_ref()
                .and_then(|r| r["action"].as_str())
                .unwrap_or("");
            let aim = f
                .choice
                .aim
                .map(|[x, y]| format!("({x}, {y})"))
                .unwrap_or_else(|| "None".into());
            c.env.emit(&format!(
                "NORTH_CONTINUOUS frame={} action={} age_ms={} aim={}",
                f.sequence,
                action,
                ((now - f.acquired) * 1000.0).round_ties_even(),
                aim
            ));
        }
        Ok(Flow::Next)
    }
}
