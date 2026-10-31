//! 검토한 지역의 현재 외형 후보와 팔레트 관찰. 미검출을 안전·개체 수·생사로 바꾸지 않는다.
use super::{
    color_palette::Palette,
    encounters::{Appearance, CandidateCategory, CandidateIdentity, SceneContext, VisualCandidate},
    error::ObservationError,
    frame::{BgrFrame, BgrView, FloatMap, PixelRect, Size},
    observation_assets::AssetOwner,
    survival::ObservationStamp,
    time,
    vision_ops::{
        codec,
        template::{self, BgrMatcher, MatchMethod},
    },
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path, sync::Arc};
#[derive(Deserialize)]
struct Manifest {
    schema: u32,
    region_id: String,
    frame_size: [usize; 2],
    terrain: Terrain,
    templates: Vec<Rule>,
}
#[derive(Deserialize)]
struct Terrain {
    floor_bgr: Vec<[u8; 3]>,
    void_bgr: Vec<[u8; 3]>,
    floor_lab_distance: f64,
    void_lab_distance: f64,
}
#[derive(Deserialize)]
struct Rule {
    id: String,
    kind: String,
    category: CandidateCategory,
    appearance: Appearance,
    path: String,
    threshold: f64,
    ground_offset: Option<[f64; 2]>,
}
struct Template {
    identity: CandidateIdentity,
    image: BgrFrame,
    threshold: f64,
    ground_offset: Option<[f64; 2]>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TerrainEvidence {
    pub context: SceneContext,
    #[serde(rename = "box")]
    pub bounds: [usize; 4],
    pub floor_ratio: f64,
    pub void_ratio: f64,
    pub state: &'static str,
}
pub struct RegionObserver {
    _compute: super::compute::ComputeLease,
    region_id: String,
    size: Size,
    templates: Vec<Template>,
    floor: Palette,
    void: Palette,
    floor_distance: f64,
    void_distance: f64,
}
#[derive(Debug, Clone, Copy)]
pub struct ScanOptions {
    pub now: f64,
    pub max_age: f64,
    pub limit_per_template: usize,
    pub workers: usize,
}
#[derive(Debug, Clone, Copy)]
pub struct TerrainOptions {
    pub now: f64,
    pub max_age: f64,
    pub minimum_floor: f64,
    pub maximum_void: f64,
}
impl RegionObserver {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ObservationError> {
        let path = path.as_ref().canonicalize()?;
        let directory = path
            .parent()
            .ok_or_else(|| ObservationError::invalid("region manifest owner missing"))?;
        let raw: Manifest = serde_json::from_slice(&std::fs::read(&path)?)?;
        if raw.schema != 1 {
            return Err(ObservationError::invalid(
                "unsupported region manifest schema",
            ));
        }
        let size = Size::new(raw.frame_size[0], raw.frame_size[1])?;
        time::nonnegative(raw.terrain.floor_lab_distance, "floor_lab_distance")?;
        time::nonnegative(raw.terrain.void_lab_distance, "void_lab_distance")?;
        let owner = AssetOwner::new(directory)?;
        let mut names = HashSet::new();
        let mut templates = Vec::new();
        for rule in raw.templates {
            let path = owner.path(&rule.path)?;
            if !path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("png"))
            {
                return Err(ObservationError::invalid(
                    "templates must be owned PNG files",
                ));
            }
            let bytes = std::fs::read(&path)?;
            if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                return Err(ObservationError::invalid("template must have PNG bytes"));
            }
            let image = codec::decode_color(&bytes)?;
            let first = &image.pixels()[..3];
            if image
                .pixels()
                .as_chunks::<3>()
                .0
                .iter()
                .all(|p| p.as_slice() == first)
            {
                return Err(ObservationError::invalid(
                    "template must contain non-uniform BGR pixels",
                ));
            }
            if !names.insert(rule.id.clone()) {
                return Err(ObservationError::invalid("template IDs must be unique"));
            }
            time::nonnegative(rule.threshold, "threshold")?;
            if rule.threshold <= 0.0 || rule.threshold > 1.0 {
                return Err(ObservationError::invalid("positive threshold required"));
            }
            let identity =
                CandidateIdentity::new(rule.id, rule.kind, rule.category, rule.appearance)?;
            let context = Arc::new(SceneContext::new(
                ObservationStamp::new("validation", "validation", 0, 0.0)?,
                raw.region_id.clone(),
                0,
                size,
                PixelRect::full(size),
            )?);
            VisualCandidate::new(
                context,
                identity.clone(),
                1.0,
                PixelRect::full(image.size()),
                rule.ground_offset,
            )?;
            templates.push(Template {
                identity,
                image,
                threshold: rule.threshold,
                ground_offset: rule.ground_offset,
            });
        }
        Ok(Self {
            region_id: raw.region_id,
            size,
            templates,
            floor: Palette::new(raw.terrain.floor_bgr)?,
            void: Palette::new(raw.terrain.void_bgr)?,
            floor_distance: raw.terrain.floor_lab_distance,
            void_distance: raw.terrain.void_lab_distance,
            _compute: super::compute::ComputeLease::acquire()?,
        })
    }
    pub fn region_id(&self) -> &str {
        &self.region_id
    }
    pub fn frame_size(&self) -> Size {
        self.size
    }
    pub(crate) fn retain_alive_monsters(&mut self) {
        self.templates.retain(|t| {
            t.identity.category() == CandidateCategory::Monster
                && t.identity.appearance() == Appearance::Alive
        });
    }
    fn validate(&self, frame: BgrView<'_>, context: &SceneContext) -> Result<(), ObservationError> {
        if frame.size().width() != context.viewport().width()
            || frame.size().height() != context.viewport().height()
        {
            return Err(ObservationError::invalid(
                "image must be native BGR pixels for the declared viewport",
            ));
        }
        if context.region_id() != self.region_id || context.frame_size() != self.size {
            return Err(ObservationError::invalid(
                "unreviewed region or capture size",
            ));
        }
        Ok(())
    }
    fn match_image(
        matcher: &BgrMatcher<'_>,
        template: &Template,
    ) -> Result<Option<FloatMap>, ObservationError> {
        if matcher.size().width() < template.image.size().width()
            || matcher.size().height() < template.image.size().height()
        {
            return Ok(None);
        }
        Ok(Some(matcher.match_pattern(
            template.image.view(),
            MatchMethod::NormalizedCoefficient,
        )?))
    }
    fn candidates(
        &self,
        index: usize,
        scores: Option<FloatMap>,
        context: &Arc<SceneContext>,
        limit: usize,
        output: &mut Vec<VisualCandidate>,
    ) -> Result<(), ObservationError> {
        let Some(mut scores) = scores else {
            return Ok(());
        };
        let template = &self.templates[index];
        let width = template.image.size().width();
        let height = template.image.size().height();
        let viewport = context.viewport();
        for _ in 0..limit {
            let Some(peak) = template::maximum(&scores) else {
                break;
            };
            if !peak.score.is_finite() || f64::from(peak.score) < template.threshold {
                break;
            }
            let [x, y] = peak.location;
            let left = viewport.left() + x;
            let top = viewport.top() + y;
            let ground = template
                .ground_offset
                .map(|[gx, gy]| [left as f64 + gx, top as f64 + gy]);
            output.push(VisualCandidate::new(
                Arc::clone(context),
                template.identity.clone(),
                f64::from(peak.score).min(1.0),
                PixelRect::new(left, top, left + width, top + height)?,
                ground,
            )?);
            let w = scores.size().width();
            for row in y.saturating_sub(height - 1)..(y + height).min(scores.size().height()) {
                scores.pixels_mut()
                    [row * w + x.saturating_sub(width - 1)..row * w + (x + width).min(w)]
                    .fill(-1.0);
            }
        }
        Ok(())
    }
    pub fn scan(
        &self,
        frame: BgrView<'_>,
        context: Arc<SceneContext>,
        options: ScanOptions,
    ) -> Result<Vec<VisualCandidate>, ObservationError> {
        self.validate(frame, &context)?;
        if !(1..=16).contains(&options.limit_per_template) || !(1..=4).contains(&options.workers) {
            return Err(ObservationError::invalid(
                "bounded region scan limit and workers required",
            ));
        }
        if !context.fresh(options.now, options.max_age)? {
            return Ok(Vec::new());
        }
        if !self.templates.iter().any(|template| {
            template.image.size().width() <= frame.size().width()
                && template.image.size().height() <= frame.size().height()
        }) {
            return Ok(Vec::new());
        }
        let matcher = BgrMatcher::new(frame)?;
        let mut found = Vec::new();
        if options.workers == 1 {
            for (index, template) in self.templates.iter().enumerate() {
                self.candidates(
                    index,
                    Self::match_image(&matcher, template)?,
                    &context,
                    options.limit_per_template,
                    &mut found,
                )?;
            }
        } else {
            let mut groups: Vec<_> = (0..options.workers.min(self.templates.len()))
                .map(|_| None)
                .collect();
            super::compute::scope(|scope| {
                for (worker, result) in groups.iter_mut().enumerate() {
                    let matcher = &matcher;
                    scope.spawn(move |_| {
                        *result = Some(
                            (worker..self.templates.len())
                                .step_by(options.workers)
                                .map(|index| {
                                    (index, Self::match_image(matcher, &self.templates[index]))
                                })
                                .collect::<Vec<_>>(),
                        );
                    });
                }
            })
            .map_err(|error| match error {
                super::compute::ComputeError::Start(error) => ObservationError::Io(error),
                super::compute::ComputeError::Panicked => {
                    ObservationError::invalid("region matcher worker panicked")
                }
            })?;
            let mut results = Vec::new();
            for group in groups {
                results.extend(
                    group.ok_or_else(|| {
                        ObservationError::invalid("region matcher result missing")
                    })?,
                );
            }
            results.sort_by_key(|(index, _)| *index);
            for (index, scores) in results {
                self.candidates(
                    index,
                    scores?,
                    &context,
                    options.limit_per_template,
                    &mut found,
                )?;
            }
        }
        Ok(found)
    }
    pub fn terrain(
        &self,
        frame: BgrView<'_>,
        context: &SceneContext,
        bounds: PixelRect,
        options: TerrainOptions,
    ) -> Result<TerrainEvidence, ObservationError> {
        self.validate(frame, context)?;
        time::nonnegative(options.minimum_floor, "minimum_floor")?;
        time::nonnegative(options.maximum_void, "maximum_void")?;
        if options.minimum_floor <= 0.0
            || options.minimum_floor > 1.0
            || options.maximum_void >= 1.0
        {
            return Err(ObservationError::invalid(
                "terrain gates must reject unobserved/void-only patches",
            ));
        }
        let viewport = context.viewport();
        if bounds.left() < viewport.left()
            || bounds.top() < viewport.top()
            || bounds.right() > viewport.right()
            || bounds.bottom() > viewport.bottom()
        {
            return Err(ObservationError::invalid(
                "terrain patch must be inside the viewport",
            ));
        }
        let rect = [bounds.left(), bounds.top(), bounds.right(), bounds.bottom()];
        let result = |floor_ratio, void_ratio, state| TerrainEvidence {
            context: context.clone(),
            bounds: rect,
            floor_ratio,
            void_ratio,
            state,
        };
        if !context.fresh(options.now, options.max_age)? {
            return Ok(result(0.0, 0.0, "unknown"));
        }
        let patch = frame.crop(PixelRect::new(
            bounds.left() - viewport.left(),
            bounds.top() - viewport.top(),
            bounds.right() - viewport.left(),
            bounds.bottom() - viewport.top(),
        )?)?;
        let floor = self.floor.distance_map(patch)?;
        let void = self.void.distance_map(patch)?;
        let (mut floors, mut voids) = (0_usize, 0_usize);
        for (f, v) in floor.pixels().iter().zip(void.pixels()) {
            let f = *f <= self.floor_distance as f32;
            let v = *v <= self.void_distance as f32;
            floors += usize::from(f && !v);
            voids += usize::from(v && !f);
        }
        let count = floor.pixels().len() as f64;
        let floor_ratio = floors as f64 / count;
        let void_ratio = voids as f64 / count;
        let state = if floor_ratio >= options.minimum_floor && void_ratio <= options.maximum_void {
            "floor-candidate"
        } else {
            "reject-or-unknown"
        };
        Ok(result(floor_ratio, void_ratio, state))
    }
}
