#[path = "support/directory.rs"]
mod directory;
use std::{error::Error, process::Command};
#[test]
fn executable_routes_help_dry_and_errors_without_capture_or_input() -> Result<(), Box<dyn Error>> {
    let directory = directory::TestDirectory::new()?;
    let config = directory.path().join("config.json");
    std::fs::write(&config, b"{}")?;
    for (args, expected) in [
        (vec!["--version"], 0),
        (vec!["--help"], 0),
        (vec!["--diagnose"], 0),
        (vec!["--unknown"], 2),
        (vec!["--town-loop=0"], 2),
        (vec!["--town-loop=1", "--summoner-run"], 2),
        (vec!["field-input-probe", "--help"], 0),
        (vec!["field-replay", "--help"], 0),
        (vec!["survival-observe", "--help"], 0),
        (vec!["arcane-navigation-replay", "--help"], 0),
        (vec!["town-loop-vision", "--help"], 0),
        (vec!["field-replay"], 2),
        (vec!["--config=absent-config-that-does-not-exist"], 1),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_d2bot"))
            .args(&args)
            .output()?;
        assert_eq!(
            output.status.code(),
            Some(expected),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if expected == 0 {
            assert!(!output.stdout.is_empty());
        } else {
            assert!(!output.stderr.is_empty());
        }
    }
    let output = Command::new(env!("CARGO_BIN_EXE_d2bot"))
        .args(["--summoner-run", "--north-cycles=1", "--config"])
        .arg(&config)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8(output.stdout)?.starts_with("DRY: north end"));
    let output = Command::new(env!("CARGO_BIN_EXE_d2bot"))
        .args([
            "--list-windows",
            "--config=absent-config-that-does-not-exist",
        ])
        .output()?;
    assert!(output.status.success());
    assert!(
        String::from_utf8(output.stdout)?
            .lines()
            .all(|line| line.contains(" | left=") && line.contains(" height="))
    );
    Ok(())
}
