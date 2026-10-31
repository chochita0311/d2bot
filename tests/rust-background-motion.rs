use d2bot::common::scene_motion::{
    FeatureSet, MotionLimits, estimate_partial_affine, hamming_matches, measure,
};
use serde_json::{Value, json};
use std::error::Error;
mod support;

fn descriptors(count: usize) -> Vec<[u8; 32]> {
    (0..count)
        .map(|index| {
            let mut state = index as u32 + 19;
            std::array::from_fn(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
        })
        .collect()
}

#[test]
fn noisy_affine_refinement_matches_reference_coefficients_and_original_mask()
-> Result<(), Box<dyn Error>> {
    for seed in 0..8u32 {
        let mut state = seed + 53;
        let mut random = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (state >> 8) as f32 / 16_777_216.0
        };
        let source = (0..64)
            .map(|_| [random() * 960.0, random() * 570.0])
            .collect::<Vec<_>>();
        let mut target = source
            .iter()
            .map(|[x, y]| {
                [
                    1.01 * x - 0.015 * y - 57.0 + (random() - 0.5) * 1.5,
                    0.015 * x + 1.01 * y + 32.0 + (random() - 0.5) * 1.5,
                ]
            })
            .collect::<Vec<_>>();
        for point in target.iter_mut().take(seed as usize * 3) {
            point[0] += 50.0 + random() * 50.0;
            point[1] -= 80.0 + random() * 50.0;
        }
        let expected = support::legacy_expectation(
            "motion-oracle",
            &json!({"kind":"affine","source":source,"target":target,"threshold":1.5}),
        )?;
        let actual = serde_json::to_value(estimate_partial_affine(&source, &target, 1.5)?)?;
        compare(&actual, &expected, 1e-6, &format!("noisy seed{seed}"))?;
    }
    Ok(())
}
fn points(count: usize) -> Vec<[f32; 2]> {
    (0..count)
        .map(|index| {
            [
                20.0 + (index % 5) as f32 * 37.0,
                30.0 + (index / 5) as f32 * 41.0,
            ]
        })
        .collect()
}
fn feature_json(features: &FeatureSet) -> Value {
    json!({"points":features.points(),"descriptors":features.descriptors().iter().map(|d|d.to_vec()).collect::<Vec<_>>()})
}
fn compare(
    actual: &Value,
    expected: &Value,
    tolerance: f64,
    path: &str,
) -> Result<(), Box<dyn Error>> {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => {
            let a = a.as_f64().ok_or("actual number")?;
            let b = b.as_f64().ok_or("expected number")?;
            assert!((a - b).abs() <= tolerance, "{path}: {a} != {b}");
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (index, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, tolerance, &format!("{path}/{index}"))?;
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (key, a) in a {
                compare(
                    a,
                    b.get(key).ok_or("expected key missing")?,
                    tolerance,
                    &format!("{path}/{key}"),
                )?;
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
    Ok(())
}
#[test]
fn partial_affine_translation_rotation_outliers_and_degenerate_match_python()
-> Result<(), Box<dyn Error>> {
    for (a, b, translation, outliers) in [
        (1.0, 0.0, [-24.0, 12.0], 0),
        (1.01, 0.02, [200.0, -75.0], 2),
        (0.99, -0.015, [-80.0, 64.0], 8),
        (1.0, 0.0, [0.0, 0.0], 0),
    ] {
        let source = points(25);
        let mut target: Vec<_> = source
            .iter()
            .map(|p| {
                [
                    (a * f64::from(p[0]) - b * f64::from(p[1]) + translation[0]) as f32,
                    (b * f64::from(p[0]) + a * f64::from(p[1]) + translation[1]) as f32,
                ]
            })
            .collect();
        for (index, point) in target.iter_mut().enumerate().take(outliers) {
            point[0] += 100.0 + index as f32 * 19.0;
            point[1] -= 70.0 + index as f32 * 13.0;
        }
        let expected = support::legacy_expectation(
            "motion-oracle",
            &json!({"kind":"affine","source":source,"target":target,"threshold":1.5}),
        )?;
        let actual = serde_json::to_value(estimate_partial_affine(&source, &target, 1.5)?)?;
        compare(&actual, &expected, 1e-6, "affine")?;
    }
    let source = vec![[10.0, 10.0]; 8];
    let target = points(8);
    let expected = support::legacy_expectation(
        "motion-oracle",
        &json!({"kind":"affine","source":source,"target":target,"threshold":1.5}),
    )?;
    assert_eq!(
        expected,
        serde_json::to_value(estimate_partial_affine(&source, &target, 1.5)?)?
    );
    Ok(())
}
#[test]
fn measured_quality_counts_reasons_and_displacement_match_python() -> Result<(), Box<dyn Error>> {
    for (count, a, b, outliers) in [
        (3, 1.0, 0.0, 0),
        (12, 1.0, 0.0, 0),
        (12, 1.0, 0.0, 3),
        (12, 1.0, 0.0, 5),
        (12, 1.05, 0.0, 0),
        (12, 1.0, 0.04, 0),
    ] {
        let source = FeatureSet::new(points(count), descriptors(count))?;
        let mut target_points: Vec<_> = source
            .points()
            .iter()
            .map(|p| {
                [
                    (a * f64::from(p[0]) - b * f64::from(p[1]) - 24.0) as f32,
                    (b * f64::from(p[0]) + a * f64::from(p[1]) + 12.0) as f32,
                ]
            })
            .collect();
        for (index, point) in target_points.iter_mut().enumerate().take(outliers) {
            point[0] += 90.0 + index as f32 * 13.0;
            point[1] -= 120.0 + index as f32 * 11.0;
        }
        let target = FeatureSet::new(target_points, descriptors(count))?;
        let expected = support::legacy_expectation(
            "motion-oracle",
            &json!({"kind":"measure","limits":{"minimum_matches":4,"minimum_inlier_ratio":0.65,"maximum_error":3.0,"scale":0.5},"source":feature_json(&source),"target":feature_json(&target)}),
        )?;
        let actual = serde_json::to_value(measure(
            &source,
            &target,
            MotionLimits::new(4, 0.65, 3.0, 0.5)?,
        )?)?;
        for field in ["reason", "inliers", "matches"] {
            assert_eq!(
                actual[field], expected[field],
                "case count={count} a={a} b={b} outliers={outliers} field={field}"
            );
        }
        compare(&actual, &expected, 1e-6, "measure")?;
    }
    Ok(())
}
#[test]
fn strict_hamming_ratio_ties_and_invalid_limits_are_explicit() -> Result<(), Box<dyn Error>> {
    let source = [[0u8; 32]; 2];
    for (first_bits, expected_count) in [(7, 0), (6, 2), (0, 2)] {
        let mut first = [0u8; 32];
        for bit in 0..first_bits {
            first[bit / 8] |= 1 << (bit % 8);
        }
        let mut second = [0u8; 32];
        for bit in 0..10 {
            second[bit / 8] |= 1 << (bit % 8);
        }
        let actual = hamming_matches(&source, &[first, second])?;
        assert_eq!(expected_count, actual.len());
        assert!(actual.iter().all(|pair| pair.train == 0));
    }
    assert!(hamming_matches(&source, &source)?.is_empty());
    for (minimum, ratio, error, scale) in [
        (3, 0.65, 3.0, 0.5),
        (4, 0.0, 3.0, 0.5),
        (4, 1.1, 3.0, 0.5),
        (4, 0.65, f64::NAN, 0.5),
        (4, 0.65, 3.0, f64::INFINITY),
    ] {
        assert!(MotionLimits::new(minimum, ratio, error, scale).is_err());
    }
    assert!(FeatureSet::new(vec![[f32::NAN, 0.0]], vec![[0; 32]]).is_err());
    Ok(())
}

#[test]
fn orb_pyramid_selection_orientation_and_descriptor_bits_match_python() -> Result<(), Box<dyn Error>>
{
    use d2bot::common::{
        frame::{GrayFrame, Size},
        scene_motion::orb,
    };
    for (width, height, seed, masked) in [
        (192, 160, 7, false),
        (320, 240, 53, true),
        (384, 288, 13, false),
        (96, 88, 23, true),
        (128, 96, 39, true),
    ] {
        let size = Size::new(width, height)?;
        let mut state: u32 = seed;
        let pixels = (0..width * height)
            .map(|_| {
                state = state.wrapping_mul(1_664_525u32).wrapping_add(1_013_904_223);
                (state >> 24) as u8
            })
            .collect::<Vec<_>>();
        let image = GrayFrame::from_vec(size, pixels)?;
        let mut mask = GrayFrame::from_vec(size, vec![255; width * height])?;
        for y in height / 3..height * 2 / 3 {
            for x in width / 3..width * 2 / 3 {
                mask.pixels_mut()[y * width + x] = 0;
            }
        }
        for (index, pixel) in mask.pixels_mut().iter_mut().enumerate() {
            if *pixel != 0 {
                *pixel = if index % 3 == 0 { 1 } else { 254 };
            }
        }
        let hex = |pixels: &[u8]| {
            pixels
                .iter()
                .map(|n| format!("{n:02x}"))
                .collect::<String>()
        };
        let expected = support::legacy_expectation(
            "motion-oracle",
            &json!({"kind":"extract","width":width,"height":height,"gray":hex(image.pixels()),"mask":masked.then(||hex(mask.pixels()))}),
        )?;
        let actual = orb::extract(image.view(), masked.then(|| mask.view()))?;
        let points = serde_json::to_value(&actual.keypoints)?;
        assert_eq!(
            points.as_array().map(Vec::len),
            expected["keypoints"].as_array().map(Vec::len),
            "count seed={seed}"
        );
        for (index, (actual, expected)) in points
            .as_array()
            .ok_or("point array")?
            .iter()
            .zip(expected["keypoints"].as_array().ok_or("oracle points")?)
            .enumerate()
        {
            // serde serializes f32 with its shortest decimal; compare the
            // underlying native float32 values, not their f64 JSON spellings.
            for field in ["point", "size"] {
                let values = |value: &Value| -> Result<Vec<f32>, Box<dyn Error>> {
                    let values = if let Some(array) = value.as_array() {
                        array.clone()
                    } else {
                        vec![value.clone()]
                    };
                    values
                        .iter()
                        .map(|n| {
                            n.as_f64()
                                .map(|n| n as f32)
                                .ok_or_else(|| "float32 value".into())
                        })
                        .collect()
                };
                assert_eq!(
                    values(&actual[field])?,
                    values(&expected[field])?,
                    "seed{seed}/{index}/{field}"
                );
            }
            assert_eq!(actual["octave"], expected["octave"], "octave");
            compare(
                &actual["angle"],
                &expected["angle"],
                1e-3,
                &format!("seed{seed}/{index}/angle"),
            )?;
            compare(
                &actual["response"],
                &expected["response"],
                1e-6,
                &format!("seed{seed}/{index}/response"),
            )?;
        }
        let expected = expected["descriptors"]
            .as_array()
            .ok_or("oracle descriptor array")?;
        for (index, (actual, expected)) in actual.descriptors.iter().zip(expected).enumerate() {
            assert_eq!(
                serde_json::to_value(actual.to_vec())?,
                *expected,
                "seed{seed}/{index}/descriptor"
            );
        }
    }
    Ok(())
}
