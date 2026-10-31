mod support;
use d2bot::tools::field_input_probe::options;
use serde_json::{Value, json};
use std::{error::Error, ffi::OsString};
#[test]
fn supervised_probe_parser_validates_before_native_effects() -> Result<(), Box<dyn Error>> {
    let base = [
        "--character=flash",
        "--permit-stdin",
        "--action=travel",
        "--aim",
        "100",
        "200",
        "--output=local",
    ];
    let mut cases = vec![base.iter().map(|v| v.to_string()).collect::<Vec<_>>()];
    for seconds in ["0.5", "0", "1", "1.1", "NaN", "Infinity", "-0.1", "０.５"] {
        let mut c = cases[0].clone();
        c.push(format!("--seconds={seconds}"));
        cases.push(c);
    }
    for args in [
        vec!["--help"],
        vec![
            "--character=flash",
            "--permit-stdin",
            "--action=buff",
            "--output=x",
        ],
        vec![
            "--character=flash",
            "--permit-stdin",
            "--action=buff",
            "--output=",
            "--step-after-buff=north",
            "--seconds=0.1",
        ],
        vec![
            "--character=flash",
            "--permit-stdin",
            "--action=buff",
            "--output=x",
            "--renew-once",
            "--buff-trace",
        ],
        vec![
            "--character=flash",
            "--permit=file",
            "--action=buff",
            "--output=x",
            "--renew-once",
        ],
        vec![
            "--character=other",
            "--permit-stdin",
            "--action=buff",
            "--output=x",
            "--step-after-buff=east",
            "--seconds=0.1",
        ],
        vec![
            "--character=flash",
            "--permit-stdin",
            "--action=combat",
            "--output=x",
        ],
        vec![
            "--character=flash",
            "--permit=file",
            "--permit-stdin",
            "--action=buff",
            "--output=x",
        ],
    ] {
        cases.push(args.into_iter().map(str::to_owned).collect());
    }
    let mut big = cases[0].clone();
    big[4] = "9".repeat(100);
    cases.push(big);
    for extra in ["--buff-trace", "--renew-once", "--step-after-buff=west"] {
        let mut c = cases[0].clone();
        c.push(extra.into());
        cases.push(c);
    }
    let requests = cases
        .iter()
        .map(|c| json!({"op":"probe-options","arguments":c}))
        .collect::<Vec<_>>();
    let expected = support::legacy_expectation("tools-oracle", &json!(requests))?;
    for (case, expected) in cases.iter().zip(expected.as_array().ok_or("list")?) {
        match options::parse(case.iter().map(OsString::from)) {
            Ok(None) => assert_eq!(expected["exit"], 0, "{case:?}"),
            Ok(Some(options)) => {
                assert_eq!(expected["ok"], true, "{case:?}: {expected}");
                let actual: Value = serde_json::to_value(options)?;
                assert_eq!(actual, expected["value"], "{case:?}");
            }
            Err(error) => assert_eq!(expected["exit"], 2, "{case:?}: {error}: {expected}"),
        }
    }
    Ok(())
}
