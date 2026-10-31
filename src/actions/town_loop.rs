//! One observed town action at a time. Planning never sends desktop input.
mod planner;
mod state;
mod vision;

pub use planner::{TownPlanner, TownSight};
pub use state::{
    Destination, Difficulty, InspectionStatus, TownAction, TownActionKind, TownOptions, TownPhase,
    TownPoint, TownState,
};
pub const REFERENCE_SIZE: [usize; 2] = [1267, 753];
pub use vision::{BluePair, TownVision};
