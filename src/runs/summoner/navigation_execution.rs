//! 현재 바닥 계획과 단발 이동 실행. 영상 좌표를 desktop 좌표로 바꾸는 책임은 호출자에게 있다.
mod dispatch;
mod landing;
use super::{
    camera_ground::{CameraOnlyGround, GroundObserver},
    navigation::{
        ArcanePathObserver, AutomapOverlay, ChooseOptions, PathChoice, PathContext, PathSettings,
        PathView, TeleportGeometry, TerrainPalette, Wing,
    },
    scene_vision::{FlashGroundObserver, GroundOptions, GroundReading, GroundTracking},
};
use crate::common::{
    error::ObservationError,
    frame::{BgrFrame, Image, PixelRect},
    observation_assets::AssetOwner,
    scene_motion::MotionView,
    time::MonotonicClock,
    vision_ops::codec,
};
pub use dispatch::{
    DispatchDiagnostics, DispatchOptions, PointerPreparation, StepEnvironment, StepGuards,
    StepRequest, dispatch_planned_step,
};
pub use landing::{
    LandingEnvironment, LandingRequest, LandingResult, LandingSample, observe_step_landing,
};
use serde::Deserialize;
use serde_json::Value;
use std::{path::Path, sync::Arc};

#[derive(Deserialize)]
struct GroundManifest {
    marker: Marker,
}
#[derive(Deserialize)]
struct Marker {
    path: String,
    search_box: [usize; 4],
    anchor_offset: [usize; 2],
    minimum_score: f64,
    #[serde(default)]
    variants: Vec<String>,
    #[serde(default = "empty_body")]
    body: Value,
    tracking: Option<GroundTracking>,
}
fn empty_body() -> Value {
    serde_json::json!({})
}
#[derive(Deserialize)]
struct Body {
    #[serde(default)]
    templates: Vec<String>,
    box_offset: [i32; 4],
    #[serde(default = "body_threshold")]
    minimum_score: f64,
}
fn body_threshold() -> f64 {
    0.82
}
pub fn load_arcane_step_planner(
    root: impl AsRef<Path>,
    automap: bool,
    character_class: &str,
) -> Result<(Box<dyn GroundObserver>, ArcanePathObserver), ObservationError> {
    let root = root.as_ref();
    let owner = AssetOwner::new(&root.join("assets/character/sorceress/remastered-1922x1140"))?;
    let marker: GroundManifest =
        serde_json::from_slice(&std::fs::read(owner.path("manifest.json")?)?)?;
    let marker = marker.marker;
    let [l, t, r, b] = marker.search_box;
    let mut options = GroundOptions::new(PixelRect::new(l, t, r, b)?, marker.anchor_offset);
    options.minimum_score = marker.minimum_score;
    options.tracking = marker.tracking;
    for path in marker.variants {
        options
            .variants
            .push(codec::load_color(owner.path(&path)?)?);
    }
    if !marker
        .body
        .as_object()
        .ok_or_else(|| ObservationError::invalid("marker body must be an object"))?
        .is_empty()
    {
        let body: Body = serde_json::from_value(marker.body)?;
        options.body_box = Some(body.box_offset);
        options.body_minimum_score = body.minimum_score;
        for path in body.templates {
            options
                .body_templates
                .push(codec::load_color(owner.path(&path)?)?);
        }
    }
    let ground = FlashGroundObserver::new(codec::load_color(owner.path(&marker.path)?)?, options)?;
    let calibration: Value = serde_json::from_slice(&std::fs::read(root.join(
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/navigation-calibration.json",
    ))?)?;
    let settings = PathSettings::from_value(&calibration["path"])?.with_seed_step(96.0, 120.0)?;
    let terrain: TerrainPalette = serde_json::from_value(calibration["terrain"].clone())?;
    let teleport = if calibration["teleport"].is_null() {
        None
    } else {
        Some(TeleportGeometry::from_value(&calibration["teleport"])?)
    };
    let overlay = if automap && !calibration["automap_overlay"].is_null() {
        Some(serde_json::from_value::<AutomapOverlay>(
            calibration["automap_overlay"].clone(),
        )?)
    } else {
        None
    };
    let path = ArcanePathObserver::new(settings, Some(terrain), teleport, overlay)?;
    let ground: Box<dyn GroundObserver> = if character_class == "sorceress" {
        Box::new(ground)
    } else {
        Box::new(CameraOnlyGround)
    };
    Ok((ground, path))
}
#[derive(Default)]
pub struct PlanOptions<'a> {
    pub hub_point: Option<[f64; 2]>,
    pub choose: ChooseOptions<'a>,
    pub allow_no_aim: bool,
    pub ground_reading: Option<&'a GroundReading>,
}
pub struct StepPlan {
    pub choice: PathChoice,
    pub ground: GroundReading,
    pub view: PathView,
}
pub fn plan_arcane_step(
    frame: &Arc<BgrFrame>,
    wing: Wing,
    ground: &mut dyn GroundObserver,
    path: &ArcanePathObserver,
    captured_at: f64,
    options: PlanOptions<'_>,
    clock: &dyn MonotonicClock,
) -> Result<StepPlan, ObservationError> {
    let reading = match options.ground_reading {
        Some(reading) => reading.clone(),
        None => ground.observe(frame, Some(captured_at))?,
    };
    let point = reading
        .point
        .ok_or_else(|| ObservationError::invalid(reading.reason.clone()))?;
    let adjusted = if options.hub_point.is_some_and(|hub| {
        (f64::from(point[0]) - hub[0]).hypot(f64::from(point[1]) - hub[1]) <= 220.0
    }) {
        Some(path.with_settings(path.settings().with_seed_step(180.0, 240.0)?))
    } else {
        None
    };
    let path = adjusted.as_ref().unwrap_or(path);
    let [x, y] = point.map(i64::from);
    let (half_width, half_height) = if path.teleport().is_some() {
        (720, 600)
    } else {
        (360, 220)
    };
    let bounds = if options.choose.prefer_frontier {
        [24, 55, 1900, 900]
    } else {
        [
            24.max(x - half_width),
            55.max(y - half_height),
            1900.min(x + half_width),
            900.min(y + half_height),
        ]
    };
    let [l, t, r, b] = bounds.map(usize::try_from);
    let view = path.observe(
        frame.view(),
        PathContext {
            sequence: 1,
            generation: 0,
            captured_at,
            frame_size: frame.size(),
            viewport: PixelRect::new(
                l.map_err(|_| ObservationError::invalid("invalid path viewport"))?,
                t.map_err(|_| ObservationError::invalid("invalid path viewport"))?,
                r.map_err(|_| ObservationError::invalid("invalid path viewport"))?,
                b.map_err(|_| ObservationError::invalid("invalid path viewport"))?,
            )?,
            anchor: point.map(f64::from),
        },
    )?;
    let choice = path.choose(&view, wing, options.choose)?;
    if choice.aim.is_none() && !options.allow_no_aim {
        return Err(ObservationError::invalid(choice.reason));
    }
    if !(0.0..0.25).contains(&(clock.now() - captured_at)) {
        return Err(ObservationError::invalid("planned step frame is stale"));
    }
    Ok(StepPlan {
        choice,
        ground: reading,
        view,
    })
}
/// 경로의 u8 마스크를 배경 관찰기의 bool 마스크로 명시적으로 변환해 소유한다.
pub struct PathMotionMasks {
    context: PathContext,
    cell_pixels: usize,
    floor: Image<bool, 1>,
    excluded: Option<Image<bool, 1>>,
}
impl PathMotionMasks {
    pub fn new(view: &PathView) -> Result<Self, ObservationError> {
        let floor = view.floor().to_owned()?;
        let floor = Image::from_vec(
            floor.size(),
            floor.pixels().iter().map(|v| *v != 0).collect(),
        )?;
        let excluded = view
            .excluded_pixels()
            .map(|mask| -> Result<_, ObservationError> {
                let mask = mask.to_owned()?;
                Ok(Image::from_vec(
                    mask.size(),
                    mask.pixels().iter().map(|v| *v != 0).collect(),
                )?)
            })
            .transpose()?;
        Ok(Self {
            context: view.context,
            cell_pixels: view.cell_pixels,
            floor,
            excluded,
        })
    }
    pub fn view(&self) -> MotionView<'_> {
        MotionView {
            sequence: self.context.sequence,
            captured_at: self.context.captured_at,
            frame_size: self.context.frame_size,
            viewport: self.context.viewport,
            anchor: self.context.anchor,
            cell_pixels: self.cell_pixels,
            floor: &self.floor,
            excluded_pixels: self.excluded.as_ref(),
        }
    }
}
