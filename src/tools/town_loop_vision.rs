use super::{ToolError, invalid};
use crate::{
    actions::town_loop::{
        Destination, Difficulty, REFERENCE_SIZE, TownActionKind, TownOptions, TownPlanner,
        TownSight, TownVision,
    },
    common::{
        capture::{CaptureSettings, ScreenCapture},
        cli::{self, Kind, OptionSpec as Opt, UsageError},
        frame::PixelRect,
        vision_ops::{
            codec, color, filters,
            template::{self, MatchMethod},
        },
    },
    platform::windows::Windows,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{ffi::OsString, io::Write, path::Path, sync::Arc};
pub const DIRECTORY: &str = "assets/ui/town-loop/remastered-ko-window";
pub const HELP: &str = "Usage: d2bot town-loop-vision [--capture-frame PNG | --calibrate-from DIRECTORY | --calibrate-frame PNG --scene NAME | --frame PNG --state JSON] [--repeat INTEGER] [--difficulty hell] [--destination town]\nScreen-only calibration/planner bridge; never sends input.\n";
const OPTIONS: &[Opt] = &[
    Opt::new("--calibrate-from", Kind::Value),
    Opt::new("--calibrate-frame", Kind::Value),
    Opt::new("--scene", Kind::Value),
    Opt::new("--capture-frame", Kind::Value),
    Opt::new("--frame", Kind::Value),
    Opt::new("--state", Kind::Value),
    Opt::new("--repeat", Kind::Integer),
    Opt::new("--difficulty", Kind::Value),
    Opt::new("--destination", Kind::Value),
];
fn mkdir_parent(path: &Path) -> Result<(), ToolError> {
    std::fs::create_dir_all(
        path.parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )?;
    Ok(())
}
fn posix(path: &Path) -> Result<String, ToolError> {
    Ok(path
        .to_str()
        .ok_or_else(|| invalid("output path must be Unicode text"))?
        .replace('\\', "/"))
}
pub fn run(
    input: impl IntoIterator<Item = OsString>,
    output: &mut impl Write,
) -> Result<u8, ToolError> {
    let args = cli::parse(input, OPTIONS, &[])?;
    if args.help {
        output.write_all(HELP.as_bytes())?;
        return Ok(0);
    }
    let root = crate::app::runtime_root()?;
    let result = if let Some(path) = args
        .path("--capture-frame")
        .filter(|p| !p.as_os_str().is_empty())
    {
        let settings = CaptureSettings {
            window_title: Some("Diablo II: Resurrected".into()),
            window_title_mode: "exact".into(),
            capture_backend: "window".into(),
            ..Default::default()
        };
        let mut capture = ScreenCapture::new(Windows::new()?, settings, &root)?;
        let packet = capture.grab().map_err(ToolError::from);
        let cleanup = capture.close().map_err(ToolError::from);
        let packet = match (packet, cleanup) {
            (Ok(p), Ok(())) => p,
            (Err(e), Ok(())) | (Ok(_), Err(e)) => return Err(e),
            (Err(e), Err(c)) => return Err(invalid(format!("{e}; capture close failed: {c}"))),
        };
        mkdir_parent(&path)?;
        codec::save_png(&path, &packet.frame)?;
        json!({"frame":posix(&path)?,"size":[packet.frame.size().width(),packet.frame.size().height()]})
    } else if let Some(path) = args
        .path("--calibrate-from")
        .filter(|p| !p.as_os_str().is_empty())
    {
        calibrate(&path, Path::new(DIRECTORY))?;
        json!({"calibration":DIRECTORY})
    } else if let Some(path) = args
        .path("--calibrate-frame")
        .filter(|p| !p.as_os_str().is_empty())
    {
        let scene = args.text("--scene")?.unwrap_or("");
        calibrate_scene(&root, &path, scene, Path::new(DIRECTORY))?;
        json!({"calibrated_scene":scene,"frame":args.text("--calibrate-frame")?})
    } else {
        let frame = args
            .path("--frame")
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                ToolError::Usage(UsageError(
                    "--frame and --state are required for the observation bridge".into(),
                ))
            })?;
        let state = args
            .path("--state")
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or_else(|| {
                ToolError::Usage(UsageError(
                    "--frame and --state are required for the observation bridge".into(),
                ))
            })?;
        let repeat = args
            .integer("--repeat")
            .and_then(|n| n.as_u64())
            .unwrap_or(if args.has("--repeat") { 0 } else { 1 });
        let difficulty: Difficulty =
            serde_json::from_value(json!(args.text("--difficulty")?.unwrap_or("hell")))?;
        let destination: Destination =
            serde_json::from_value(json!(args.text("--destination")?.unwrap_or("town")))?;
        step_bridge(
            &root,
            &frame,
            &state,
            TownOptions {
                repeat_count: u8::try_from(repeat).unwrap_or(0),
                difficulty,
                destination,
                ..Default::default()
            },
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| invalid(e.to_string()))?
                .as_secs_f64(),
        )?
    };
    serde_json::to_writer(&mut *output, &result)?;
    writeln!(output)?;
    Ok(0)
}
pub fn step_bridge(
    root: &Path,
    frame_path: &Path,
    state: &Path,
    options: TownOptions,
    now: f64,
) -> Result<Value, ToolError> {
    let mut vision = TownVision::load(root)?;
    let mut planner = TownPlanner::new(options, |name| vision.calibrated(name))?;
    if state.exists() {
        let raw: Value = serde_json::from_slice(&std::fs::read(state)?)?;
        planner.restore(
            raw.as_object()
                .ok_or_else(|| invalid("saved town state must be object"))?,
        )?;
    }
    let frame = Arc::new(
        codec::load_color(frame_path)
            .map_err(|e| invalid(format!("Input frame is unreadable: {e}")))?,
    );
    vision.set_frame(frame.clone());
    let mut action = planner.step(&mut vision, now)?;
    mkdir_parent(state)?;
    std::fs::write(state, serde_json::to_vec_pretty(&planner.data)?)?;
    if let Some(action) = &mut action
        && matches!(action.kind, TownActionKind::Click | TownActionKind::Hold)
        && let Some(point) = &mut action.point
    {
        point[0] = (point[0] as f64 * frame.size().width() as f64 / REFERENCE_SIZE[0] as f64)
            .round_ties_even() as i64;
        point[1] = (point[1] as f64 * frame.size().height() as f64 / REFERENCE_SIZE[1] as f64)
            .round_ties_even() as i64;
    }
    Ok(json!({"action":action,"state":planner.data}))
}
struct Definition {
    name: &'static str,
    source: &'static str,
    crop: [usize; 4],
    search: [usize; 4],
    threshold: f64,
}
const BASELINE: &[Definition] = &[
    Definition {
        name: "play",
        source: "01-character-select.png",
        crop: [472, 665, 580, 690],
        search: [400, 620, 850, 720],
        threshold: 0.84,
    },
    Definition {
        name: "difficulty_header",
        source: "02-difficulty.png",
        crop: [589, 278, 678, 297],
        search: [520, 255, 735, 320],
        threshold: 0.88,
    },
    Definition {
        name: "normal",
        source: "02-difficulty.png",
        crop: [566, 323, 700, 346],
        search: [545, 305, 721, 358],
        threshold: 0.86,
    },
    Definition {
        name: "nightmare",
        source: "02-difficulty.png",
        crop: [566, 367, 700, 390],
        search: [545, 357, 721, 402],
        threshold: 0.86,
    },
    Definition {
        name: "hell",
        source: "02-difficulty.png",
        crop: [566, 411, 700, 434],
        search: [545, 399, 721, 450],
        threshold: 0.86,
    },
    Definition {
        name: "hud",
        source: "03-act1-spawn.png",
        crop: [616, 703, 650, 746],
        search: [605, 690, 665, 750],
        threshold: 0.86,
    },
    Definition {
        name: "area_act1",
        source: "04-act1-map.png",
        crop: [1168, 70, 1249, 85],
        search: [1120, 64, 1257, 93],
        threshold: 0.79,
    },
    Definition {
        name: "area_act2",
        source: "09-act2-town.png",
        crop: [1181, 70, 1249, 85],
        search: [1120, 64, 1257, 93],
        threshold: 0.79,
    },
    Definition {
        name: "waypoint_header",
        source: "07-waypoint-act1.png",
        crop: [230, 123, 315, 144],
        search: [190, 110, 355, 152],
        threshold: 0.88,
    },
    Definition {
        name: "list_act1",
        source: "07-waypoint-act1.png",
        crop: [198, 240, 277, 306],
        search: [190, 234, 290, 312],
        threshold: 0.82,
    },
    Definition {
        name: "list_act2",
        source: "08-waypoint-act2.png",
        crop: [198, 240, 277, 306],
        search: [190, 234, 290, 312],
        threshold: 0.82,
    },
    Definition {
        name: "world_act1_waypoint",
        source: "05-act1-waypoint-world.png",
        crop: [1122, 282, 1211, 322],
        search: [420, 110, 1256, 570],
        threshold: 0.72,
    },
    Definition {
        name: "save_exit",
        source: "11-save-exit-menu.png",
        crop: [550, 336, 715, 361],
        search: [500, 310, 770, 390],
        threshold: 0.86,
    },
];
pub fn calibrate(evidence: &Path, directory: &Path) -> Result<(), ToolError> {
    std::fs::create_dir_all(directory)?;
    let manifest_path = directory.join("manifest.json");
    let previous: Value = if manifest_path.exists() {
        serde_json::from_slice(&std::fs::read(&manifest_path)?)?
    } else {
        json!({"templates":{}})
    };
    let mut manifest =
        json!({"reference_size":REFERENCE_SIZE,"source":posix(evidence)?,"templates":{}});
    for definition in BASELINE {
        let source = codec::load_color(evidence.join("frames/raw").join(definition.source))?;
        if [source.size().width(), source.size().height()] != REFERENCE_SIZE {
            return Err(invalid(format!(
                "Expected baseline {REFERENCE_SIZE:?}: {}",
                definition.source
            )));
        }
        let [x1, y1, x2, y2] = definition.crop;
        let filename = previous["templates"][definition.name]["file"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "templates/{}-ko-{}x{}.png",
                    definition.name.replace('_', "-"),
                    x2 - x1,
                    y2 - y1
                )
            });
        let path = directory.join(&filename);
        mkdir_parent(&path)?;
        let crop = source
            .view()
            .crop(PixelRect::new(x1, y1, x2, y2)?)?
            .to_owned()?;
        let encoded = codec::encode_png(&crop)?;
        std::fs::write(&path, &encoded)?;
        manifest["templates"][definition.name] = json!({"file":filename,"crop":definition.crop,"search":definition.search,"threshold":definition.threshold,"source_frame":definition.source,"asset_sha256":format!("{:x}",Sha256::digest(&encoded))});
    }
    std::fs::write(manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}
pub fn calibrate_scene(
    root: &Path,
    frame_path: &Path,
    scene: &str,
    directory: &Path,
) -> Result<(), ToolError> {
    let names: &[&str] = match scene {
        "character" => &["play"],
        "difficulty" => &["difficulty_header", "normal", "nightmare", "hell"],
        "act1" => &["hud", "area_act1"],
        "act2" => &["hud", "area_act2"],
        "waypoint1" => &["waypoint_header", "list_act1"],
        "waypoint2" => &["waypoint_header", "list_act2"],
        "exit" => &["save_exit"],
        "world_waypoint1" => &["world_act1_waypoint"],
        _ => return Err(invalid("Unknown calibration scene.")),
    };
    let mut vision = TownVision::load_directory(root, directory)?;
    let manifest_path = directory.join("manifest.json");
    let mut manifest: Value = serde_json::from_slice(&std::fs::read(&manifest_path)?)?;
    let frame = Arc::new(codec::load_color(frame_path)?);
    vision.set_frame(frame);
    vision.normalize()?;
    let gray = color::gray(
        vision
            .normalized()
            .ok_or_else(|| invalid("normalized calibration frame required"))?
            .view(),
    )?;
    let mut originals = std::collections::BTreeMap::new();
    for name in names {
        let definition = &manifest["templates"][name];
        let mut paths = vec![
            definition["file"]
                .as_str()
                .ok_or_else(|| invalid("template file required"))?,
        ];
        if let Some(variants) = definition.get("variants") {
            for path in variants
                .as_array()
                .ok_or_else(|| invalid("variants must be list"))?
            {
                paths.push(
                    path.as_str()
                        .ok_or_else(|| invalid("variant path required"))?,
                );
            }
        }
        originals.insert(
            *name,
            paths
                .into_iter()
                .map(|path| codec::load_gray(directory.join(path)))
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    for name in names {
        let definition = &mut manifest["templates"][name];
        let [x1, y1, x2, y2]: [usize; 4] = serde_json::from_value(definition["search"].clone())?;
        let region = filters::separable_q8(
            gray.view().crop(PixelRect::new(x1, y1, x2, y2)?)?,
            &[61, 134, 61],
        )?;
        let mut best = None;
        for template in originals
            .get(name)
            .ok_or_else(|| invalid("original variants required"))?
        {
            let filtered = filters::separable_q8(template.view(), &[61, 134, 61])?;
            let scores = template::match_template(
                region.view(),
                filtered.view(),
                MatchMethod::NormalizedCoefficient,
            )?;
            let peak = template::maximum(&scores)
                .ok_or_else(|| invalid("calibration score unavailable"))?;
            if best.is_none_or(|(score, _, _)| peak.score > score) {
                best = Some((peak.score, peak.location, template.size()));
            }
        }
        let (_, [px, py], size) = best.ok_or_else(|| invalid("calibration variant required"))?;
        let crop = gray
            .view()
            .crop(PixelRect::new(
                x1 + px,
                y1 + py,
                x1 + px + size.width(),
                y1 + py + size.height(),
            )?)?
            .to_owned()?;
        let variants = definition
            .as_object_mut()
            .ok_or_else(|| invalid("template definition must be object"))?
            .entry("variants")
            .or_insert(json!([]))
            .as_array_mut()
            .ok_or_else(|| invalid("variants must be list"))?;
        let mut filename = None;
        for suffix in ["-reviewed-window", "-window"] {
            if let Some(path) = variants.iter().filter_map(Value::as_str).find(|path| {
                Path::new(path)
                    .file_stem()
                    .and_then(|n| n.to_str())
                    .is_some_and(|stem| stem.ends_with(suffix))
            }) {
                filename = Some(path.to_owned());
                break;
            }
        }
        let filename = filename.unwrap_or_else(|| {
            format!(
                "templates/{}-window-ko-{}x{}.png",
                name.replace('_', "-"),
                size.width(),
                size.height()
            )
        });
        if !variants.iter().any(|v| v == &filename) {
            variants.push(json!(filename));
        }
        let path = directory.join(&filename);
        mkdir_parent(&path)?;
        let encoded = codec::encode_gray_png(&crop)?;
        std::fs::write(path, &encoded)?;
        let definition = definition
            .as_object_mut()
            .ok_or_else(|| invalid("template definition required"))?;
        let hashes = definition
            .entry("variant_asset_sha256")
            .or_insert(json!({}))
            .as_object_mut()
            .ok_or_else(|| invalid("variant hashes must be object"))?;
        hashes.insert(filename, json!(format!("{:x}", Sha256::digest(&encoded))));
        definition.entry("variant_sources").or_insert(json!([])).as_array_mut().ok_or_else(||invalid("variant sources must be list"))?.push(json!({"file":posix(frame_path)?,"scene":scene,"crop":[x1+px,y1+py,x1+px+size.width(),y1+py+size.height()]}));
    }
    std::fs::write(manifest_path, serde_json::to_vec_pretty(&manifest)?)?;
    Ok(())
}
