use d2bot::common::{
    frame::{BgrFrame, GrayFrame, Size},
    scene_motion::{BackgroundMotionObserver, CameraMotion, MotionLimits, SceneFeatures},
};
use serde_json::{Value, json};
use std::error::Error;
mod support;

#[test]
fn existing_consecutive_navigation_png_frames_match_without_capture_or_input()
-> Result<(), Box<dyn Error>> {
    use d2bot::common::{
        frame::{Image, PixelRect},
        scene_motion::{LandmarkMask, MotionView},
        vision_ops::codec,
    };
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let paths = [
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-before.png",
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-after.png",
    ];
    let frames = paths
        .iter()
        .map(|path| codec::load_color(root.join(path)))
        .collect::<Result<Vec<_>, _>>()?;
    let size = Size::new(1922, 1140)?;
    assert!(frames.iter().all(|frame| frame.size() == size));
    let floor = Image::<bool, 1>::from_vec(Size::new(240, 142)?, vec![false; 240 * 142])?;
    let view_json = json!({"viewport":[0,0,1922,1140],"anchor":[960,580],"cell_pixels":8,"floor":floor.pixels().chunks(240).collect::<Vec<_>>()});
    let items=paths.iter().enumerate().map(|(index,path)|json!({"path":path,"width":1922,"height":1140,"sequence":index+1,"captured_at":index as f64*0.1,"view":view_json})).collect::<Vec<_>>();
    let expected = support::legacy_expectation(
        "motion-oracle",
        &json!({"kind":"observe","limits":{"minimum_matches":12,"minimum_inlier_ratio":0.65,"maximum_error":3,"scale":0.5,"landmark_viewport":[120,180,1720,890],"actor_exclusion":[260,230,260,120]},"frames":items}),
    )?;
    let mut observer = BackgroundMotionObserver::new(
        MotionLimits::new(12, 0.65, 3.0, 0.5)?,
        Some(LandmarkMask {
            viewport: PixelRect::new(120, 180, 1720, 890)?,
            actor_exclusion: [260, 230, 260, 120],
        }),
    );
    for (index, (frame, expected)) in frames
        .iter()
        .zip(expected.as_array().ok_or("real results")?)
        .enumerate()
    {
        let started = std::time::Instant::now();
        let actual = observer.observe(
            frame.view(),
            MotionView {
                sequence: index as u64 + 1,
                captured_at: index as f64 * 0.1,
                frame_size: size,
                viewport: PixelRect::full(size),
                anchor: [960.0, 580.0],
                cell_pixels: 8,
                floor: &floor,
                excluded_pixels: None,
            },
            None,
        )?;
        eprintln!(
            "native PNG frame {index}: {} ms (input-free)",
            started.elapsed().as_millis()
        );
        compare_motion(actual, &expected["motion"], &format!("real/frame{index}"))?;
        compare_features(
            observer.features().ok_or("real features")?,
            &expected["features"],
            &format!("real/frame{index}"),
        )?;
        assert_eq!(
            serde_json::to_value(observer.previous_sequence())?,
            expected["previous_sequence"]
        );
    }
    Ok(())
}

#[test]
fn floor_exclusions_landmarks_and_overlap_stationary_fallback_match_python()
-> Result<(), Box<dyn Error>> {
    use d2bot::common::{
        frame::{Image, PixelRect},
        scene_motion::{LandmarkMask, MotionView},
    };
    for landmark in [false, true] {
        let original = noise(384, 288, 53)?;
        let shifted = shift(&original, -24, 12)?;
        let mut actor_changed = shifted.clone();
        for y in 114..174 {
            for x in 162..222 {
                let index = (y * 384 + x) * 3;
                actor_changed.pixels_mut()[index..index + 3].fill(255);
            }
        }
        let viewport = PixelRect::new(8, 8, 376, 280)?;
        let floor = Image::<bool, 1>::from_vec(
            Size::new(46, 34)?,
            (0..46 * 34).map(|index| index % 17 != 0).collect(),
        )?;
        let excluded = Image::<bool, 1>::from_vec(
            original.size(),
            (0..384 * 288)
                .map(|index| index % 384 < 60 && index / 384 < 90)
                .collect(),
        )?;
        let view_json = json!({"viewport":[8,8,376,280],"anchor":[192.75,144.25],"cell_pixels":8,"floor":floor.pixels().chunks(46).collect::<Vec<_>>(),"excluded_pixels":excluded.pixels().chunks(384).collect::<Vec<_>>()});
        let input = [
            (&original, None),
            (&shifted, landmark.then_some([24.0, -12.0])),
            (&actor_changed, landmark.then_some([380.0, -280.0])),
            (&original, landmark.then_some([380.0, -280.0])),
        ];
        let frames=input.iter().enumerate().map(|(index,(frame,hint))|json!({"width":384,"height":288,"bgr":hex(frame.pixels()),"sequence":index+1,"captured_at":index as f64*0.1,"hint":hint,"view":view_json})).collect::<Vec<_>>();
        let mut limits =
            json!({"minimum_matches":12,"minimum_inlier_ratio":0.65,"maximum_error":3,"scale":1});
        if landmark {
            limits["landmark_viewport"] = json!([8, 8, 376, 280]);
            limits["actor_exclusion"] = json!([30, 30, 30, 30]);
        }
        let expected = support::legacy_expectation(
            "motion-oracle",
            &json!({"kind":"observe","limits":limits,"frames":frames}),
        )?;
        let mut observer = BackgroundMotionObserver::new(
            MotionLimits::new(12, 0.65, 3.0, 1.0)?,
            landmark.then_some(LandmarkMask {
                viewport,
                actor_exclusion: [30; 4],
            }),
        );
        for (index, ((frame, hint), expected)) in input
            .iter()
            .zip(expected.as_array().ok_or("view results")?)
            .enumerate()
        {
            let view = MotionView {
                sequence: index as u64 + 1,
                captured_at: index as f64 * 0.1,
                frame_size: frame.size(),
                viewport,
                anchor: [192.75, 144.25],
                cell_pixels: 8,
                floor: &floor,
                excluded_pixels: Some(&excluded),
            };
            let actual = observer.observe(frame.view(), view, *hint)?;
            let label = format!("landmark{landmark}/frame{index}");
            compare_motion(actual, &expected["motion"], &label)?;
            assert_eq!(
                serde_json::to_value(observer.previous_sequence())?,
                expected["previous_sequence"],
                "{label}/previous"
            );
            compare_features(
                observer.features().ok_or("view features")?,
                &expected["features"],
                &label,
            )?;
        }
    }
    Ok(())
}

#[test]
fn early_unavailable_precedes_hint_validation_and_errors_publish_current_features()
-> Result<(), Box<dyn Error>> {
    use d2bot::common::scene_motion::MotionReason;
    let frame = noise(160, 160, 13)?;
    let mask = GrayFrame::from_vec(frame.size(), vec![255; 160 * 160])?;
    let mut observer = BackgroundMotionObserver::new(MotionLimits::new(4, 0.65, 3.0, 1.0)?, None);
    assert_eq!(
        observer
            .observe_mask(frame.view(), mask.view(), 1, 0.0, Some([f64::NAN, 0.0]))?
            .reason,
        MotionReason::CorrespondenceUnavailable
    );
    assert!(
        observer
            .observe_mask(frame.view(), mask.view(), 2, 0.1, Some([0.0, 0.0]))
            .is_err()
    );
    assert_eq!(observer.features().ok_or("published")?.sequence, 2);
    assert_eq!(observer.previous_sequence(), Some(1));
    assert_eq!(
        observer
            .observe_mask(frame.view(), mask.view(), 2, 0.1, None)?
            .reason,
        MotionReason::FrameNotNewer
    );
    assert!(
        observer
            .observe_mask(frame.view(), mask.view(), 3, 0.2, None)?
            .displacement
            .is_some()
    );
    Ok(())
}

fn hex(pixels: &[u8]) -> String {
    pixels.iter().map(|n| format!("{n:02x}")).collect()
}
fn noise(width: usize, height: usize, mut state: u32) -> Result<BgrFrame, Box<dyn Error>> {
    let pixels = (0..width * height * 3)
        .map(|_| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            25 + ((state >> 24) % 175) as u8
        })
        .collect();
    Ok(BgrFrame::from_vec(Size::new(width, height)?, pixels)?)
}
fn shift(source: &BgrFrame, dx: isize, dy: isize) -> Result<BgrFrame, Box<dyn Error>> {
    let size = source.size();
    let mut output = BgrFrame::zeros(size)?;
    for y in 0..size.height() {
        for x in 0..size.width() {
            let sx = x as isize - dx;
            let sy = y as isize - dy;
            if sx >= 0 && sy >= 0 && sx < size.width() as isize && sy < size.height() as isize {
                let from = (sy as usize * size.width() + sx as usize) * 3;
                let to = (y * size.width() + x) * 3;
                output.pixels_mut()[to..to + 3].copy_from_slice(&source.pixels()[from..from + 3]);
            }
        }
    }
    Ok(output)
}
fn compare_motion(
    actual: CameraMotion,
    expected: &Value,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    let actual = serde_json::to_value(actual)?;
    for field in ["reason", "inliers", "matches"] {
        assert_eq!(actual[field], expected[field], "{label}/{field}");
    }
    for (field, tolerance) in [("inlier_ratio", 1e-12), ("linear_error", 1e-6)] {
        match (actual[field].as_f64(), expected[field].as_f64()) {
            (Some(a), Some(b)) => {
                assert!((a - b).abs() <= tolerance, "{label}/{field}: {a} != {b}")
            }
            _ => assert_eq!(actual[field], expected[field], "{label}/{field}"),
        }
    }
    match (
        actual["displacement"].as_array(),
        expected["displacement"].as_array(),
    ) {
        (Some(a), Some(b)) => {
            for axis in 0..2 {
                let left = a[axis].as_f64().ok_or("displacement")?;
                let right = b[axis].as_f64().ok_or("oracle displacement")?;
                assert!(
                    (left - right).abs() <= 0.1,
                    "{label}/displacement/{axis}: {left} != {right}"
                );
            }
        }
        _ => assert_eq!(
            actual["displacement"], expected["displacement"],
            "{label}/displacement"
        ),
    }
    Ok(())
}
fn compare_features(
    actual: &SceneFeatures,
    expected: &Value,
    label: &str,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(
        actual.sequence,
        expected["sequence"].as_u64().ok_or("sequence")?
    );
    assert_eq!(
        actual.captured_at,
        expected["captured_at"].as_f64().ok_or("captured_at")?
    );
    assert_eq!(
        json!([actual.frame_size.width(), actual.frame_size.height()]),
        expected["frame_size"]
    );
    let expected_points = expected["points"].as_array().ok_or("native points array")?;
    assert_eq!(
        actual.points().len(),
        expected_points.len(),
        "{label}/point count"
    );
    for (index, (actual, expected)) in actual.points().iter().zip(expected_points).enumerate() {
        for axis in 0..2 {
            assert_eq!(
                actual[axis],
                expected[axis].as_f64().ok_or("native coordinate")? as f32,
                "{label}/native {index}/{axis}"
            );
        }
    }
    assert_eq!(
        serde_json::to_value(
            actual
                .descriptors()
                .map(|ds| ds.iter().map(|d| d.to_vec()).collect::<Vec<_>>())
        )?,
        expected["descriptors"],
        "{label}/descriptors"
    );
    Ok(())
}
#[test]
fn observer_sequence_blank_recovery_resize_and_reset_match_python() -> Result<(), Box<dyn Error>> {
    for scale in [0.5, 1.0, 0.7] {
        let original = noise(384, 288, 7)?;
        let shifted = shift(&original, -24, 12)?;
        let blank = BgrFrame::zeros(original.size())?;
        let resized = noise(320, 256, 53)?;
        let input = [
            (&original, 1, 0.0, false),
            (&original, 2, 0.1, false),
            (&shifted, 3, 0.2, false),
            (&blank, 4, 0.3, false),
            (&original, 4, 0.4, false),
            (&original, 5, 0.25, false),
            (&original, 6, 0.4, false),
            (&resized, 7, 0.5, false),
            (&original, 8, 0.6, true),
        ];
        let frames=input.iter().map(|(frame,sequence,at,reset)|json!({"width":frame.size().width(),"height":frame.size().height(),"bgr":hex(frame.pixels()),"mask":"ff".repeat(frame.size().width()*frame.size().height()),"sequence":sequence,"captured_at":at,"reset":reset})).collect::<Vec<_>>();
        let expected = support::legacy_expectation(
            "motion-oracle",
            &json!({"kind":"observe","limits":{"minimum_matches":12,"minimum_inlier_ratio":0.65,"maximum_error":3,"scale":scale},"frames":frames}),
        )?;
        let mut observer =
            BackgroundMotionObserver::new(MotionLimits::new(12, 0.65, 3.0, scale)?, None);
        for (index, ((frame, sequence, at, reset), expected)) in input
            .iter()
            .zip(expected.as_array().ok_or("observer results")?)
            .enumerate()
        {
            if *reset {
                observer.reset();
                assert!(observer.features().is_none());
            }
            let mask = GrayFrame::from_vec(
                frame.size(),
                vec![255; frame.size().width() * frame.size().height()],
            )?;
            let actual = observer.observe_mask(frame.view(), mask.view(), *sequence, *at, None)?;
            let label = format!("scale{scale}/frame{index}");
            compare_motion(actual, &expected["motion"], &label)?;
            assert_eq!(
                serde_json::to_value(observer.previous_sequence())?,
                expected["previous_sequence"],
                "{label}/previous"
            );
            compare_features(
                observer.features().ok_or("latest features")?,
                &expected["features"],
                &label,
            )?;
        }
        let features = std::sync::Arc::downgrade(observer.features().ok_or("last features")?);
        observer.reset();
        assert!(features.upgrade().is_none());
    }
    Ok(())
}
