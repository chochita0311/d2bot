use d2bot::common::{
    frame::{BgrFrame, Size},
    recording::{
        Mpeg4Writer, OutputMode,
        avi::{AviCodec, AviWriter},
        mpeg4::Mpeg4Encoder,
    },
};
use serde_json::json;
use std::{error::Error, fs::File};
#[path = "support/directory.rs"]
mod directory;
mod support;
type TestResult = Result<(), Box<dyn Error>>;
fn frame(width: usize, height: usize, index: usize) -> Result<BgrFrame, Box<dyn Error>> {
    let mut pixels = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let shifted = (x + index * 2) % width;
            let mut color = [
                (shifted * 190 / width + 20) as u8,
                (y * 160 / height + 30) as u8,
                90,
            ];
            if (x as i64 - index as i64 * 2).unsigned_abs() < 8 && (10..25).contains(&y) {
                color = [20, 80, 220];
            }
            if (24..32).contains(&index) {
                color = [color[2], color[0], color[1]];
            }
            pixels.extend_from_slice(&color);
        }
    }
    Ok(BgrFrame::from_vec(Size::new(width, height)?, pixels)?)
}
#[test]
fn independent_decoder_checks_i_p_timebase_motion_cuts_and_drift() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    for (width, height, count) in [
        (64, 48, 1),
        (64, 48, 40),
        (65, 49, 1),
        (65, 49, 40),
        (640, 480, 4),
    ] {
        let path = directory
            .path()
            .join(format!("mpeg4-{width}-{height}-{count}.avi"));
        let mut writer =
            Mpeg4Writer::open(&path, Size::new(width, height)?, 4, OutputMode::CreateNew)?;
        let mut expected = vec![];
        for index in 0..count {
            let frame = frame(width, height, index)?;
            writer.write(&frame)?;
            expected.push(json!({"width":width,"height":height,"bgr":frame.bytes()}));
        }
        writer.finish()?;
        drop(writer);
        let output = support::legacy_expectation(
            "recording-oracle",
            &json!({"paths":[path],"expected_frames":expected,"compare_source":true}),
        )?;
        let output = &output[0];
        assert_eq!(output["decoded_frames"], count);
        assert_eq!(output["reported_frames"], count);
        assert_eq!(output["width"], width);
        assert_eq!(output["height"], height);
        assert_eq!(output["fps"], 8.0);
        let psnr = output["psnr_db"].as_f64().ok_or("PSNR")?;
        let source_psnr = output["source_xvid"]["psnr_db"]
            .as_f64()
            .ok_or("source PSNR")?;
        // 급격한 단색 경계의 4:2:0 손실은 원본도 발생한다. 동일 공통 영역에서 차이를 제한한다.
        assert!(psnr >= 26.0 && psnr >= source_psnr - 1.0, "{output}");
    }
    Ok(())
}
#[test]
fn p_frame_indexes_remain_seekable_across_riff_segments() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let path = directory.path().join("rollover.avi");
    let size = Size::new(64, 48)?;
    let mut encoder = Mpeg4Encoder::new(size, 4)?;
    let mut avi =
        AviWriter::with_codec_and_segment_target(File::create(&path)?, size, AviCodec::Mpeg4, 1)?;
    for index in 0..20 {
        let frame = frame(64, 48, index)?;
        let packet = encoder.encode(&frame)?;
        assert_eq!(packet.keyframe, index == 0 || index == 16);
        avi.write_packet(packet.bytes, packet.keyframe)?;
    }
    avi.finish()?;
    drop(avi);
    let output = support::legacy_expectation("recording-oracle", &json!({"paths":[path]}))?;
    assert_eq!(output[0]["decoded_frames"], 20);
    assert_eq!(output[0]["reported_frames"], 20);
    assert_eq!(output[0]["fps"], 8.0);
    Ok(())
}

#[test]
fn long_duplicate_reference_and_scene_cuts_do_not_accumulate_error() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let path = directory.path().join("long.avi");
    let size = Size::new(65, 49)?;
    let mut encoder = Mpeg4Encoder::new(size, 4)?;
    let mut avi = AviWriter::for_codec(File::create(&path)?, size, AviCodec::Mpeg4)?;
    let mut expected = vec![];
    for index in 0..320 {
        let color = if index < 128 {
            [24, 40, 64]
        } else if index < 255 {
            [210, 220, 230]
        } else {
            [100, 120, 140]
        };
        let frame = BgrFrame::from_vec(size, color.repeat(size.width() * size.height()))?;
        let packet = encoder.encode(&frame)?;
        if index == 128 || index == 255 {
            assert!(packet.keyframe);
        }
        if !packet.keyframe {
            assert!(packet.bytes.len() < 32, "Duplicate picture was not compact");
        }
        avi.write_packet(packet.bytes, packet.keyframe)?;
        expected.push(json!({"width":size.width(),"height":size.height(),"bgr":frame.bytes()}));
    }
    avi.finish()?;
    drop(avi);
    let output = support::legacy_expectation(
        "recording-oracle",
        &json!({"paths":[path],"expected_frames":expected}),
    )?;
    assert_eq!(output[0]["decoded_frames"], 320);
    assert_eq!(output[0]["reported_frames"], 320);
    for mse in output[0]["frame_mse"].as_array().ok_or("frame errors")? {
        assert!(mse.as_f64().ok_or("MSE")? <= 9.0, "{output}");
    }
    Ok(())
}

#[test]
fn translated_texture_checks_both_axes_chroma_interpolation_and_motion_signs() -> TestResult {
    let directory = directory::TestDirectory::new()?;
    let size = Size::new(96, 80)?;
    for (dx, dy) in [(3i32, 5i32), (-3, -5), (15, -16)] {
        let path = directory.path().join(format!("motion-{dx}-{dy}.avi"));
        let mut writer = Mpeg4Writer::open(&path, size, 4, OutputMode::CreateNew)?;
        let mut expected = vec![];
        for index in 0..40 {
            let mut bytes = Vec::with_capacity(size.byte_len()?);
            for y in 0..size.height() {
                for x in 0..size.width() {
                    let a = (x as i32 - index * dx).rem_euclid(size.width() as i32) as usize;
                    let b = (y as i32 - index * dy).rem_euclid(size.height() as i32) as usize;
                    let grey = ((a * 19 + b * 31 + (a * b) % 71) % 140 + 40) as u8;
                    bytes.extend_from_slice(&[grey - (a / 8) as u8, grey, grey + (b / 8) as u8]);
                }
            }
            let frame = BgrFrame::from_vec(size, bytes)?;
            writer.write(&frame)?;
            expected.push(json!({"width":size.width(),"height":size.height(),"bgr":frame.bytes()}));
        }
        writer.finish()?;
        drop(writer);
        let output = support::legacy_expectation(
            "recording-oracle",
            &json!({"paths":[path],"expected_frames":expected}),
        )?;
        assert_eq!(output[0]["decoded_frames"], 40);
        assert!(
            output[0]["psnr_db"].as_f64().ok_or("PSNR")? >= 34.0,
            "{output}"
        );
    }
    Ok(())
}

#[test]
fn quantizer_limits_minimal_frames_and_changed_dimensions_are_explicit() -> TestResult {
    let size = Size::new(1, 1)?;
    for q in [0, 1, 32, 255] {
        assert!(Mpeg4Encoder::new(size, q).is_err());
    }
    assert!(Mpeg4Encoder::new(Size::new(8192, 1)?, 4).is_err());
    let directory = directory::TestDirectory::new()?;
    for q in [2, 31] {
        let path = directory.path().join(format!("one-pixel-q{q}.avi"));
        let mut writer = Mpeg4Writer::open(&path, size, q, OutputMode::CreateNew)?;
        writer.write(&BgrFrame::from_vec(size, vec![40, 100, 180])?)?;
        assert!(writer.write(&frame(2, 1, 0)?).is_err());
        writer.finish()?;
        drop(writer);
        let output = support::legacy_expectation(
            "recording-oracle",
            &json!({"paths":[path],"pixels":true}),
        )?;
        assert_eq!(output[0]["width"], 1);
        assert_eq!(output[0]["height"], 1);
        assert_eq!(output[0]["decoded_frames"], 1);
        for (channel, expected) in [40i64, 100, 180].into_iter().enumerate() {
            let actual = output[0]["pixels"][0][0][0][channel]
                .as_i64()
                .ok_or("pixel")?;
            assert!((actual - expected).abs() <= 12, "{output}");
        }
    }
    Ok(())
}
