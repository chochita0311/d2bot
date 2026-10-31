use d2bot::{
    actions::town_loop::{TownSight, TownVision},
    common::{
        frame::{Image, PixelRect, Size},
        vision_ops::{
            codec,
            resize::{self, Interpolation},
        },
    },
};
use std::{error::Error, sync::Arc};
#[test]
fn output_region_keeps_whole_image_weights_edges_and_rounding() -> Result<(), Box<dyn Error>> {
    for (from, to) in [
        ([71, 43], [47, 29]),
        ([73, 47], [37, 23]),
        ([31, 19], [31, 19]),
        ([19, 11], [31, 19]),
    ] {
        let input = Size::new(from[0], from[1])?;
        let frame = Image::<u8, 3>::from_vec(
            input,
            (0..input.byte_len()?)
                .map(|i| ((i * 73 + i / 7) % 256) as u8)
                .collect(),
        )?;
        let target = Size::new(to[0], to[1])?;
        let whole = resize::resize(frame.view(), target, Interpolation::Area)?;
        for roi in [
            PixelRect::full(target),
            PixelRect::new(0, 0, 1, 1)?,
            PixelRect::new(to[0] - 3, to[1] - 2, to[0], to[1])?,
            PixelRect::new(3, 2, 17, 9)?,
        ] {
            assert_eq!(
                resize::area_region(frame.view(), target, roi)?,
                whole.view().crop(roi)?.to_owned()?
            );
        }
        assert!(
            resize::area_region(
                frame.view(),
                target,
                PixelRect::new(0, 0, to[0] + 1, to[1])?
            )
            .is_err()
        );
    }
    Ok(())
}
#[test]
fn panel_roi_matches_existing_full_frame_recognizer() -> Result<(), Box<dyn Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut vision = TownVision::load(root)?;
    for path in [
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/navigation/north-step-before.png",
        "assets/regions/rogue-encampment/remastered-ko-window/references/map-reviewed-window.png",
        "assets/regions/lut-gholein/remastered-ko-window/references/main-reviewed.png",
        "assets/ui/town-loop/remastered-ko-window/references/exit-menu-window.png",
        "assets/ui/town-loop/remastered-ko-window/references/save-exit-window.png",
    ] {
        let original = codec::load_color(root.join(path))?;
        for size in [
            Size::new(1922, 1140)?,
            Size::new(1267, 753)?,
            Size::new(1601, 951)?,
        ] {
            let image = Arc::new(resize::resize(original.view(), size, Interpolation::Area)?);
            vision.set_frame(image.clone());
            vision.normalize()?;
            let expected =
                vision.find("waypoint_header")?.is_some() || vision.find("save_exit")?.is_some();
            vision.clear_frame();
            assert_eq!(vision.panels_open(&image)?, expected, "{path}, {size:?}");
            assert!(vision.normalized().is_none());
        }
    }
    assert!(
        vision
            .panels_open(&Arc::new(Image::<u8, 3>::zeros(Size::new(300, 200)?)?))
            .is_err()
    );
    Ok(())
}
