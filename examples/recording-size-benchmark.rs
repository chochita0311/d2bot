//! 저장된 화면만 사용한 코덱 크기 비교. 게임 캡처·입력은 수행하지 않는다.
use d2bot::common::{
    frame::BgrFrame,
    recording::{Mpeg4Writer, OutputMode},
    vision_ops::codec,
};
#[path = "support/mjpeg.rs"]
mod mjpeg;
use mjpeg::MjpegWriter;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    error::Error,
    path::PathBuf,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args_os().skip(1);
    let directory = arguments
        .next()
        .map(PathBuf::from)
        .ok_or("output directory argument required")?;
    let codec = arguments.next().unwrap_or_else(|| "mpeg4".into());
    let mpeg4 = match codec.to_str() {
        Some("mpeg4") => true,
        Some("mjpeg") => false,
        _ => return Err("codec must be mpeg4 or mjpeg".into()),
    };
    std::fs::create_dir(&directory)?;
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let sources = [
        "assets/regions/arcane-sanctuary/remastered-ko-1922x1140/references/hub/main.png",
        "assets/ui/town-loop/remastered-ko-window/references/npc-dialogue-window.png",
        "assets/ui/town-loop/remastered-ko-window/references/exit-menu-window.png",
    ];
    let images = sources
        .iter()
        .map(|p| codec::load_color(root.join(p)))
        .collect::<Result<Vec<_>, _>>()?;
    let size = images[0].size();
    if images.iter().any(|frame| frame.size() != size) {
        return Err("comparison source dimensions differ".into());
    }
    let count = 64usize;
    let mut rows = vec![];
    for pattern in ["static", "pan", "cuts"] {
        let qualities: &[u8] = if mpeg4 { &[3, 4, 6] } else { &[70, 80, 90, 95] };
        for &quality in qualities {
            let path = directory.join(format!("rust-{pattern}-q{quality}.avi"));
            let started = Instant::now();
            let mut writer = if mpeg4 {
                Writer::Mpeg4(Box::new(Mpeg4Writer::open(
                    &path,
                    size,
                    quality,
                    OutputMode::CreateNew,
                )?))
            } else {
                Writer::Mjpeg(MjpegWriter::open(
                    &path,
                    size,
                    quality,
                    OutputMode::CreateNew,
                )?)
            };
            let mut io_time = started.elapsed();
            let mut times = vec![];
            let mut hash = Sha256::new();
            for index in 0..count {
                let source = if pattern == "cuts" {
                    &images[(index / 8) % images.len()]
                } else {
                    &images[0]
                };
                let frame = if pattern == "pan" {
                    let shift = index * 3 % size.width();
                    let mut bytes = Vec::with_capacity(source.bytes().len());
                    for row in source.bytes().chunks_exact(size.row_bytes()) {
                        let split = size.row_bytes() - shift * 3;
                        bytes.extend_from_slice(&row[split..]);
                        bytes.extend_from_slice(&row[..split]);
                    }
                    BgrFrame::from_vec(size, bytes)?
                } else {
                    source.clone()
                };
                hash.update(frame.bytes());
                let started = Instant::now();
                writer.write(&frame)?;
                let elapsed = started.elapsed();
                io_time += elapsed;
                times.push(elapsed);
            }
            let started = Instant::now();
            writer.finish()?;
            let finish = started.elapsed();
            io_time += finish;
            drop(writer);
            times.sort();
            let ms = |duration: Duration| duration.as_secs_f64() * 1000.0;
            rows.push(json!({"path":path,"pattern":pattern,"codec":if mpeg4 {"Rust MPEG-4"} else {"MJPG"},"quality":quality,"frames":count,"width":size.width(),"height":size.height(),"bytes":std::fs::metadata(&path)?.len(),"encode_io_ms":ms(io_time),"write_p50_ms":ms(times[count/2]),"write_p95_ms":ms(times[(count*95/100).min(count-1)]),"finish_ms":ms(finish),"input_sha256":format!("{:x}",hash.finalize())}));
        }
    }
    let result = json!({"source_paths":sources,"frame_count":count,"fps":8,"sampling":if mpeg4 {"4:2:0"} else {"4:2:2"},"clips":rows});
    std::fs::write(
        directory.join("rust-manifest.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}
enum Writer {
    Mjpeg(MjpegWriter),
    Mpeg4(Box<Mpeg4Writer>),
}
impl Writer {
    fn write(&mut self, frame: &BgrFrame) -> std::io::Result<()> {
        match self {
            Self::Mjpeg(writer) => writer.write(frame),
            Self::Mpeg4(writer) => writer.write(frame),
        }
    }
    fn finish(&mut self) -> std::io::Result<()> {
        match self {
            Self::Mjpeg(writer) => writer.finish(),
            Self::Mpeg4(writer) => writer.finish(),
        }
    }
}
