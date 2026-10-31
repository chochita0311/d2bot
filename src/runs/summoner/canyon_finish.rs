//! 현재 일지·포탈·협곡 발판 근거. 처치와 전리품 확인은 실행 진입의 별도 조건이다.
mod execution;
mod native;
mod observers;
use super::navigation::{ArcanePathObserver, ChooseOptions, PathView, Wing};
use crate::common::error::ObservationError;
pub use execution::*;
pub use native::NativeCanyonVision;
pub use observers::{CanyonApproachObserver, RedPortalObserver};

pub fn journal_landing(
    path: &ArcanePathObserver,
    view: &PathView,
    journal: [f64; 2],
    player: [f64; 2],
) -> Result<Option<[i32; 2]>, ObservationError> {
    let mut path = path.clone();
    if let Some(mut teleport) = path.teleport() {
        teleport.preferred_distance = Some(teleport.maximum_distance);
        path.set_teleport(Some(teleport))?;
    }
    let side = if player[0] < journal[0] { -1.0 } else { 1.0 };
    for direction in [side, -side] {
        let goal = [journal[0] + direction * 160.0, journal[1] + 80.0];
        let choice = path.choose(
            view,
            Wing::North,
            ChooseOptions {
                destination: Some([goal[0] - player[0], goal[1] - player[1]]),
                ..Default::default()
            },
        )?;
        if let Some(aim) = choice.aim {
            let [x, y] = aim.map(f64::from);
            if (x - goal[0]).hypot(y - goal[1]) <= 100.0
                && (x - journal[0]).hypot(y - journal[1]) <= 220.0
                && direction * (x - journal[0]) >= 120.0
            {
                return Ok(Some(aim));
            }
        }
    }
    Ok(None)
}
