use super::*;
impl ConditionalLootController {
    pub fn after_combat<E: ConditionalLootEnvironment>(
        &mut self,
        env: &mut E,
        buff: Option<f64>,
        reason: &str,
        combat_id: Option<Value>,
    ) -> Result<LootScanReport, E::Error>
    where
        E::Error: Display,
    {
        self.recovery_attempted = false;
        let mut report = LootScanReport::new(reason);
        if combat_id.as_ref().is_some_and(|id| {
            self.combat_ids
                .iter()
                .any(|previous| same_combat_id(previous, id))
        }) {
            report.status = "combat_scan_already_handled".into();
            report.labels_state = Some(self.labels_state);
            report.movement_allowed = Some(self.movement_allowed());
            return Ok(report);
        }
        if let Some(id) = combat_id {
            self.combat_ids.push_back(id);
            while self.combat_ids.len() > 32 {
                self.combat_ids.pop_front();
            }
        }
        let deadline = env.clock() + self.limits.scan_seconds;
        match self.scan(env, buff, deadline, &mut report) {
            Ok(()) => {}
            Err(Failure::Abort(reason)) => {
                report.status = reason.into();
                if ["stopped", "focus_lost"].contains(&reason) {
                    self.labels_state = LabelsState::Unknown;
                }
            }
            Err(Failure::Error(error)) => {
                report.status = "scan_error".into();
                report.error = Some(error.to_string());
            }
        }
        if self.labels_state == LabelsState::On {
            match self.toggle(env, LabelsState::Off, None, None, true, None) {
                Ok(sent) => report.labels_off_sent = Some(sent),
                Err(error) => {
                    if matches!(error, Failure::Abort("stopped" | "focus_lost")) {
                        self.labels_state = LabelsState::Unknown;
                    }
                    report.restore_error = Some(error.message());
                }
            }
        }
        if self.overlay_hud_reacquisition_pending && self.labels_state == LabelsState::Off {
            let current = (|| {
                Self::active(env)?;
                env.wait(0.05).map_err(Failure::Error)?;
                self.read(env, None, None, true)
            })();
            match current {
                Ok(current) => {
                    self.overlay_hud_reacquisition_pending = false;
                    report.overlay_hud_reacquired_at = Some(current.acquired);
                    if let Some(last) = &mut self.last_hud_failure {
                        last.metadata.recovered_at = Some(current.acquired);
                    }
                    if report.status == "mercenary_unverified" {
                        report.status = "inspection_interrupted".into();
                        report.bounded_reason = Some("mercenary_reacquired_after_labels_off");
                    }
                }
                Err(error) => {
                    if matches!(error, Failure::Abort("stopped" | "focus_lost")) {
                        self.labels_state = LabelsState::Unknown;
                    }
                    report.restore_error = Some(error.message());
                }
            }
        }
        if !report.clicks.is_empty() && report.status == "inspection_complete" {
            report.status = "click_unconfirmed".into();
        }
        report.labels_state = Some(self.labels_state);
        report.movement_allowed = Some(self.movement_allowed());
        report.ended_at = Some(env.clock());
        self.records.push(report.clone());
        if self.resource_exit_required {
            return Err(external(Failure::Abort("resource_low")));
        }
        Ok(report)
    }
    fn scan<E: ConditionalLootEnvironment>(
        &mut self,
        env: &mut E,
        buff: Option<f64>,
        deadline: f64,
        report: &mut LootScanReport,
    ) -> Result<(), Failure<E::Error>> {
        if !self.movement_allowed() {
            return Err(Failure::Abort("item_labels_state_unknown"));
        }
        self.guard(env, None, buff, Some(deadline))?;
        report.labels_on_sent =
            Some(self.toggle(env, LabelsState::On, buff, Some(deadline), false, None)?);
        env.wait(0.05).map_err(Failure::Error)?;
        let candidates = env.build_candidates().map_err(Failure::Error)?;
        report.configured_labels = Some(
            candidates
                .iter()
                .map(|candidate| E::candidate_label(candidate).to_owned())
                .collect(),
        );
        if let Some(pickup) = self.policy.potion_pickup() {
            let available: BTreeSet<_> = candidates
                .iter()
                .filter_map(|candidate| potion_label(&casefold(E::candidate_label(candidate))))
                .collect();
            report.unavailable_potion_kinds = Some(
                std::iter::once(pickup.preferred_kind())
                    .chain(pickup.fallback_kind())
                    .filter(|kind| !available.contains(kind))
                    .map(str::to_owned)
                    .collect(),
            );
        }
        let mut attempted = BTreeSet::new();
        let mut quiet = 0;
        while report.scan_frames < self.limits.scan_frames {
            let current = self.read(env, buff, Some(deadline), false)?;
            let allowed = self.eligible(env, &candidates, &attempted)?;
            report.scan_frames += 1;
            let hit = if allowed.is_empty() {
                None
            } else {
                env.scan_frame(&current.frame, &allowed)
                    .map_err(Failure::Error)?
            };
            report.scan_observations.push(ScanObservation {
                acquired_at: current.acquired,
                processed_at: env.clock(),
                label: hit.as_ref().map(|hit| hit.label.clone()),
            });
            self.guard(env, Some(current.acquired), buff, Some(deadline))?;
            let Some(hit) = hit else {
                quiet += 1;
                if quiet >= 2 {
                    report.status = "inspection_complete".into();
                    report.bounded_reason = Some("two_frames_without_eligible_candidate");
                    return Ok(());
                }
                env.wait(0.05).map_err(Failure::Error)?;
                continue;
            };
            quiet = 0;
            if report.clicks.len() >= self.limits.clicks {
                report.status = "inspection_complete".into();
                report.bounded_reason = Some("click_limit");
                return Ok(());
            }
            if !env.supports_click_confirmation() {
                return Err(Failure::Abort("loot_click_backend_unavailable"));
            }
            let center = hit
                .center()
                .ok_or(Failure::Abort("loot_aim_outside_capture"))?;
            let size = current.frame.size();
            if center[0] < 0
                || center[1] < 0
                || center[0] as u64 >= size.width() as u64
                || center[1] as u64 >= size.height() as u64
            {
                return Err(Failure::Abort("loot_aim_outside_capture"));
            }
            let aim = env.frame_to_desktop(center).map_err(Failure::Error)?;
            env.move_pointer(aim).map_err(Failure::Error)?;
            let prepared_at = env.clock();
            let fresh = self.read(env, buff, Some(deadline), false)?;
            let reconfirm: Vec<_> = self
                .eligible(env, &candidates, &attempted)?
                .into_iter()
                .filter(|candidate| E::candidate_label(candidate) == hit.label)
                .collect();
            let current = if reconfirm.is_empty() {
                None
            } else {
                env.scan_frame(&fresh.frame, &reconfirm)
                    .map_err(Failure::Error)?
            };
            self.guard(env, Some(fresh.acquired), buff, Some(deadline))?;
            if !confirmed(&hit.label, center, current.as_ref()) {
                attempted.insert(casefold(&hit.label));
                continue;
            }
            if !env.pointer_matches(aim).map_err(Failure::Error)? {
                return Err(Failure::Abort("loot_aim_unverified"));
            }
            self.guard(env, Some(fresh.acquired), buff, Some(deadline))?;
            let index = report.teleports.len();
            report.teleports.push(LootApproach {
                label: hit.label.clone(),
                aim,
                requested_at: None,
                dispatch_returned: false,
                landing_observation: None,
                observed_after_at: None,
                player: None,
                item_center: None,
                status: None,
            });
            self.toggle(
                env,
                LabelsState::Off,
                buff,
                Some(deadline),
                false,
                Some(fresh.acquired),
            )?;
            let remaining = (prepared_at + 0.05 - env.clock()).max(0.0);
            if remaining != 0.0 {
                env.wait(remaining).map_err(Failure::Error)?;
            }
            self.guard(env, Some(fresh.acquired), buff, Some(deadline))?;
            if !env.pointer_matches(aim).map_err(Failure::Error)? {
                return Err(Failure::Abort("loot_aim_unverified"));
            }
            report.teleports[index].requested_at = Some(env.clock());
            env.press(&self.movement_key).map_err(Failure::Error)?;
            report.teleports[index].dispatch_returned = true;
            env.wait(self.limits.cast_reference_seconds)
                .map_err(Failure::Error)?;
            self.toggle(env, LabelsState::On, buff, Some(deadline), false, None)?;
            let landed = self.read(env, buff, Some(deadline), false)?;
            let current = if reconfirm.is_empty() {
                None
            } else {
                env.scan_frame(&landed.frame, &reconfirm)
                    .map_err(Failure::Error)?
            };
            let ground_available = env.ground_available().map_err(Failure::Error)?;
            let mut landing = LandingObservation {
                acquired_at: landed.acquired,
                processed_at: None,
                label: current.as_ref().map(|hit| hit.label.clone()),
                item_center: current.as_ref().and_then(LootLabelHit::center),
                player: None,
                ground_reason: None,
                missing: vec!["ground_observer"],
                status: "unverified",
            };
            self.record_landing(&landed.frame, &landing, &mut report.teleports[index]);
            if !ground_available {
                return Err(Failure::Abort("loot_landing_unverified"));
            }
            let ground = env
                .observe_ground(&landed.frame, landed.acquired)
                .map_err(Failure::Error)?;
            landing.processed_at = Some(env.clock());
            landing.player = ground.point;
            landing.ground_reason = ground.reason;
            landing.missing = if current.is_none() {
                vec!["label"]
            } else {
                vec![]
            };
            if ground.point.is_none() {
                landing.missing.push("player");
            }
            landing.status = if current.is_none() || ground.point.is_none() {
                "unverified"
            } else {
                "proximity_pending"
            };
            self.record_landing(&landed.frame, &landing, &mut report.teleports[index]);
            self.guard(env, Some(landed.acquired), buff, Some(deadline))?;
            let Some((current, player)) = current.zip(ground.point) else {
                return Err(Failure::Abort("loot_landing_unverified"));
            };
            let center = current
                .center()
                .ok_or(Failure::Abort("loot_landing_not_near_item"))?;
            if ((center[0] as f64 - f64::from(player[0])) / 300.0)
                .hypot((center[1] as f64 - f64::from(player[1])) / 180.0)
                > 1.0
            {
                landing.status = "not_near_item";
                self.record_landing(&landed.frame, &landing, &mut report.teleports[index]);
                return Err(Failure::Abort("loot_landing_not_near_item"));
            }
            landing.status = "current_item_near_player";
            self.record_landing(&landed.frame, &landing, &mut report.teleports[index]);
            let approach = &mut report.teleports[index];
            approach.observed_after_at = Some(landed.acquired);
            approach.player = Some(player);
            approach.item_center = Some(center);
            approach.status = Some("current_item_near_player");
            let aim = env.frame_to_desktop(center).map_err(Failure::Error)?;
            env.move_pointer(aim).map_err(Failure::Error)?;
            let fresh = self.read(env, buff, Some(deadline), false)?;
            let current = if reconfirm.is_empty() {
                None
            } else {
                env.scan_frame(&fresh.frame, &reconfirm)
                    .map_err(Failure::Error)?
            };
            self.guard(env, Some(fresh.acquired), buff, Some(deadline))?;
            if !confirmed(&hit.label, center, current.as_ref()) {
                return Err(Failure::Abort("loot_label_changed_after_teleport"));
            }
            if !env.pointer_matches(aim).map_err(Failure::Error)? {
                return Err(Failure::Abort("loot_aim_unverified"));
            }
            self.guard(env, Some(fresh.acquired), buff, Some(deadline))?;
            let index = report.clicks.len();
            report.clicks.push(LootClick {
                label: hit.label.clone(),
                aim,
                requested_at: env.clock(),
                status: "click_unconfirmed",
                pickup_confirmed: false,
                dispatch_returned: false,
                observed_after_at: None,
                label_visible_after: None,
            });
            env.click().map_err(Failure::Error)?;
            report.clicks[index].dispatch_returned = true;
            attempted.insert(casefold(&hit.label));
            let after = self.read(env, buff, Some(deadline), false)?;
            let visible = if reconfirm.is_empty() {
                None
            } else {
                env.scan_frame(&after.frame, &reconfirm)
                    .map_err(Failure::Error)?
            };
            self.guard(env, Some(after.acquired), buff, Some(deadline))?;
            report.clicks[index].observed_after_at = Some(after.acquired);
            report.clicks[index].label_visible_after = Some(visible.is_some());
        }
        report.status = "inspection_complete".into();
        report.bounded_reason = Some("frame_limit");
        Ok(())
    }
    fn record_landing(
        &mut self,
        frame: &Arc<BgrFrame>,
        landing: &LandingObservation,
        approach: &mut LootApproach,
    ) {
        approach.landing_observation = Some(landing.clone());
        self.last_landing_observation = Some(LootFrameEvidence {
            frame: frame.clone(),
            metadata: landing.clone(),
        });
    }
}
fn confirmed(label: &str, center: [i64; 2], current: Option<&LootLabelHit>) -> bool {
    current.is_some_and(|current| {
        current.label == label
            && current.center().is_some_and(|point| {
                (center[0] as f64 - point[0] as f64).hypot(center[1] as f64 - point[1] as f64)
                    <= 18.0
            })
            && current.contains(center)
    })
}

// Python의 동등성은 1, 1.0, True를 같은 식별자로 본다. 큰 정수는 f64로 뭉개지 않는다.
fn same_combat_id(a: &Value, b: &Value) -> bool {
    enum Numeric {
        Integer(String),
        Float(f64),
    }
    fn numeric(value: &Value) -> Option<Numeric> {
        match value {
            Value::Bool(value) => Some(Numeric::Integer(if *value { "1" } else { "0" }.into())),
            Value::Number(value) => {
                let text = value.to_string();
                if text.contains(['.', 'e', 'E']) {
                    text.parse().ok().map(Numeric::Float)
                } else {
                    Some(Numeric::Integer(if text == "-0" {
                        "0".into()
                    } else {
                        text
                    }))
                }
            }
            _ => None,
        }
    }
    match (numeric(a), numeric(b)) {
        (Some(Numeric::Integer(a)), Some(Numeric::Integer(b))) => return a == b,
        (Some(Numeric::Float(a)), Some(Numeric::Float(b))) => return a == b,
        (Some(Numeric::Integer(integer)), Some(Numeric::Float(float)))
        | (Some(Numeric::Float(float)), Some(Numeric::Integer(integer))) => {
            if !float.is_finite() || float.fract() != 0.0 {
                return false;
            }
            return integer
                == if float == 0.0 {
                    "0".into()
                } else {
                    format!("{float:.0}")
                };
        }
        _ => {}
    }
    match (a, b) {
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_combat_id(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| same_combat_id(a, b)))
        }
        _ => a == b,
    }
}
