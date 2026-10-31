#![deny(unsafe_code)]

pub mod actions;
pub mod app;
pub mod common;
pub mod core;
pub mod platform;
pub mod runs;
pub mod tools;
pub mod ui;

#[cfg(test)]
#[path = "../tests/support/field_world.rs"]
pub(crate) mod test_field_world;
#[cfg(test)]
#[path = "../tests/support/mod.rs"]
pub(crate) mod test_support;
