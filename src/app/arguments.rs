use crate::common::cli::{self, Kind, OptionSpec as Opt, UsageError};
use serde::Serialize;
use std::{ffi::OsString, path::PathBuf};
#[cfg(test)]
mod tests;

const WINGS: &[&str] = &["north", "east", "south", "west"];
const DIFFICULTIES: &[&str] = &["normal", "nightmare", "hell"];
pub const OPTIONS: &[Opt] = &[
    Opt::new("--list-windows", Kind::Flag),
    Opt::new("--config", Kind::Value),
    Opt::new("--cli", Kind::Flag),
    Opt::new("--town-loop", Kind::Range(1, 3)).group(1),
    Opt::new("--arcane-entry", Kind::Flag).group(1),
    Opt::new("--summoner-run", Kind::Flag).group(1),
    Opt::new("--live", Kind::Flag),
    Opt::new("--resume-arcane", Kind::Flag).group(2),
    Opt::new("--resume-town", Kind::Flag).group(2),
    Opt::new("--resume-labels-off", Kind::Flag),
    Opt::new("--north-casts", Kind::Range(1, 12)),
    Opt::new("--north-segments", Kind::Range(1, 4)),
    Opt::new("--continuous-north", Kind::Flag),
    Opt::new("--all-wings", Kind::Flag),
    Opt::new("--north-cycles", Kind::Range(1, 5)),
    Opt::new("--no-combat", Kind::Flag),
    Opt::new("--automap", Kind::Choice(&["on", "preserve"])),
    Opt::new("--continue-navigation", Kind::Value),
    Opt::new("--field-seconds", Kind::Range(1, 60)),
    Opt::new("--wing", Kind::Choice(WINGS)),
    Opt::new("--return-to-hub-from", Kind::Choice(WINGS)),
    Opt::new("--output", Kind::Value),
    Opt::new("--difficulty", Kind::Choice(DIFFICULTIES)),
    Opt::new("--town-destination", Kind::Choice(&["town", "arcane"])),
    Opt::new("--version", Kind::Flag),
    Opt::new("-V", Kind::Flag),
    Opt::new("--diagnose", Kind::Flag),
];
#[derive(Debug, Clone, Serialize)]
pub struct MainOptions {
    #[serde(skip)]
    pub config_explicit: bool,
    pub list_windows: bool,
    pub config: PathBuf,
    pub cli: bool,
    pub town_loop: Option<u8>,
    pub arcane_entry: bool,
    pub summoner_run: bool,
    pub live: bool,
    pub resume_arcane: bool,
    pub resume_town: bool,
    pub resume_labels_off: bool,
    pub north_casts: u8,
    pub north_segments: u8,
    pub continuous_north: bool,
    pub all_wings: bool,
    pub north_cycles: Option<u8>,
    pub no_combat: bool,
    pub automap: String,
    pub continue_navigation: Option<PathBuf>,
    pub field_seconds: u8,
    pub wing: String,
    pub return_to_hub_from: Option<String>,
    pub output: Option<PathBuf>,
    pub difficulty: String,
    pub town_destination: String,
}
#[derive(Debug)]
pub enum Command {
    Help,
    Version,
    Diagnose,
    Main(Box<MainOptions>),
}
pub fn parse(input: impl IntoIterator<Item = OsString>) -> Result<Command, UsageError> {
    let p = cli::parse(input, OPTIONS, &[])?;
    if p.help {
        return Ok(Command::Help);
    }
    if p.has("--version") || p.has("-V") {
        return Ok(Command::Version);
    }
    if p.has("--diagnose") {
        return Ok(Command::Diagnose);
    }
    let text = |key, default: &str| -> Result<String, UsageError> {
        Ok(p.text(key)?.unwrap_or(default).into())
    };
    Ok(Command::Main(Box::new(MainOptions {
        config_explicit: p.has("--config"),
        list_windows: p.has("--list-windows"),
        config: p
            .raw("--config")
            .map(PathBuf::from)
            .unwrap_or_else(|| "config".into()),
        cli: p.has("--cli"),
        town_loop: p.small("--town-loop").map(|v| v as u8),
        arcane_entry: p.has("--arcane-entry"),
        summoner_run: p.has("--summoner-run"),
        live: p.has("--live"),
        resume_arcane: p.has("--resume-arcane"),
        resume_town: p.has("--resume-town"),
        resume_labels_off: p.has("--resume-labels-off"),
        north_casts: p.small("--north-casts").unwrap_or(3) as u8,
        north_segments: p.small("--north-segments").unwrap_or(1) as u8,
        continuous_north: p.has("--continuous-north"),
        all_wings: p.has("--all-wings"),
        north_cycles: p.small("--north-cycles").map(|v| v as u8),
        no_combat: p.has("--no-combat"),
        automap: text("--automap", "on")?,
        continue_navigation: p.path("--continue-navigation"),
        field_seconds: p.small("--field-seconds").unwrap_or(20) as u8,
        wing: text("--wing", "north")?,
        return_to_hub_from: p.text("--return-to-hub-from")?.map(str::to_owned),
        output: p.path("--output"),
        difficulty: text("--difficulty", "hell")?,
        town_destination: text("--town-destination", "town")?,
    })))
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Windows,
    Town,
    Summoner,
    Gui,
    Preview,
}
impl MainOptions {
    pub fn route(&self) -> Route {
        if self.list_windows {
            Route::Windows
        } else if self.town_loop.is_some() || self.arcane_entry {
            Route::Town
        } else if self.summoner_run {
            Route::Summoner
        } else if !self.cli {
            Route::Gui
        } else {
            Route::Preview
        }
    }
    pub fn summoner_options(
        &self,
        output: PathBuf,
    ) -> Result<crate::runs::summoner::options::SummonerOptions, UsageError> {
        use crate::{
            actions::town_loop::Difficulty,
            runs::summoner::{
                navigation::Wing,
                options::{AutomapPreference, SummonerOptions},
            },
        };
        let difficulty = match self.difficulty.as_str() {
            "normal" => Difficulty::Normal,
            "nightmare" => Difficulty::Nightmare,
            _ => Difficulty::Hell,
        };
        let wing = |value: &str| match value {
            "east" => Wing::East,
            "south" => Wing::South,
            "west" => Wing::West,
            _ => Wing::North,
        };
        let mut o = SummonerOptions::new(output, difficulty);
        o.live = self.live;
        o.resume_arcane = self.resume_arcane || self.return_to_hub_from.is_some();
        o.resume_town = self.resume_town;
        o.resume_labels_off = self.resume_labels_off;
        o.casts = self.north_casts;
        o.segments = self.north_segments;
        o.continuous = self.continuous_north
            || self.all_wings
            || self.north_cycles.is_some()
            || self.return_to_hub_from.is_some()
            || self.no_combat
            || self.wing != "north";
        o.all_wings = self.all_wings;
        o.wing = wing(&self.wing);
        o.return_from = self.return_to_hub_from.as_deref().map(wing);
        o.engage_monsters = !self.no_combat;
        o.automap = if self.automap == "preserve" {
            AutomapPreference::Preserve
        } else {
            AutomapPreference::On
        };
        o.continue_navigation = self.continue_navigation.clone();
        o.field_seconds = self.field_seconds;
        Ok(o)
    }
}
