use super::*;
use crate::runs::summoner::{
    continuous_navigation::ContinuousVision,
    encounter_execution::{EncounterEvidence, EncounterVision},
};
use serde_json::json;
use std::time::Duration;
impl WindowsSummonerRuntime {
    pub(super) fn snapshot_records(
        &self,
        report: &mut Value,
        final_snapshot: bool,
    ) -> Result<(), RunError> {
        if let Some(dispatch) = self.driver.failed_dispatch() {
            report["automatic_buff_failure"] = serde_json::to_value(dispatch)?;
        }
        if self.driver.weapon_state_unverified() {
            report["operator_review_required"] = json!("buff_weapon_state_unverified");
            if report["automatic_buff_failure"].is_null() {
                report["automatic_buff_failure"] =
                    json!({"status":"dispatch_error","equipment_dispatch_balanced":null});
            }
        }
        if let Some(recovery) = &self.field.recovery {
            report["resource_recoveries"] = serde_json::to_value(recovery.records())?;
        }
        if let Some(loot) = &self.field.loot {
            let rows = serde_json::to_value(loot.records())?;
            report["loot_scans"] = if final_snapshot {
                rows
            } else {
                Value::Array(
                    rows.as_array()
                        .ok_or_else(|| missing("loot records"))?
                        .iter()
                        .filter(|row| row["reason"] == "buff_site_combat_ended_scan")
                        .cloned()
                        .collect(),
                )
            };
            if final_snapshot {
                report["item_labels_final"] = json!(loot.labels_state());
            }
        }
        Ok(())
    }
    pub(super) fn cleanup_native(
        &mut self,
        report: &mut Value,
        evidence: &mut Vec<EncounterEvidence>,
    ) -> Vec<String> {
        self.stop.set();
        self.gate.stop();
        let mut errors = vec![];
        // 입력부터 차단·해제하고, 중단된 관찰 작업의 종료까지 각 소유자를 보존한다.
        if let Some(core) = &self.field.core
            && let Err(e) = core.input().close()
        {
            errors.push(format!("input cleanup: {e}"));
        }
        let mut retained = vec![];
        for mut session in self.retained_inputs.drain(..) {
            if let Err(e) = session.finish(Duration::from_secs(1)) {
                errors.push(format!("field input cleanup: {e}"));
                retained.push(session);
            }
        }
        self.retained_inputs = retained;
        if let Some(vision) = &mut self.continuous
            && let Err(e) = vision.finish(Duration::from_secs(1))
        {
            errors.push(format!("navigation cleanup: {e}"));
        }
        if let Some(vision) = &mut self.encounter_vision
            && let Err(e) = vision.finish(Duration::from_secs(1))
        {
            errors.push(format!("encounter cleanup: {e}"));
        }
        if let Err(e) = self.entry.stop(Duration::from_secs(2)) {
            errors.push(format!("entry cleanup: {e}"));
        }
        if let Some(hotkeys) = &mut self.hotkeys {
            if let Err(e) = hotkeys.stop(Duration::from_secs(2)) {
                errors.push(format!("hotkey cleanup: {e}"));
            }
            match hotkeys.faults() {
                Ok(faults) => errors.extend(
                    faults
                        .into_iter()
                        .map(|f| format!("hotkey {}: {}", f.stage, f.message)),
                ),
                Err(e) => errors.push(format!("hotkey faults: {e}")),
            }
        }
        if let Some(core) = &mut self.field.core
            && let Err(e) = core.close_capture()
        {
            errors.push(format!("capture cleanup: {e}"));
        }
        if let Some(capture) = &mut self.pending_capture
            && let Err(e) = capture.close()
        {
            errors.push(format!("capture cleanup: {e}"));
        }
        if let Err(e) = self.snapshot_records(report, true) {
            errors.push(format!("report snapshot: {e}"));
        }
        // 입력/캡처/관찰 작업을 먼저 닫은 뒤 작은 진단 큐의 실제 저장 결과를 합류한다.
        if let Some(stream) = &mut self.evidence_stream {
            let result = stream.finish();
            errors.extend(
                result
                    .errors
                    .iter()
                    .map(|e| format!("navigation evidence: {e}")),
            );
            match serde_json::to_value(&result) {
                Ok(value) => report["diagnostic_evidence_stream"] = value,
                Err(e) => errors.push(format!("navigation evidence report: {e}")),
            }
        }
        if let Some(recovery) = &mut self.field.recovery {
            evidence.extend(
                recovery
                    .take_evidence()
                    .into_iter()
                    .map(|e| EncounterEvidence {
                        filename: e.filename,
                        frame: e.frame,
                    }),
            );
        }
        if let Some(loot) = &mut self.field.loot {
            if let Some(last) = loot.take_hud_failure() {
                match serde_json::to_value(last.metadata) {
                    Ok(v) => report["loot_hud_reacquisition"] = v,
                    Err(e) => errors.push(format!("loot HUD metadata: {e}")),
                }
                evidence.push(EncounterEvidence {
                    filename: "loot-hud-unverified-last.png".into(),
                    frame: last.frame,
                });
            }
            if let Some(last) = loot.take_landing_observation() {
                match serde_json::to_value(last.metadata) {
                    Ok(v) => report["loot_landing_observation"] = v,
                    Err(e) => errors.push(format!("loot landing metadata: {e}")),
                }
                evidence.push(EncounterEvidence {
                    filename: "loot-landing-last.png".into(),
                    frame: last.frame,
                });
            }
        }
        if self.camera_started {
            let camera=self.field.camera.with(|camera| {let d=camera.diagnostics();Ok(json!({"calibrations":d.calibrations,"projected_observations":d.projections,"final_calibrated_center":d.center,"final_authorized":d.authorized,"reset_events":d.reset_events}))});
            match camera {
                Ok(value) => {
                    if report["camera_projection"].is_null() {
                        report["camera_projection"] = json!({});
                    }
                    if let (Some(target), Some(values)) = (
                        report["camera_projection"].as_object_mut(),
                        value.as_object(),
                    ) {
                        target.extend(values.clone());
                    }
                }
                Err(e) => errors.push(format!("camera diagnostics: {e}")),
            }
        }
        errors
    }
}
