#[path = "support/directory.rs"]
mod directory;
mod support;

use d2bot::common::frame::{BgrFrame, Image, Size};
use d2bot::common::frame_geometry::{
    FrameGeometry, PhysicalTarget, ReferenceLayout, rect_from_value,
};
use d2bot::common::vision_ops::{
    codec::{self, DecodedImage},
    color, components, filters,
    morphology::{self, Kernel, Morphology},
    resize::{self, Interpolation},
    template::{self, MatchMethod},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn pattern(width: usize, height: usize, channels: usize) -> Vec<u8> {
    let mut state = 0x5a81_63c9u32;
    (0..width * height * channels)
        .map(|_| {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            (state >> 24) as u8
        })
        .collect()
}

fn check<const C: usize>(actual: &Image<u8, C>, expected: &Value, tolerance: u8) -> TestResult {
    assert_eq!(
        (actual.size().width(), actual.size().height(), C),
        (
            expected["width"].as_u64().ok_or("width")? as usize,
            expected["height"].as_u64().ok_or("height")? as usize,
            expected["channels"].as_u64().ok_or("channels")? as usize
        )
    );
    let values = expected["pixels"].as_array().ok_or("pixels")?;
    assert_eq!(actual.bytes().len(), values.len());
    let mut worst = 0;
    for (index, (a, b)) in actual.bytes().iter().zip(values).enumerate() {
        let b = u8::try_from(b.as_u64().ok_or("pixel")?)?;
        worst = worst.max(a.abs_diff(b));
        if a.abs_diff(b) > tolerance {
            return Err(
                format!("pixel {index}: {a} != {b}, tolerance {tolerance}, worst {worst}").into(),
            );
        }
    }
    Ok(())
}

#[test]
fn resize_patterns_match_opencv_with_declared_pixel_bounds() -> TestResult {
    for (width, height) in [(1, 1), (2, 3), (7, 9), (32, 21)] {
        let pixels = pattern(width, height, 3);
        let frame = BgrFrame::from_vec(Size::new(width, height)?, pixels.clone())?;
        for size in [[1, 1], [3, 5], [11, 7], [19, 23], [width, height]] {
            for (name, mode, tolerance) in [
                ("nearest", Interpolation::Nearest, 0),
                ("linear", Interpolation::Linear, 1),
                ("area", Interpolation::Area, 1),
            ] {
                let expected = support::legacy_expectation(
                    "vision-oracle",
                    &json!({"op":"resize","width":width,"height":height,"pixels":pixels,"mode":name,"size":size}),
                )?;
                check(
                    &resize::resize(frame.view(), Size::new(size[0], size[1])?, mode)?,
                    &expected,
                    tolerance,
                )?;
            }
        }
    }
    Ok(())
}

#[test]
fn color_conversions_match_opencv() -> TestResult {
    let pixels = pattern(41, 27, 3);
    let frame = BgrFrame::from_vec(Size::new(41, 27)?, pixels.clone())?;
    for operation in ["gray", "hsv", "lab"] {
        let expected = support::legacy_expectation(
            "vision-oracle",
            &json!({"op":operation,"width":41,"height":27,"pixels":pixels}),
        )?;
        match operation {
            "gray" => check(&color::gray(frame.view())?, &expected, 0)?,
            "hsv" => check(&color::hsv(frame.view())?, &expected, 0)?,
            _ => check(&color::lab(frame.view())?, &expected, 0)?,
        }
    }
    Ok(())
}

#[test]
fn lab_all_uint8_colors_match_pinned_opencv_exactly() -> TestResult {
    color_audit("lab")
}

#[test]
fn hsv_all_uint8_colors_match_pinned_opencv_exactly() -> TestResult {
    color_audit("hsv")
}

fn color_audit(operation: &str) -> TestResult {
    let expected = support::legacy_expectation(
        "observer-oracle",
        &json!({"kind":format!("{operation}_audit")}),
    )?;
    let mut frame = BgrFrame::zeros(Size::new(256, 256)?)?;
    for blue in 0..=255u8 {
        for green in 0..=255u8 {
            for red in 0..=255u8 {
                let at = (usize::from(green) * 256 + usize::from(red)) * 3;
                frame.pixels_mut()[at..at + 3].copy_from_slice(&[blue, green, red]);
            }
        }
        let converted = if operation == "lab" {
            color::lab(frame.view())?
        } else {
            color::hsv(frame.view())?
        };
        let actual = format!("{:x}", Sha256::digest(converted.bytes()));
        assert_eq!(
            actual,
            expected[blue as usize].as_str().ok_or("color hash")?,
            "{operation}, B={blue}"
        );
    }
    Ok(())
}

#[test]
fn current_asset_channel_modes_and_lossless_png_round_trip() -> TestResult {
    for path in [
        "assets/ui/survival/glyphs/1.png",
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/templates/encounters/summoner-robe-corpse-lower-band-right.png",
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/encounters/lightning-spire-lit.png",
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/encounters/specter.webp",
    ] {
        let expected =
            support::legacy_expectation("vision-oracle", &json!({"op":"decode","path":path}))?;
        match codec::load_unchanged(path)? {
            DecodedImage::Gray(image) => check(&image, &expected, 0)?,
            DecodedImage::Bgr(image) => check(&image, &expected, 0)?,
            DecodedImage::Bgra(image) => check(&image, &expected, 0)?,
        }
    }
    let frame = BgrFrame::from_vec(Size::new(19, 27)?, pattern(19, 27, 3))?;
    assert_eq!(codec::decode_color(&codec::encode_png(&frame)?)?, frame);
    assert!(codec::decode_color(b"not an image").is_err());
    Ok(())
}

#[test]
fn normalization_uses_separate_client_and_borrows_native_frame() -> TestResult {
    for (source_size, client) in [
        ([126, 84], [3, 12, 123, 79]),
        ([402, 260], [10, 40, 370, 240]),
    ] {
        let target = PhysicalTarget::new(-300, 140, source_size[0], source_size[1])?;
        let raw = json!({"schema":1,"frame_size":[200,140],"client_box":[10,30,190,130],"aspect_tolerance":0.01});
        let geometry = FrameGeometry::create(
            target,
            Some(ReferenceLayout::from_value(&raw)?),
            Some(rect_from_value(&json!(client))?),
        )?;
        let frame = BgrFrame::from_vec(target.size(), pattern(source_size[0], source_size[1], 3))?;
        let expected = support::legacy_expectation(
            "vision-oracle",
            &json!({"op":"normalize","width":source_size[0],"height":source_size[1],"pixels":frame.bytes(),
            "target":target.as_value(),"layout":raw,"client":client}),
        )?;
        check(resize::normalize(&geometry, &frame)?.as_ref(), &expected, 1)?;
    }
    let target = PhysicalTarget::new(0, 0, 2, 3)?;
    let frame = BgrFrame::zeros(target.size())?;
    let geometry = FrameGeometry::create(target, None, None)?;
    assert!(matches!(
        resize::normalize(&geometry, &frame)?,
        std::borrow::Cow::Borrowed(_)
    ));
    assert!(resize::normalize(&geometry, &BgrFrame::zeros(Size::new(3, 2)?)?).is_err());
    Ok(())
}

fn check_float(actual: &Image<f32, 1>, expected: &Value, tolerance: f32) -> TestResult {
    let values = expected["pixels"].as_array().ok_or("pixels")?;
    assert_eq!(actual.pixels().len(), values.len());
    for (index, (a, b)) in actual.pixels().iter().zip(values).enumerate() {
        let b = b.as_f64().ok_or("float pixel")? as f32;
        if (*a - b).abs() > tolerance {
            return Err(format!("float pixel {index}: {a} != {b}, tolerance {tolerance}").into());
        }
    }
    Ok(())
}

#[test]
fn filters_and_thresholds_match_opencv() -> TestResult {
    let frame = Image::<u8, 1>::from_vec(Size::new(19, 17)?, pattern(19, 17, 1))?;
    let options = [(3, 0.0), (3, 0.8), (15, 0.0), (0, 5.0)];
    let requests:Vec<_>=options.iter().map(|(kernel,sigma)| json!({"op":"gaussian","width":19,"height":17,"channels":1,"pixels":frame.bytes(),"kernel":kernel,"sigma":sigma})).collect();
    let expected = support::legacy_expectation("vision-oracle", &json!(requests))?;
    for (index, (kernel, sigma)) in options.into_iter().enumerate() {
        check(
            &filters::gaussian(frame.view(), kernel, sigma)?,
            &expected[index],
            1,
        )?;
    }
    let floats = Image::<f32, 1>::from_vec(
        frame.size(),
        frame
            .bytes()
            .iter()
            .map(|value| if *value > 150 { 1.0 } else { 0.0 })
            .collect(),
    )?;
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"box","dtype":"float32","width":19,"height":17,"channels":1,"pixels":floats.pixels(),"kernel":[9,9]}),
    )?;
    check_float(&filters::box_filter(floats.view(), 9, 9)?, &expected, 1e-4)?;
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"threshold","width":19,"height":17,"channels":1,"pixels":frame.bytes(),"otsu":true}),
    )?;
    let (threshold, result) = filters::otsu(frame.view())?;
    assert_eq!(Some(f64::from(threshold)), expected["threshold"].as_f64());
    assert_eq!(json!(result.bytes()), expected["pixels"]);
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"threshold","width":19,"height":17,"channels":1,"pixels":frame.bytes(),"threshold":130}),
    )?;
    assert_eq!(
        json!(filters::threshold(frame.view(), 130.0)?.bytes()),
        expected["pixels"]
    );
    assert!(filters::gaussian(frame.view(), 2, 0.0).is_err());
    assert!(filters::box_filter(floats.view(), 0, 9).is_err());
    Ok(())
}

#[test]
fn morphology_borders_even_anchors_and_cross_match_opencv() -> TestResult {
    let mut requests = Vec::new();
    let mut actual = Vec::new();
    for (width, height) in [(1, 1), (2, 3), (19, 17)] {
        let frame = Image::<u8, 1>::from_vec(
            Size::new(width, height)?,
            pattern(width, height, 1)
                .into_iter()
                .map(|value| if value > 170 { 255 } else { 0 })
                .collect(),
        )?;
        for (kw, kh, cross) in [
            (2, 2, false),
            (19, 1, false),
            (1, 3, false),
            (5, 5, false),
            (3, 3, true),
        ] {
            let kernel = if cross {
                Kernel::Cross3
            } else {
                Kernel::Rectangle {
                    width: kw,
                    height: kh,
                }
            };
            for (mode, operation) in [
                ("erode", Morphology::Erode),
                ("dilate", Morphology::Dilate),
                ("open", Morphology::Open),
                ("close", Morphology::Close),
            ] {
                requests.push(json!({"op":"morphology","width":width,"height":height,"channels":1,"pixels":frame.bytes(),"kernel":[kw,kh],"cross":cross,"mode":mode}));
                actual.push(morphology::morphology(frame.view(), kernel, operation)?);
            }
        }
    }
    let expected = support::legacy_expectation("vision-oracle", &json!(requests))?;
    for (index, result) in actual.iter().enumerate() {
        check(result, &expected[index], 0)?;
    }
    Ok(())
}

#[test]
fn components_membership_order_centroids_and_mask5_distance_match_opencv() -> TestResult {
    let mut masks = vec![Image::<u8, 1>::from_vec(
        Size::new(37, 25)?,
        pattern(37, 25, 1)
            .into_iter()
            .map(|value| if value > 230 { 255 } else { 0 })
            .collect(),
    )?];
    masks.push(Image::<u8, 1>::from_vec(
        Size::new(7, 3)?,
        vec![
            0, 0, 0, 0, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0,
        ],
    )?);
    masks.push(Image::<u8, 1>::zeros(Size::new(3, 2)?)?);
    masks.push(Image::<u8, 1>::from_vec(Size::new(3, 2)?, vec![255; 6])?);
    for frame in masks {
        for (connectivity, number) in [
            (components::Connectivity::Four, 4),
            (components::Connectivity::Eight, 8),
        ] {
            let expected = support::legacy_expectation(
                "vision-oracle",
                &json!({"op":"components","width":frame.size().width(),"height":frame.size().height(),"channels":1,"pixels":frame.bytes(),"connectivity":number}),
            )?;
            let actual = components::connected_components(frame.view(), connectivity)?;
            assert_eq!(json!(actual.labels.pixels()), expected["labels"]);
            let stats:Vec<_>=actual.stats.iter().map(|item|json!({"label":item.label,"area":item.area,
                "rect":item.rect.map(|rect|[rect.left(),rect.top(),rect.right(),rect.bottom()]),"centroid":item.centroid})).collect();
            assert_eq!(json!(stats), expected["stats"]);
        }
        let expected = support::legacy_expectation(
            "vision-oracle",
            &json!({"op":"distance","width":frame.size().width(),"height":frame.size().height(),"channels":1,"pixels":frame.bytes()}),
        )?;
        check_float(
            &components::distance_l2_mask5(frame.view())?,
            &expected,
            1e-4,
        )?;
    }
    Ok(())
}

#[test]
fn template_channel_means_constants_and_fft_maps_match_opencv() -> TestResult {
    for (width, height, tw, th) in [(18, 14, 5, 4), (96, 72, 17, 19)] {
        let frame = BgrFrame::from_vec(Size::new(width, height)?, pattern(width, height, 3))?;
        let patch = frame
            .view()
            .crop(d2bot::common::frame::PixelRect::new(5, 6, 5 + tw, 6 + th)?)?
            .to_owned()?;
        for (name, method) in [
            ("correlation", MatchMethod::Correlation),
            ("normalized_correlation", MatchMethod::NormalizedCorrelation),
            ("normalized_coefficient", MatchMethod::NormalizedCoefficient),
        ] {
            let expected = support::legacy_expectation(
                "vision-oracle",
                &json!({"op":"match","width":width,"height":height,"channels":3,"pixels":frame.bytes(),
                "template_width":tw,"template_height":th,"template_pixels":patch.bytes(),"method":name}),
            )?;
            let actual = template::match_template(frame.view(), patch.view(), method)?;
            let values = expected["pixels"].as_array().ok_or("pixels")?;
            for (index, (a, b)) in actual.pixels().iter().zip(values).enumerate() {
                let b = b.as_f64().ok_or("score")?;
                let tolerance = if method == MatchMethod::Correlation {
                    1e-4f64.max(b.abs() * 1e-6)
                } else {
                    2e-5
                };
                assert!(
                    (f64::from(*a) - b).abs() <= tolerance,
                    "{name} {width}x{height} map[{index}] {a} != {b}, tolerance {tolerance}"
                );
            }
            let peak = template::maximum(&actual).ok_or("missing peak")?;
            assert_eq!(json!(peak.location), expected["peak"]["location"]);
        }
    }
    for (source, pattern) in [
        (vec![50; 42], vec![10, 20, 30, 40]),
        (vec![50; 42], vec![0; 4]),
        ((0..42).collect(), vec![70; 4]),
    ] {
        let source = Image::<u8, 1>::from_vec(Size::new(7, 6)?, source)?;
        let pattern = Image::<u8, 1>::from_vec(Size::new(2, 2)?, pattern)?;
        let expected = support::legacy_expectation(
            "vision-oracle",
            &json!({"op":"match","width":7,"height":6,"channels":1,"pixels":source.bytes(),
            "template_width":2,"template_height":2,"template_pixels":pattern.bytes(),"method":"normalized_coefficient"}),
        )?;
        let actual = template::match_template(
            source.view(),
            pattern.view(),
            MatchMethod::NormalizedCoefficient,
        )?;
        check_float(&actual, &expected, 2e-5)?;
        assert_eq!(
            json!(template::maximum(&actual).ok_or("peak")?.location),
            expected["peak"]["location"]
        );
    }
    Ok(())
}

#[test]
fn prepared_gray_matcher_keeps_one_owned_source_across_templates() -> TestResult {
    let source = Image::<u8, 1>::from_vec(Size::new(97, 73)?, pattern(97, 73, 1))?;
    let matcher = template::GrayMatcher::new(source.clone())?;
    for (tw, th) in [(7, 5), (17, 19), (25, 23)] {
        let patch = source
            .view()
            .crop(d2bot::common::frame::PixelRect::new(8, 9, 8 + tw, 9 + th)?)?
            .to_owned()?;
        for (name, method) in [
            ("correlation", MatchMethod::Correlation),
            ("normalized_correlation", MatchMethod::NormalizedCorrelation),
            ("normalized_coefficient", MatchMethod::NormalizedCoefficient),
        ] {
            let expected = support::legacy_expectation(
                "vision-oracle",
                &json!({"op":"match","width":97,"height":73,"channels":1,"pixels":source.bytes(),"template_width":tw,"template_height":th,"template_pixels":patch.bytes(),"method":name}),
            )?;
            let actual = matcher.match_pattern(patch.view(), method)?;
            let direct = template::match_template(source.view(), patch.view(), method)?;
            assert_eq!(actual, direct);
            for (a, b) in actual
                .pixels()
                .iter()
                .zip(expected["pixels"].as_array().ok_or("pixels")?)
            {
                let b = b.as_f64().ok_or("score")?;
                let tolerance = if method == MatchMethod::Correlation {
                    1e-4_f64.max(b.abs() * 1e-6)
                } else {
                    2e-5
                };
                assert!((f64::from(*a) - b).abs() <= tolerance);
            }
            assert_eq!(
                json!(template::maximum(&actual).ok_or("peak")?.location),
                expected["peak"]["location"]
            );
        }
    }
    Ok(())
}

#[test]
fn float_correlation_and_rejected_types_remain_explicit() -> TestResult {
    let source = Image::<f32, 1>::from_vec(
        Size::new(23, 17)?,
        pattern(23, 17, 1)
            .into_iter()
            .map(|value| f32::from(value) / 255.0)
            .collect(),
    )?;
    let patch = source
        .view()
        .crop(d2bot::common::frame::PixelRect::new(4, 6, 9, 10)?)?
        .to_owned()?;
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"match","dtype":"float32","width":23,"height":17,"channels":1,"pixels":source.pixels(),
        "template_width":5,"template_height":4,"template_pixels":patch.pixels(),"method":"correlation"}),
    )?;
    check_float(
        &template::match_template(source.view(), patch.view(), MatchMethod::Correlation)?,
        &expected,
        1e-4,
    )?;
    assert!(
        template::match_template(
            patch.view(),
            source.view(),
            MatchMethod::NormalizedCoefficient
        )
        .is_err()
    );
    let mut invalid = source.clone();
    invalid.pixels_mut()[0] = f32::NAN;
    assert!(
        template::match_template(invalid.view(), patch.view(), MatchMethod::Correlation).is_err()
    );
    Ok(())
}

#[test]
fn all_current_lossless_assets_decode_to_identical_pixels() -> TestResult {
    let mut pending = vec![std::path::PathBuf::from("assets")];
    let mut images = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry.path().extension().is_some_and(|value| {
                value.eq_ignore_ascii_case("png") || value.eq_ignore_ascii_case("webp")
            }) {
                let bytes = match codec::load_unchanged(entry.path())? {
                    DecodedImage::Gray(image) => image.into_pixels(),
                    DecodedImage::Bgr(image) => image.into_pixels(),
                    DecodedImage::Bgra(image) => image.into_pixels(),
                };
                images.push(
                    json!({"path":entry.path(),"sha256":format!("{:x}",Sha256::digest(&bytes))}),
                );
                images.push(json!({"path":entry.path(),"mode":"gray","sha256":format!("{:x}",Sha256::digest(codec::load_gray(entry.path())?.bytes()))}));
            }
        }
    }
    assert!(!images.is_empty());
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"audit_decodes","images":images}),
    )?;
    assert_eq!(expected["count"].as_u64(), Some(images.len() as u64));
    assert_eq!(expected["differences"], json!([]));
    eprintln!(
        "Lossless image comparisons: {} (unchanged and grayscale)",
        images.len()
    );
    Ok(())
}

#[test]
fn jpeg_decode_uses_declared_pixel_bound() -> TestResult {
    let path = "assets/ui/town-loop/remastered-ko-window/references/save-exit-baseline.jpg";
    let frame = codec::load_color(path)?;
    let png = codec::encode_png(&frame)?;
    let hex: String = png.iter().map(|value| format!("{value:02x}")).collect();
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"decode_compare","path":path,"png_hex":hex}),
    )?;
    assert!(
        expected["max_difference"].as_u64().ok_or("difference")? <= 3,
        "{expected}"
    );
    Ok(())
}

#[test]
fn unicode_png_paths_round_trip_without_production_subprocesses() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let path = directory.path().join("한글-이미지.png");
    let frame = BgrFrame::from_vec(Size::new(11, 13)?, pattern(11, 13, 3))?;
    codec::save_png(&path, &frame)?;
    assert_eq!(codec::load_color(&path)?, frame);
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"decode","path":path,"mode":"color"}),
    )?;
    check(&frame, &expected, 0)
}

#[test]
fn explicit_grayscale_decode_matches_opencv() -> TestResult {
    for path in [
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/templates/encounters/summoner-robe-corpse-lower-band-right.png",
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/encounters/lightning-spire-lit.png",
        "assets/items/runes/icons/jah.png",
    ] {
        let expected = support::legacy_expectation(
            "vision-oracle",
            &json!({"op":"decode","path":path,"mode":"gray"}),
        )?;
        check(&codec::load_gray(path)?, &expected, 0)?;
        assert_eq!(
            codec::file_sha256(path)?,
            format!("{:x}", Sha256::digest(std::fs::read(path)?))
        );
    }
    Ok(())
}

#[test]
fn detector_preserves_rule_order_threshold_equality_and_peak_boxes() -> TestResult {
    use d2bot::common::detectors::{TemplateMatcher, TemplateRule};
    let directory = directory::TestDirectory::new()?;
    let absolute_path = directory.path().join("template.png");
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).canonicalize()?;
    let path = absolute_path.strip_prefix(root)?.to_path_buf();
    let template = BgrFrame::from_vec(Size::new(2, 3)?, [10, 60, 200].repeat(6))?;
    codec::save_png(&path, &template)?;
    let definitions = [
        ("second", 1.0),
        ("excluded", 1.0 + f64::EPSILON),
        ("first", 0.9),
    ];
    let matcher =
        TemplateMatcher::load(definitions.iter().map(|(name, threshold)| TemplateRule {
            name: (*name).into(),
            path: path.clone(),
            threshold: *threshold,
            action: "log".into(),
        }))?;
    let frame = BgrFrame::from_vec(Size::new(17, 19)?, pattern(17, 19, 3))?;
    let actual:Vec<_>=matcher.scan(frame.view())?.into_iter().map(|item|json!({"name":item.name,"confidence":item.confidence,"top_left":item.top_left,"bottom_right":item.bottom_right,"action":item.action})).collect();
    let rules:Vec<_>=definitions.into_iter().map(|(name,threshold)|json!({"name":name,"path":absolute_path,"threshold":threshold,"action":"log"})).collect();
    let expected = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"detector","width":17,"height":19,"pixels":frame.bytes(),"rules":rules}),
    )?;
    assert_eq!(json!(actual), expected);
    Ok(())
}

#[test]
fn jpeg_references_keep_presence_variants_and_approved_coordinate_bounds() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let mut pending = vec![std::path::PathBuf::from("assets")];
    let mut images = Vec::new();
    while let Some(folder) = pending.pop() {
        for entry in std::fs::read_dir(folder)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                pending.push(entry.path());
            } else if entry
                .path()
                .extension()
                .is_some_and(|value| value.eq_ignore_ascii_case("jpg"))
            {
                let output = directory.path().join(format!("jpeg-{}.png", images.len()));
                codec::save_png(&output, &codec::load_color(entry.path())?)?;
                images.push(json!({"original":entry.path(),"decoded":output}));
            }
        }
    }
    let mut paths: Vec<_> = images
        .iter()
        .flat_map(|image| [image["original"].clone(), image["decoded"].clone()])
        .collect();
    // 비교 기준이 읽는 공용 판독 자산도 변경 시 새 검증을 요구한다.
    for (folder, key) in [
        ("assets/ui/town-loop/remastered-ko-window", "templates"),
        ("assets/ui/automap", "areas"),
    ] {
        let folder = std::path::Path::new(folder);
        let manifest_path = folder.join("manifest.json");
        let manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
        paths.push(json!(manifest_path));
        for definition in manifest[key]
            .as_object()
            .ok_or("comparison asset definitions")?
            .values()
        {
            paths.push(json!(
                folder.join(definition["file"].as_str().ok_or("comparison asset file")?)
            ));
            if let Some(variants) = definition["variants"].as_array() {
                for variant in variants {
                    paths.push(json!(
                        folder.join(variant.as_str().ok_or("comparison variant")?)
                    ));
                }
            }
        }
    }
    let result = support::legacy_expectation(
        "vision-oracle",
        &json!({"op":"jpeg_reference_decisions","images":images,"paths":paths}),
    )?;
    eprintln!("JPEG reference evidence: {result}");
    let results = result.as_array().ok_or("results")?;
    assert_eq!(results.len(), 7);
    let mut changed_centers = 0;
    let mut changed_rounded_centers = 0;
    for item in results {
        assert!(
            item["maximum_pixel_difference"]
                .as_u64()
                .ok_or("pixel difference")?
                <= 3,
            "{item}"
        );
        assert!(
            item["maximum_score_delta"]
                .as_f64()
                .ok_or("score difference")?
                <= 7e-5,
            "{item}"
        );
        assert_eq!(item["decision_checks"], json!(22));
        for change in item["differences"].as_array().ok_or("differences")? {
            let mut old = change["old"]
                .as_object()
                .ok_or("detection presence changed")?
                .clone();
            let mut new = change["new"]
                .as_object()
                .ok_or("detection presence changed")?
                .clone();
            let mut rounded_changed = false;
            for axis in ["x", "y"] {
                let a = old
                    .remove(axis)
                    .and_then(|v| v.as_f64())
                    .ok_or("old center")?;
                let b = new
                    .remove(axis)
                    .and_then(|v| v.as_f64())
                    .ok_or("new center")?;
                assert!((a - b).abs() <= 1.0, "{change}");
                let delta = (a.round_ties_even() - b.round_ties_even()).abs();
                assert!(delta <= 1.0, "{change}");
                rounded_changed |= delta != 0.0;
            }
            assert_eq!(old, new, "selected variant/evidence changed: {change}");
            changed_centers += 1;
            changed_rounded_centers += usize::from(rounded_changed);
        }
    }
    assert!(changed_centers <= 3);
    assert!(changed_rounded_centers <= 1);
    Ok(())
}
