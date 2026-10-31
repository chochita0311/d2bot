use super::*;
use serde_json::{Value, json};
#[test]
fn source_main_parser_and_routing() -> Result<(), Box<dyn std::error::Error>> {
    let mut cases = vec![
        vec![],
        vec!["--help".into()],
        vec!["--he".into()],
        vec!["-h".into()],
        vec!["--bad".into(), "--help".into()],
        vec!["--".into()],
        vec!["--cli".into()],
        vec!["--live".into()],
    ];
    for (name, values) in [
        (
            "--north-casts",
            vec![
                "1",
                "12",
                "0",
                "13",
                "-1",
                "+３",
                "１_２",
                "1_",
                "1.0",
                "NaN",
                " 2 ",
                "\u{2003}２\u{2003}",
                "\u{1c}2",
                " \u{1c}2 ",
            ],
        ),
        ("--north-segments", vec!["1", "4", "5"]),
        ("--field-seconds", vec!["1", "60", "61"]),
        ("--north-cycles", vec!["1", "5", "6"]),
        ("--town-loop", vec!["1", "3", "4"]),
        (
            "--wing",
            vec!["north", "east", "south", "west", "North", "bad"],
        ),
        ("--difficulty", vec!["normal", "nightmare", "hell", "bad"]),
        ("--town-destination", vec!["town", "arcane", "canyon"]),
        ("--automap", vec!["on", "preserve", "off"]),
        ("--config", vec!["config", "한글 경로", "-1", "", "--bad"]),
    ] {
        for value in values {
            cases.push(vec![name.into(), value.into()]);
            cases.push(vec![format!("{name}={value}")]);
        }
    }
    for flags in [
        vec!["--north-casts", "2", "--north-casts", "3"],
        vec!["--town-loop", "1", "--town-loop", "2"],
        vec!["--town-loop", "1", "--summoner-run"],
        vec!["--resume-arcane", "--resume-town"],
        vec!["--resume-town", "--resume-town"],
        vec!["--list-windows", "--town-loop", "1", "--cli"],
        vec![
            "--summoner-run",
            "--live",
            "--north-cycles",
            "3",
            "--resume-arcane",
            "--return-to-hub-from",
            "east",
        ],
        vec!["--n"],
        vec!["--north-c"],
        vec!["--north-casts", "bad", "--help"],
        vec!["--cli=true"],
        vec!["--list-windows", "--config", "missing"],
        vec!["--out=한글"],
        vec!["--output=", "--continue-navigation=./foo//bar"],
        vec!["--help=true"],
        vec!["-hh"],
    ] {
        cases.push(flags.into_iter().map(str::to_owned).collect());
    }
    let expected = crate::test_support::legacy_expectation("cli-oracle", &json!(cases))?;
    for (input, expected) in cases
        .iter()
        .zip(expected.as_array().ok_or("oracle list required")?)
    {
        let actual = parse(input.iter().map(OsString::from));
        match actual {
            Ok(Command::Help) => assert_eq!(expected["exit"], 0, "{input:?}"),
            Ok(Command::Main(options)) => {
                assert_eq!(
                    serde_json::to_value(&options)?,
                    expected["arguments"],
                    "{input:?}"
                );
                let route = if options.list_windows {
                    Route::Windows
                } else if options.town_loop.is_some() || options.arcane_entry {
                    Route::Town
                } else if options.summoner_run {
                    Route::Summoner
                } else if !options.cli {
                    Route::Gui
                } else {
                    Route::Preview
                };
                assert_eq!(options.route(), route);
                let s = options.summoner_options("unused".into())?;
                assert_eq!(
                    s.resume_arcane,
                    options.resume_arcane || options.return_to_hub_from.is_some()
                );
                assert_eq!(s.casts, options.north_casts);
                assert_eq!(s.engage_monsters, !options.no_combat);
            }
            Err(_) => assert_eq!(expected["exit"], 2, "{input:?}: {expected}"),
            other => return Err(format!("unexpected main command {other:?}").into()),
        }
    }
    assert!(matches!(parse(Vec::<OsString>::new())?,Command::Main(o) if o.route()==Route::Gui));
    // Windows 경로의 단독 surrogate도 Unicode 문자열로 손실 변환하지 않는다.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        let path = OsString::from_wide(&[0xd800, 0x61]);
        let Command::Main(o) = parse([OsString::from("--config"), path.clone()])? else {
            return Err("main required".into());
        };
        assert_eq!(o.config, PathBuf::from(&path));
        let mut inline: Vec<u16> = "--config=".encode_utf16().collect();
        inline.extend([0xd800, 0x61]);
        let Command::Main(o) = parse([OsString::from_wide(&inline)])? else {
            return Err("main required".into());
        };
        assert_eq!(o.config, PathBuf::from(path));
    }
    let _: Value = expected;
    Ok(())
}
