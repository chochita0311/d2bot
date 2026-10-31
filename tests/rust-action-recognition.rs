use d2bot::{
    actions::{
        gem_summing::{self, FrameCounts, GemVision},
        loot_pickup::{self, LootCandidate, LootScanner},
    },
    common::{
        config::BotConfig,
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
};
use serde_json::{Value, json};
use std::{collections::BTreeSet, error::Error, path::Path, sync::Arc};
mod support;
fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|n| format!("{n:02x}")).collect()
}
fn image_json(frame: &BgrFrame) -> Value {
    json!({"width":frame.size().width(),"height":frame.size().height(),"bgr":hex(frame.pixels())})
}
fn put(frame: &mut BgrFrame, image: &BgrFrame, x: usize, y: usize) {
    for row in 0..image.size().height() {
        let from = row * image.size().width() * 3;
        let to = ((y + row) * frame.size().width() + x) * 3;
        frame.pixels_mut()[to..to + image.size().width() * 3]
            .copy_from_slice(&image.pixels()[from..from + image.size().width() * 3]);
    }
}
#[test]
fn loot_config_candidate_order_dedup_and_cache_invalidation_match_python()
-> Result<(), Box<dyn Error>> {
    let path = "assets/items/keys/key-of-hate-on-the-ground.png";
    let config = BotConfig::from_value(
        json!({"shared_loot":{"fixed_items":[{"label":"first","ground_template":path},{"label":"first","ground_template":path},{"label":"absent","ground_template":""}]},"farm":{"templates":[{"name":"first","path":path},{"name":"icon","path":"none.png","context":"inventory"},{"name":"GROUND_rune","path":path,"context":"inventory"},{"name":"another","path":path,"context":"GrOuNd","label":""}]}}),
    )?;
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"loot_specs","config":config.snapshot()}),
    )?;
    let actual = loot_pickup::candidate_specs(&config)?;
    assert_eq!(serde_json::to_value(&actual)?, expected);
    assert_eq!(
        actual.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
        ["first", "GROUND rune", "another"]
    );
    let mut scanner = LootScanner::new(&config, root())?;
    let first = Arc::downgrade(&scanner.candidates()?[0].template);
    assert!(first.upgrade().is_some());
    scanner.update_config(&BotConfig::from_value(json!({}))?)?;
    assert!(first.upgrade().is_none());
    assert!(scanner.candidates()?.is_empty());
    Ok(())
}
#[test]
fn loot_scan_preserves_candidate_priority_and_duplicate_boundaries() -> Result<(), Box<dyn Error>> {
    let paths = [
        "assets/items/keys/key-of-hate-on-the-ground.png",
        "assets/items/runes/rune-on-the-ground.png",
    ];
    let images = paths
        .iter()
        .map(|p| codec::load_color(root().join(p)))
        .collect::<Result<Vec<_>, _>>()?;
    let mut frame = BgrFrame::zeros(Size::new(
        images[0].size().width() + images[1].size().width() + 40,
        images[0].size().height().max(images[1].size().height()) + 20,
    )?)?;
    put(&mut frame, &images[0], 10, 10);
    put(&mut frame, &images[1], images[0].size().width() + 20, 10);
    let candidates = images
        .into_iter()
        .enumerate()
        .map(|(index, image)| LootCandidate {
            label: format!("item{index}"),
            template: Arc::new(image),
            threshold: 0.85,
        })
        .collect::<Vec<_>>();
    let request_candidates = paths
        .iter()
        .enumerate()
        .map(|(index, path)| json!({"label":format!("item{index}"),"path":path,"threshold":0.85}))
        .collect::<Vec<_>>();
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"loot_scan","frame":image_json(&frame),"candidates":request_candidates}),
    )?;
    let hit = loot_pickup::scan(frame.view(), &candidates)?.ok_or("loot hit")?;
    assert_eq!(hit.label, expected["label"].as_str().ok_or("label")?);
    assert_eq!(
        json!(hit.match_result.top_left),
        expected["match"]["top_left"]
    );
    assert!(
        (hit.match_result.score - expected["match"]["score"].as_f64().ok_or("score")?).abs() < 1e-5
    );
    assert!(loot_pickup::duplicate_hit(&hit, Some(&hit), 1.4));
    assert!(!loot_pickup::duplicate_hit(
        &hit,
        Some(&hit),
        1.4f64.next_up()
    ));
    let mut nearby = hit.clone();
    nearby.match_result.top_left[0] += 18;
    assert!(loot_pickup::duplicate_hit(&nearby, Some(&hit), 0.0));
    nearby.match_result.top_left[0] += 1;
    assert!(!loot_pickup::duplicate_hit(&nearby, Some(&hit), 0.0));
    let mut candidates = candidates;
    candidates.push(LootCandidate {
        label: "later invalid".into(),
        template: Arc::new(BgrFrame::zeros(Size::new(frame.size().width() + 10, 1)?)?),
        threshold: 0.85,
    });
    assert_eq!(
        loot_pickup::scan(frame.view(), &candidates)?
            .ok_or("priority hit")?
            .label,
        hit.label
    );
    Ok(())
}
#[test]
fn configured_loot_requires_named_runes_and_skips_suffix_and_gem() -> Result<(), Box<dyn Error>> {
    let config = BotConfig::load(root().join("config"))?;
    let mut scanner = LootScanner::new(&config, root())?;
    assert_eq!(
        scanner
            .specs()
            .iter()
            .map(|spec| spec.label.as_str())
            .collect::<Vec<_>>(),
        ["key of hate", "ist rune", "ohm rune"]
    );
    for (path, expected) in [
        ("assets/items/runes/ist-on-the-ground.png", Some("ist rune")),
        ("assets/items/runes/ohm-on-the-ground.png", Some("ohm rune")),
        ("assets/items/runes/rune-on-the-ground.png", None),
        ("assets/items/gems/amethyst-on-the-ground.png", None),
    ] {
        let image = codec::load_color(root().join(path))?;
        // 모든 후보보다 큰 같은 바탕에서 이름 없는 접미사·보석도 실제 matching한다.
        let mut frame = BgrFrame::zeros(Size::new(320, 80)?)?;
        put(&mut frame, &image, 40, 20);
        let hit = scanner.scan_frame(frame.view(), None)?;
        assert_eq!(
            hit.as_ref().map(|hit| hit.label.as_str()),
            expected,
            "{path}"
        );
        if let Some(hit) = hit {
            assert_eq!(hit.match_result.top_left, [40, 20]);
            assert!(hit.match_result.score >= 0.95);
        }
    }
    Ok(())
}

#[test]
fn gem_column_first_plan_and_weighted_consensus_match_python() -> Result<(), Box<dyn Error>> {
    let counts =
        std::array::from_fn(|row| std::array::from_fn(|col| ((row * 7 + col) * 7 % 100) as u32));
    let blocked = BTreeSet::from([(1, 0), (5, 3)]);
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"gem_policy","counts":counts,"blocked":blocked}),
    )?;
    assert_eq!(
        serde_json::to_value(gem_summing::build_plan(&counts, &blocked)?)?,
        expected
    );
    let mut samples: [FrameCounts; 3] = std::array::from_fn(|_| FrameCounts {
        counts: [[0; 7]; 5],
        confidences: [[0.1; 7]; 5],
        icon_scores: [[1.0; 7]; 5],
    });
    let mut fallback = [[0; 7]; 5];
    fallback[0][1] = 6;
    fallback[0][2] = 7;
    for (index, sample) in samples.iter_mut().enumerate() {
        sample.counts[0][0] = 10 + index as u32;
        sample.counts[0][1] = if index < 2 { 50 } else { 60 };
        sample.counts[0][2] = 8;
        sample.icon_scores[0][2] = 0.419999;
        sample.counts[0][3] = 6;
        sample.icon_scores[0][3] = 0.42;
    }
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"gem_consensus","samples":samples,"fallback":fallback}),
    )?;
    let (counts, events) = gem_summing::consensus_counts(&samples, Some(&fallback))?;
    assert_eq!(json!({"counts":counts,"events":events}), expected);
    assert_eq!(counts[0][0], 0); // Reread10 differs by more than6 from tracked0.
    assert_eq!(counts[0][3], 6); // Exactly6 is accepted; icon equality0.42 contributes.
    Ok(())
}
#[test]
fn gem_count_preprocessing_and_digit_scores_match_python() -> Result<(), Box<dyn Error>> {
    let vision = GemVision::load(root())?;
    let mut state = 53u32;
    let crops = (0..16)
        .map(|index| {
            let pixels = (0..28 * 18 * 3)
                .map(|_| {
                    state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    if index == 0 {
                        0
                    } else if index == 1 {
                        255
                    } else {
                        (state >> 24) as u8
                    }
                })
                .collect();
            BgrFrame::from_vec(Size::new(28, 18)?, pixels)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"gem_counts","crops":crops.iter().map(image_json).collect::<Vec<_>>()}),
    )?;
    for (index, (crop, expected)) in crops
        .iter()
        .zip(expected.as_array().ok_or("count results")?)
        .enumerate()
    {
        let actual = vision.read_count(crop.view())?;
        assert_eq!(json!(actual.value), expected["value"], "crop{index}");
        assert!(
            (actual.confidence - expected["confidence"].as_f64().ok_or("confidence")?).abs() < 1e-6,
            "crop{index}: {} != {}",
            actual.confidence,
            expected["confidence"]
        );
    }
    Ok(())
}
#[test]
fn gem_panel_origin_and_truncation_match_python() -> Result<(), Box<dyn Error>> {
    let vision = GemVision::load(root())?;
    let header = codec::load_color(root().join("assets/ui/stash/templates/header-ko-576x74.png"))?;
    let mut frame = BgrFrame::zeros(Size::new(616, 817)?)?;
    put(&mut frame, &header, 20, 30);
    let expected = support::legacy_expectation(
        "action-oracle",
        &json!({"kind":"gem_panel","frame":image_json(&frame)}),
    )?;
    let actual = vision
        .locate_stash_panel(frame.view())?
        .ok_or("gem stash panel")?;
    assert_eq!(json!(actual.top_left), expected["top_left"]);
    assert_eq!([actual.width, actual.height], [576, 757]);
    assert!((actual.score - expected["score"].as_f64().ok_or("header score")?).abs() < 1e-5);
    assert!(vision.locate_stash_panel(header.view())?.is_none());
    Ok(())
}
