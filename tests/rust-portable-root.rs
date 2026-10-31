#[path = "support/directory.rs"]
mod directory;
use std::{error::Error, process::Command};
#[test]
fn portable_executable_uses_siblings_and_preserves_explicit_caller_config()
-> Result<(), Box<dyn Error>> {
    let directory = directory::TestDirectory::new()?;
    let package = directory.path().join("portable");
    let caller = directory.path().join("caller");
    std::fs::create_dir_all(package.join("assets"))?;
    std::fs::create_dir_all(package.join("config"))?;
    std::fs::create_dir(&caller)?;
    let executable = package.join("d2bot.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_d2bot"), &executable)?;
    std::fs::write(package.join("package-manifest.json"), b"{}")?;
    // caller의 잘못된 기본 설정이 배포 설정을 가리는 상황을 방지한다.
    std::fs::write(caller.join("config"), b"invalid caller config")?;
    let diagnostic = Command::new(&executable)
        .current_dir(&caller)
        .arg("--diagnose")
        .output()?;
    assert!(diagnostic.status.success());
    let output = String::from_utf8(diagnostic.stdout)?;
    let reported = output
        .lines()
        .find_map(|line| line.strip_prefix("Runtime directory: "))
        .ok_or("runtime directory missing")?;
    assert_eq!(
        std::path::Path::new(reported).canonicalize()?,
        package.canonicalize()?
    );
    assert!(output.contains("Assets directory: true"));
    let dry = Command::new(&executable)
        .current_dir(&caller)
        .args(["--summoner-run", "--north-cycles=1"])
        .output()?;
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stderr)
    );
    assert!(String::from_utf8_lossy(&dry.stdout).starts_with("DRY:"));
    let explicit = Command::new(&executable)
        .current_dir(&caller)
        .args(["--summoner-run", "--north-cycles=1", "--config=config"])
        .output()?;
    assert!(!explicit.status.success());
    // 필수 설정이 사라져도 caller/원본 프로젝트로 되돌아가서 성공하지 않는다.
    std::fs::remove_dir(package.join("config"))?;
    let missing = Command::new(&executable)
        .current_dir(&caller)
        .args(["--summoner-run", "--north-cycles=1"])
        .output()?;
    assert!(!missing.status.success());
    Ok(())
}
