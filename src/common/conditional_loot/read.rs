use super::*;
use crate::common::survival_vision::resource_guard;
pub(super) struct ReadFrame {
    pub frame: Arc<BgrFrame>,
    pub acquired: f64,
}
impl ConditionalLootController {
    pub(super) fn active<E: ConditionalLootEnvironment>(env: &E) -> Result<(), Failure<E::Error>> {
        if env.stopped() {
            return Err(Failure::Abort("stopped"));
        }
        if !env.focused().map_err(Failure::Error)? {
            return Err(Failure::Abort("focus_lost"));
        }
        Ok(())
    }
    pub(super) fn guard<E: ConditionalLootEnvironment>(
        &self,
        env: &mut E,
        acquired: Option<f64>,
        explicit: Option<f64>,
        scan_deadline: Option<f64>,
    ) -> Result<(), Failure<E::Error>> {
        Self::active(env)?;
        let now = env.clock();
        if acquired.is_some_and(|acquired| !(0.0..0.25).contains(&(now - acquired))) {
            return Err(Failure::Abort("screen_stale"));
        }
        let supplied = env.buff_deadline().map_err(Failure::Error)?;
        if [explicit, supplied]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite())
        {
            return Err(Failure::Abort("buff_deadline_unverified"));
        }
        if explicit
            .into_iter()
            .chain(supplied)
            .any(|deadline| now >= deadline)
        {
            return Err(Failure::Abort("buff_renewal_due"));
        }
        if scan_deadline.is_some_and(|deadline| now >= deadline) {
            return Err(Failure::Abort("scan_deadline"));
        }
        Ok(())
    }
    pub(super) fn read<E: ConditionalLootEnvironment>(
        &mut self,
        env: &mut E,
        buff: Option<f64>,
        scan: Option<f64>,
        restoring: bool,
    ) -> Result<ReadFrame, Failure<E::Error>> {
        let deadline = env.clock() + 0.75;
        let mut pending = false;
        loop {
            Self::active(env)?;
            if pending {
                if !restoring {
                    self.guard(env, None, buff, scan)?;
                }
                if env.clock() >= deadline {
                    return Err(Failure::Abort("observation_unverified"));
                }
            }
            let acquired = env.clock();
            let frame = env.capture().map_err(Failure::Error)?;
            let reading = env.hud(&frame).map_err(Failure::Error)?;
            let blocked = resource_guard(&reading, &self.policy);
            // 준비 이후의 ON은 이 controller가 켠 상태다. 가린 초상은 OFF 정리에만 예외로 둔다.
            let clearing_owned_overlay = restoring
                && self.prepared
                && self.labels_state == LabelsState::On
                && blocked == Some("mercenary_unverified");
            if blocked == Some("resource_low") && env.has_recovery() {
                Self::active(env)?;
                if !(0.0..0.25).contains(&(env.clock() - acquired)) {
                    return Err(Failure::Abort("screen_stale"));
                }
                if env.panels_open(&frame).map_err(Failure::Error)? {
                    return Err(Failure::Abort("panel_open"));
                }
                self.resource_exit_required = true;
                if self.recovery_attempted {
                    return Err(Failure::Abort("resource_low"));
                }
                self.recovery_attempted = true;
                let recovered = env
                    .recover_resources(&frame, &reading, acquired)
                    .map_err(Failure::Error)?;
                self.resource_exit_required = !recovered;
                if restoring && recovered {
                    continue;
                }
                return Err(Failure::Abort(if recovered {
                    "resource_recovered"
                } else {
                    "resource_low"
                }));
            }
            if env.panels_open(&frame).map_err(Failure::Error)?
                || (!matches!(blocked, None | Some("observation_unverified"))
                    && !clearing_owned_overlay)
            {
                return Err(Failure::Abort(blocked.unwrap_or("panel_open")));
            }
            if restoring {
                Self::active(env)?;
                if !(0.0..0.25).contains(&(env.clock() - acquired)) {
                    return Err(Failure::Abort("screen_stale"));
                }
            } else {
                self.guard(env, Some(acquired), buff, scan)?;
            }
            if pending && env.clock() >= deadline {
                return Err(Failure::Abort("observation_unverified"));
            }
            if blocked.is_none() || clearing_owned_overlay {
                if clearing_owned_overlay {
                    self.overlay_hud_reacquisition_pending = true;
                    self.last_hud_failure = Some(LootFrameEvidence {
                        frame: frame.clone(),
                        metadata: HudFailure {
                            acquired_at: acquired,
                            life_ratio: reading.life.ratio(),
                            mana_ratio: reading.mana.ratio(),
                            belt_visibility: reading.belt_visibility,
                            restoring,
                            deadline,
                            recovered_at: None,
                        },
                    });
                }
                if pending && let Some(last) = &mut self.last_hud_failure {
                    last.metadata.recovered_at = Some(acquired);
                }
                return Ok(ReadFrame { frame, acquired });
            }
            self.last_hud_failure = Some(LootFrameEvidence {
                frame,
                metadata: HudFailure {
                    acquired_at: acquired,
                    life_ratio: reading.life.ratio(),
                    mana_ratio: reading.mana.ratio(),
                    belt_visibility: reading.belt_visibility,
                    restoring,
                    deadline,
                    recovered_at: None,
                },
            });
            pending = true;
            let remaining = (deadline - env.clock()).max(0.0);
            env.wait(remaining.min(0.02)).map_err(Failure::Error)?;
        }
    }
    pub(super) fn toggle<E: ConditionalLootEnvironment>(
        &mut self,
        env: &mut E,
        target: LabelsState,
        buff: Option<f64>,
        scan: Option<f64>,
        restoring: bool,
        verified: Option<f64>,
    ) -> Result<bool, Failure<E::Error>> {
        if self.labels_state == target {
            return Ok(false);
        }
        if self.labels_state == LabelsState::Unknown {
            return Err(Failure::Abort("item_labels_state_unknown"));
        }
        let acquired = if let Some(acquired) = verified {
            self.guard(env, Some(acquired), buff, scan)?;
            acquired
        } else {
            self.read(env, buff, scan, restoring)?.acquired
        };
        if restoring {
            Self::active(env)?;
        } else {
            self.guard(env, Some(acquired), buff, scan)?;
        }
        self.labels_state = LabelsState::Unknown;
        env.press("alt").map_err(Failure::Error)?;
        Self::active(env)?;
        self.labels_state = target;
        Ok(true)
    }
    pub(super) fn eligible<E: ConditionalLootEnvironment>(
        &self,
        env: &mut E,
        candidates: &[E::Candidate],
        attempted: &BTreeSet<String>,
    ) -> Result<Vec<E::Candidate>, Failure<E::Error>> {
        let count = env
            .potion_count()
            .map_err(Failure::Error)?
            .filter(Value::is_object)
            .and_then(|proof| {
                let count = proof
                    .get("count")
                    .or_else(|| proof.get("potions_remaining"))
                    .and_then(Value::as_i64)?;
                let acquired = proof
                    .get("observed_at")
                    .or_else(|| proof.get("belt_observed_at"))
                    .and_then(Value::as_f64)?;
                (proof["confirmed"] == true
                    && count >= 0
                    && acquired.is_finite()
                    && (0.0..0.25).contains(&(env.clock() - acquired)))
                .then_some(count)
            });
        Ok(candidates
            .iter()
            .filter(|candidate| {
                let label = casefold(E::candidate_label(candidate));
                if attempted.contains(&label) || self.ignored.contains(&label) || label == "gold" {
                    return false;
                }
                let Some(potion) = potion_label(&label) else {
                    return true;
                };
                self.policy.potion_pickup().is_some_and(|pickup| {
                    potion == pickup.preferred_kind()
                        || (Some(potion) == pickup.fallback_kind()
                            && count
                                .zip(pickup.fallback_at_or_below())
                                .is_some_and(|(count, limit)| count <= i64::from(limit)))
                })
            })
            .cloned()
            .collect())
    }
}
