//! 게임 캡처/입력 없이 preview의 표시·Escape·비동기 닫기를 관찰하는 개발 도구.
use d2bot::{
    common::{
        detectors::{TemplateMatch, draw_overlay},
        frame::{BgrFrame, Size},
        workers::Cancellation,
    },
    core::PreviewFrames,
    ui::preview,
};
use std::{
    error::Error,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
struct Finished(Arc<AtomicBool>);
impl Drop for Finished {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}
fn main() -> Result<(), Box<dyn Error>> {
    let size = Size::new(960, 540)?;
    let mut frame = BgrFrame::zeros(size)?;
    for (i, p) in frame
        .pixels_mut()
        .as_chunks_mut::<3>()
        .0
        .iter_mut()
        .enumerate()
    {
        *p = [(i % 960 / 4) as u8, (i / 960 / 3) as u8, 45];
    }
    let matches = [TemplateMatch {
        name: "stored reference".into(),
        confidence: 0.985,
        top_left: [130, 150],
        bottom_right: [430, 350],
        action: "log".into(),
    }];
    let frame = draw_overlay(&frame, &matches, "Preview | watching | DRY-RUN | RUNNING")?;
    let frames = PreviewFrames::default();
    let stop = Cancellation::default();
    let finished = Arc::new(AtomicBool::new(false));
    let channel = frames.clone();
    let token = stop.clone();
    let done = Finished(finished.clone());
    let worker = std::thread::spawn(move || -> Result<(), d2bot::runs::RunError> {
        let _done = done;
        while !token.is_cancelled() {
            channel.publish(frame.clone())?;
            token.wait(Duration::from_millis(50))?;
        }
        std::thread::sleep(Duration::from_secs(3));
        Ok(())
    });
    let display = preview::run(frames, stop.clone(), finished);
    stop.cancel();
    worker.join().map_err(|_| "preview worker panicked")??;
    display?;
    Ok(())
}
