//! 실행 소유자와 Windows 효과의 조합. 모의 효과 검증은 게임에 접근하지 않는다.
mod automatic;
mod effects;
mod field;
mod loot;
mod paused;
#[cfg(windows)]
mod rooms;
#[cfg(windows)]
mod runtime;
mod town_backend;
mod world;
pub use automatic::{AutomaticField, BuffRecipe, observe_runtime};
pub use effects::{RecordedInput, WindowGuard};
pub use field::{FieldCore, FieldEmitter, FieldGuards, FieldServices, PanelObservers};
pub use loot::FieldLoot;
pub use paused::{EncounterRecheck, PausedField};
#[cfg(windows)]
pub use rooms::execute;
#[cfg(windows)]
pub use runtime::{RuntimeContext, WindowsSummonerRuntime};
#[cfg(windows)]
pub fn execute_once(
    context: RuntimeContext,
    options: &crate::runs::summoner::options::SummonerOptions,
) -> Result<i32, crate::runs::RunError> {
    let _compute = crate::common::compute::ComputeLease::acquire()?;
    use crate::runs::summoner::main_run::{RunFiles, execute_prepared};
    let selected = options.selected(&context.config)?;
    let emit = context.emit.clone();
    let mut runtime = WindowsSummonerRuntime::load(context, options, &selected)?;
    if !options.live {
        emit(&options.dry_message());
        return Ok(0);
    }
    let mut output = RunFiles::create(options.output.clone())?;
    runtime.prepare_live()?;
    execute_prepared(&mut runtime, &mut output, options, &selected, |message| {
        emit(message)
    })
}
pub use town_backend::BorrowedTownBackend;
pub use world::{FieldWorld, LootContext, MapScreen, TargetProbe};
