use crate::common::cli::{self, Arguments, Kind, OptionSpec as Opt, UsageError};
use std::{ffi::OsString, path::PathBuf};
pub const HELP: &str = "Usage: d2bot field-input-probe --character KEY (--permit JSON | --permit-stdin) --action travel|combat|buff --output NEW_DIRECTORY [--live] [--config PATH] [--seconds 0.5] [--aim X Y] [--buff-trace] [--renew-once] [--step-after-buff north|east|south|west]\nExplicitly enabled bounded supervised field input probe.\n";
const OPTIONS: &[Opt] = &[
    Opt::new("--live", Kind::Flag),
    Opt::new("--config", Kind::Value),
    Opt::new("--character", Kind::Value).required(),
    Opt::new("--permit", Kind::Value).group(1),
    Opt::new("--permit-stdin", Kind::Flag).group(1),
    Opt::new("--action", Kind::Choice(&["travel", "combat", "buff"])).required(),
    Opt::new("--buff-trace", Kind::Flag),
    Opt::new("--renew-once", Kind::Flag),
    Opt::new("--seconds", Kind::Float),
    Opt::new(
        "--step-after-buff",
        Kind::Choice(&["north", "east", "south", "west"]),
    ),
    Opt::new("--aim", Kind::Pair),
    Opt::new("--output", Kind::Value).required(),
];
#[derive(serde::Serialize)]
pub struct Options {
    #[serde(skip)]
    pub config_explicit: bool,
    pub live: bool,
    pub config: PathBuf,
    pub character: String,
    pub permit: Option<PathBuf>,
    pub permit_stdin: bool,
    pub action: String,
    pub buff_trace: bool,
    pub renew_once: bool,
    pub seconds: f64,
    pub step_after_buff: Option<String>,
    pub aim: Option<[serde_json::Number; 2]>,
    pub output: PathBuf,
}
pub fn parse(input: impl IntoIterator<Item = OsString>) -> Result<Option<Options>, UsageError> {
    let args = cli::parse(input, OPTIONS, &[1])?;
    if args.help {
        return Ok(None);
    }
    Ok(Some(Options::from_args(args)?))
}
impl Options {
    fn from_args(args: Arguments) -> Result<Self, UsageError> {
        let required = |name: &str| -> Result<String, UsageError> {
            args.text(name)?
                .map(str::to_owned)
                .ok_or_else(|| UsageError(format!("{name} required")))
        };
        let action = required("--action")?;
        let character = required("--character")?;
        let trace = args.has("--buff-trace");
        let renewal = args.has("--renew-once");
        let stdin = args.has("--permit-stdin");
        let seconds = args.float("--seconds").unwrap_or(0.5);
        let step = args.text("--step-after-buff")?.map(str::to_owned);
        let aim = args.pair_numbers("--aim");
        if trace && action != "buff" {
            return Err(UsageError("--buff-trace requires --action buff".into()));
        }
        if renewal && (action != "buff" || !stdin) {
            return Err(UsageError(
                "--renew-once requires --action buff and --permit-stdin".into(),
            ));
        }
        if step.is_some()
            && (action != "buff" || !stdin || character != "flash" || renewal || seconds > 0.1)
        {
            return Err(UsageError("--step-after-buff requires Flash buff/stdin mode, no renewal, and at most 0.1 seconds".into()));
        }
        if !(seconds > 0.0 && seconds <= 1.0) || (action != "buff" && aim.is_none()) {
            return Err(UsageError(
                "hold requires aim and duration in (0,1] seconds".into(),
            ));
        }
        Ok(Self {
            config_explicit: args.has("--config"),
            live: args.has("--live"),
            config: args.path("--config").unwrap_or_else(|| "config".into()),
            character,
            permit: args.path("--permit"),
            permit_stdin: stdin,
            action,
            buff_trace: trace,
            renew_once: renewal,
            seconds,
            step_after_buff: step,
            aim,
            output: args
                .path("--output")
                .ok_or_else(|| UsageError("--output required".into()))?,
        })
    }
}
