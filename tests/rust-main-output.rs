use d2bot::{
    common::{
        frame::{BgrFrame, Size},
        vision_ops::codec,
    },
    runs::summoner::{
        encounter_execution::EncounterEvidence,
        main_run::{RunFiles, RunOutput},
    },
};
use serde_json::json;
use std::{error::Error, sync::Arc};
#[path = "support/directory.rs"]
mod directory;
#[test]
fn reports_replace_atomically_with_exclusive_run_and_owned_image_paths()
-> Result<(), Box<dyn Error>> {
    let fixture = directory::TestDirectory::new()?;
    let output = fixture.path().join("run");
    let mut files = RunFiles::create(&output)?;
    let first = json!({"status":"preparing","evidence_write_status":"pending"});
    files.save_report(&first)?;
    let frame = Arc::new(BgrFrame::from_vec(
        Size::new(2, 2)?,
        vec![0, 0, 255, 0, 255, 0, 255, 0, 0, 50, 100, 150],
    )?);
    files.save_frame(&EncounterEvidence {
        filename: "proof.png".into(),
        frame: frame.clone(),
    })?;
    assert_eq!(
        codec::load_color(output.join("proof.png"))?.pixels(),
        frame.pixels()
    );
    let complete =
        json!({"status":"north_room_closed","errors":[],"evidence_write_status":"complete"});
    files.save_report(&complete)?;
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(files.report_path())?)?,
        complete
    );
    assert!(!output.join("report.json.pending").exists());
    assert!(RunFiles::create(&output).is_err());
    for name in ["../escape.png", "nested/proof.png", "D:/escape.png"] {
        assert!(
            files
                .save_frame(&EncounterEvidence {
                    filename: name.into(),
                    frame: frame.clone()
                })
                .is_err()
        );
    }
    assert!(!fixture.path().join("escape.png").exists());
    let pending = output.join("report.json.pending");
    std::fs::create_dir(&pending)?;
    assert!(files.save_report(&first).is_err());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&std::fs::read(files.report_path())?)?,
        complete
    );
    std::fs::remove_dir(pending)?;
    Ok(())
}
