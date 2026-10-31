use super::{ToolError, invalid, json_file};
use crate::{
    common::{
        cli::{self, Kind, OptionSpec as Opt},
        frame::{BgrFrame, PixelRect, Size},
        scene_motion::{BackgroundMotionObserver, LandmarkMask, MotionLimits},
        vision_ops::{codec, drawing},
    },
    runs::summoner::{
        navigation::{
            ArcanePathObserver, ChooseOptions, PathContext, PathSettings, TeleportGeometry,
            TerrainPalette, Wing,
        },
        navigation_execution::PathMotionMasks,
    },
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{ffi::OsString, io::Write, path::Path, time::Instant};
pub const HELP: &str = "Usage: d2bot arcane-navigation-replay --manifest JSON --calibration JSON [--overlays NEW_DIRECTORY]\nInput-free terrain/background-motion calibration.\n";
const OPTIONS: &[Opt] = &[
    Opt::new("--manifest", Kind::Value).required(),
    Opt::new("--calibration", Kind::Value).required(),
    Opt::new("--overlays", Kind::Value),
];
pub fn run(
    input: impl IntoIterator<Item = OsString>,
    output: &mut impl Write,
) -> Result<u8, ToolError> {
    let args = cli::parse(input, OPTIONS, &[])?;
    if args.help {
        output.write_all(HELP.as_bytes())?;
        return Ok(0);
    }
    let manifest = args
        .path("--manifest")
        .ok_or_else(|| invalid("manifest required"))?;
    let calibration = json_file(
        &args
            .path("--calibration")
            .ok_or_else(|| invalid("calibration required"))?,
        1024 * 1024,
    )?;
    let result = replay(&manifest, &calibration, args.path("--overlays").as_deref())?;
    serde_json::to_writer(&mut *output, &result)?;
    writeln!(output)?;
    Ok(0)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Motion {
    minimum_matches: usize,
    minimum_inlier_ratio: f64,
    maximum_error: f64,
    #[serde(default = "half")]
    scale: f64,
    landmark_viewport: Option<[usize; 4]>,
    actor_exclusion: Option<[usize; 4]>,
}
fn half() -> f64 {
    0.5
}
fn required<'a>(value: &'a Value, key: &str) -> Result<&'a Value, ToolError> {
    value
        .get(key)
        .ok_or_else(|| invalid(format!("missing {key}")))
}
pub fn replay(
    manifest_path: &Path,
    calibration: &Value,
    overlays: Option<&Path>,
) -> Result<Value, ToolError> {
    let manifest = json_file(manifest_path, 1024 * 1024)?;
    let [width, height]: [usize; 2] =
        serde_json::from_value(required(&manifest, "frame_size")?.clone())?;
    if width == 0 || height == 0 || width.checked_mul(height).is_none_or(|n| n > 8_000_000) {
        return Err(invalid("bounded native frame size required"));
    }
    let size = Size::new(width, height)?;
    if calibration["mode"] != "input_free_calibration"
        || calibration["frame_size"] != manifest["frame_size"]
    {
        return Err(invalid(
            "explicit input-free calibration for this frame size required",
        ));
    }
    let cases = manifest["cases"]
        .as_array()
        .filter(|v| (1..=100).contains(&v.len()))
        .ok_or_else(|| invalid("one to 100 source cases required"))?;
    let terrain: TerrainPalette = serde_json::from_value(
        calibration
            .get("terrain")
            .or_else(|| manifest.get("terrain"))
            .ok_or_else(|| invalid("terrain required"))?
            .clone(),
    )?;
    let teleport = calibration
        .get("teleport")
        .filter(|v| !v.is_null())
        .map(TeleportGeometry::from_value)
        .transpose()?;
    let observer = ArcanePathObserver::new(
        PathSettings::from_value(&calibration["path"])?,
        Some(terrain),
        teleport,
        None,
    )?;
    let raw: Motion = serde_json::from_value(calibration["motion"].clone())?;
    let landmark = match (raw.landmark_viewport, raw.actor_exclusion) {
        (Some([l, t, r, b]), Some(exclusion)) => Some(LandmarkMask {
            viewport: PixelRect::new(l, t, r, b)?,
            actor_exclusion: exclusion,
        }),
        (None, None) => None,
        _ => {
            return Err(invalid(
                "Landmark viewport and actor exclusion must be supplied together",
            ));
        }
    };
    let mut motion = BackgroundMotionObserver::new(
        MotionLimits::new(
            raw.minimum_matches,
            raw.minimum_inlier_ratio,
            raw.maximum_error,
            raw.scale,
        )?,
        landmark,
    );
    let anchor: [f64; 2] = serde_json::from_value(calibration["anchor"].clone())?;
    let wing: Wing = serde_json::from_value(calibration["wing"].clone())?;
    if let Some(overlays) = overlays {
        std::fs::create_dir_all(
            overlays
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )?;
        std::fs::create_dir(overlays)?;
    }
    let mut results = Vec::with_capacity(cases.len());
    let mut previous: Option<(Value, f64)> = None;
    for (sequence, case) in cases.iter().enumerate() {
        let [x0, y0, x1, y1]: [usize; 4] = serde_json::from_value(case["viewport"].clone())?;
        let viewport = PixelRect::new(x0, y0, x1, y1)?.checked_in(size)?;
        let path = case["path"]
            .as_str()
            .ok_or_else(|| invalid("case path required"))?;
        let mut crop =
            codec::load_color(manifest_path.parent().unwrap_or(Path::new(".")).join(path))?;
        if crop.size() != Size::new(viewport.width(), viewport.height())? {
            return Err(invalid(
                "source crop and viewport must match without rescaling",
            ));
        }
        let source = required(case, "provenance")?;
        let episode = required(source, "episode")?;
        let source_time = source["seconds_from_record_start"]
            .as_f64()
            .ok_or_else(|| invalid("source time must be numeric"))?;
        if previous
            .as_ref()
            .is_none_or(|(e, t)| e != episode || source_time <= *t)
        {
            motion.reset();
        }
        let mut frame = BgrFrame::zeros(size)?;
        for row in 0..viewport.height() {
            let destination = ((y0 + row) * width + x0) * 3;
            let start = row * viewport.width() * 3;
            frame.pixels_mut()[destination..destination + viewport.width() * 3]
                .copy_from_slice(&crop.pixels()[start..start + viewport.width() * 3]);
        }
        let began = Instant::now();
        let view = observer.observe(
            frame.view(),
            PathContext {
                sequence: sequence as u64,
                generation: 0,
                captured_at: source_time,
                frame_size: size,
                viewport,
                anchor,
            },
        )?;
        let choice = observer.choose(&view, wing, ChooseOptions::default())?;
        let masks = PathMotionMasks::new(&view)?;
        let displacement = motion.observe(frame.view(), masks.view(), None)?;
        results.push(json!({"case":required(case,"id")?,"layout":case.get("layout"),"source":source,"choice":choice,"motion":displacement,"vision_seconds":began.elapsed().as_secs_f64()}));
        if let Some(overlays) = overlays {
            drawing::circle(
                &mut crop,
                [
                    (anchor[0] - x0 as f64) as i32,
                    (anchor[1] - y0 as f64) as i32,
                ],
                10,
                false,
                [0, 0, 255],
            )?;
            for point in choice.path {
                drawing::circle(
                    &mut crop,
                    [
                        (point[0] - x0 as f64).round_ties_even() as i32,
                        (point[1] - y0 as f64).round_ties_even() as i32,
                    ],
                    3,
                    true,
                    [255, 0, 255],
                )?;
            }
            if let Some(aim) = choice.aim {
                drawing::circle(
                    &mut crop,
                    [aim[0] - x0 as i32, aim[1] - y0 as i32],
                    8,
                    false,
                    [255, 255, 0],
                )?;
            }
            codec::save_png(overlays.join(format!("case-{sequence:03}.png")), &crop)?;
        }
        previous = Some((episode.clone(), source_time));
    }
    Ok(
        json!({"mode":"input_free_calibration","inputs_sent":0,"revision":required(&manifest,"revision")?,"results":results}),
    )
}
