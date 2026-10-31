use d2bot::{common::config::BotConfig, runs::summoner::options::navigation_reaction_radius};
use serde_json::json;
use std::error::Error;

#[test]
fn navigation_radius_has_reviewed_default_and_rejects_invalid_overrides()
-> Result<(), Box<dyn Error>> {
    assert_eq!(
        navigation_reaction_radius(&BotConfig::from_value(json!({}))?)?,
        [240.0, 144.0]
    );
    for value in [json!([240, 144]), json!([300, 180]), json!([1, 1])] {
        let config = BotConfig::from_value(
            json!({"run_profiles":{"summoner":{"hunting":{"combat_reaction_radius":value}}}}),
        )?;
        assert_eq!(
            navigation_reaction_radius(&config)?,
            serde_json::from_value::<[f64; 2]>(value)?
        );
    }
    for value in [
        json!([0, 144]),
        json!([-1, 144]),
        json!([301, 180]),
        json!([240, 181]),
        json!([240]),
        json!(["NaN", 144]),
        json!(null),
    ] {
        let config = BotConfig::from_value(
            json!({"run_profiles":{"summoner":{"hunting":{"combat_reaction_radius":value}}}}),
        )?;
        assert!(navigation_reaction_radius(&config).is_err());
    }
    Ok(())
}
