use crate::{
    actions::town_loop::{TownSight, TownVision},
    common::{error::ControlError, field_control::DesktopPoint, frame::BgrFrame},
    runs::{
        RunError,
        summoner::{
            canyon_finish::CanyonBackend, encounter_execution::EncounterEvidence,
            navigation_execution::StepEnvironment,
        },
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
#[cfg(test)]
mod tests;
pub trait ExitEnvironment {
    fn capture(&mut self) -> Result<Arc<BgrFrame>, RunError>;
    fn frame_to_desktop(&self, point: [f64; 2]) -> Result<DesktopPoint, ControlError>;
}
pub trait MenuVision {
    fn find(&mut self, frame: &Arc<BgrFrame>, name: &str) -> Result<Option<[f64; 2]>, RunError>;
}
impl MenuVision for TownVision {
    fn find(&mut self, frame: &Arc<BgrFrame>, name: &str) -> Result<Option<[f64; 2]>, RunError> {
        self.set_frame(frame.clone());
        self.normalize()?;
        Ok(TownSight::find(self, name)?.map(|p| p.point()))
    }
}
fn guard(g: &impl StepEnvironment, acquired: Option<f64>) -> Result<(), RunError> {
    if g.stopped() || !g.focused()? {
        return Err(RunError::Runtime(
            "safe exit interrupted or unfocused".into(),
        ));
    }
    if acquired.is_some_and(|at| !(0.0..0.25).contains(&(g.clock() - at))) {
        return Err(RunError::Runtime("safe exit frame stale".into()));
    }
    Ok(())
}
struct Reader<'a, G, E, V> {
    guards: &'a G,
    env: &'a mut E,
    vision: &'a mut V,
    evidence: &'a mut Vec<EncounterEvidence>,
    retained: Option<usize>,
    diagnostics: &'a mut Value,
}
struct MenuFrame {
    frame: Arc<BgrFrame>,
    acquired: f64,
    found: Option<[f64; 2]>,
}
impl<G: StepEnvironment, E: ExitEnvironment, V: MenuVision> Reader<'_, G, E, V> {
    fn read(&mut self, name: &str) -> Result<MenuFrame, RunError> {
        guard(self.guards, None)?;
        let acquired = self.guards.clock();
        let frame = self.env.capture()?;
        let item = EncounterEvidence {
            filename: "safe-exit-last.png".into(),
            frame: frame.clone(),
        };
        if let Some(at) = self.retained {
            self.evidence[at] = item;
        } else {
            self.retained = Some(self.evidence.len());
            self.evidence.push(item);
        }
        let found = self.vision.find(&frame, name)?;
        self.diagnostics["observations"] =
            json!(self.diagnostics["observations"].as_u64().unwrap_or(0) + 1);
        self.diagnostics["last_acquired_at"] = json!(acquired);
        self.diagnostics["phase"] = json!(name);
        guard(self.guards, Some(acquired))?;
        Ok(MenuFrame {
            frame,
            acquired,
            found,
        })
    }
}
pub fn exit_to_character(
    backend: &impl CanyonBackend,
    g: &impl StepEnvironment,
    env: &mut impl ExitEnvironment,
    vision: &mut impl MenuVision,
    evidence: &mut Vec<EncounterEvidence>,
    diagnostics: &mut Value,
) -> Result<(), RunError> {
    let started = g.clock();
    diagnostics["started_at"] = json!(started);
    diagnostics["observations"] = json!(0);
    guard(g, None)?;
    let menu_deadline = started + 8.0;
    diagnostics["escape_requested_at"] = json!(g.clock());
    backend.press("esc")?;
    let mut reader = Reader {
        guards: g,
        env,
        vision,
        evidence,
        retained: None,
        diagnostics,
    };
    let mut prepared: Option<(DesktopPoint, f64)> = None;
    while g.clock() < menu_deadline {
        if let Some((_, at)) = prepared {
            let remaining = (at + 0.05).min(menu_deadline) - g.clock();
            if remaining > 0.0 {
                g.wait(remaining)?;
            }
        }
        let MenuFrame {
            frame,
            acquired,
            found,
        } = reader.read("save_exit")?;
        if g.clock() >= menu_deadline {
            break;
        }
        let Some(found) = found else {
            g.wait(0.05)?;
            continue;
        };
        let point = [
            (found[0] * frame.size().width() as f64 / 1267.0).round_ties_even(),
            (found[1] * frame.size().height() as f64 / 753.0).round_ties_even(),
        ];
        let aim = reader.env.frame_to_desktop(point)?;
        if prepared.is_none_or(|(p, _)| {
            (aim[0] as f64 - p[0] as f64).hypot(aim[1] as f64 - p[1] as f64) > 3.0
        }) {
            guard(g, Some(acquired))?;
            backend.move_pointer(aim)?;
            let at = g.clock();
            prepared = Some((aim, at));
            reader.diagnostics["menu_point"] = json!(point);
            reader.diagnostics["pointer_requested_at"] = json!(at);
            continue;
        }
        if prepared.is_some_and(|(_, at)| acquired - at < 0.05) {
            continue;
        }
        if !backend.pointer_matches(aim)? {
            return Err(RunError::Runtime("safe exit pointer unverified".into()));
        }
        guard(g, Some(acquired))?;
        reader.diagnostics["click_requested_at"] = json!(g.clock());
        backend.click()?;
        break;
    }
    if reader.diagnostics.get("click_requested_at").is_none() {
        return Err(RunError::Runtime(
            "safe exit state unverified: save_exit".into(),
        ));
    }
    let selection_deadline = g.clock() + 20.0;
    while g.clock() < selection_deadline {
        let MenuFrame {
            acquired, found, ..
        } = reader.read("play")?;
        if found.is_some() && g.clock() < selection_deadline {
            reader.diagnostics["status"] = json!("character_select_observed");
            reader.diagnostics["selected_at"] = json!(acquired);
            return Ok(());
        }
        g.wait(0.05)?;
    }
    Err(RunError::Runtime("safe exit state unverified: play".into()))
}
