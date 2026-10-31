//! 현재 메뉴와 커서 확인을 거치는 필드 종료 및 마을 복귀.
mod safe_exit;
mod town_return;
pub use safe_exit::{ExitEnvironment, MenuVision, exit_to_character};
pub use town_return::{NativeTownFinish, TownFinishRuntime, return_to_act1_and_exit};
