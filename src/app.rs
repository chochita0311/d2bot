//! 단일 실행파일의 명령 선택과 종료 코드. 도메인 실행은 소유 모듈에 맡긴다.
pub mod arguments;
mod execution;
mod routes;
use crate::{
    common::cli::UsageError,
    runs::RunError,
    tools::{self, ToolError},
};
use std::{
    ffi::OsString,
    fmt,
    io::{self, Write},
    path::{Path, PathBuf},
};
/// 배포 표시 파일이 있으면 EXE 옆 자산을 사용하고 개발 실행은 호출 디렉터리를 유지한다.
pub(crate) fn runtime_root() -> io::Result<PathBuf> {
    let executable = std::env::current_exe()?;
    Ok(runtime_root_at(&executable, &std::env::current_dir()?))
}
fn runtime_root_at(executable: &Path, caller: &Path) -> PathBuf {
    executable
        .parent()
        .filter(|p| p.join("package-manifest.json").is_file())
        .unwrap_or(caller)
        .to_owned()
}
pub const HELP: &str = include_str!("app/help.txt");
#[derive(Debug)]
pub enum AppError {
    Usage(String),
    Io(io::Error),
    Runtime(RunError),
}
impl AppError {
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            _ => 1,
        }
    }
}
impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(e) => write!(f, "d2bot: {e}\nUse --help for usage."),
            Self::Io(e) => write!(f, "d2bot: {e}"),
            Self::Runtime(e) => write!(f, "d2bot: {e}"),
        }
    }
}
impl std::error::Error for AppError {}
impl From<io::Error> for AppError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<RunError> for AppError {
    fn from(e: RunError) -> Self {
        Self::Runtime(e)
    }
}
impl From<UsageError> for AppError {
    fn from(e: UsageError) -> Self {
        Self::Usage(e.to_string())
    }
}
impl From<ToolError> for AppError {
    fn from(e: ToolError) -> Self {
        match e {
            ToolError::Usage(e) => e.into(),
            ToolError::Runtime(e) => e.into(),
        }
    }
}
pub fn run(
    arguments: impl IntoIterator<Item = OsString>,
    output: &mut (impl Write + Send),
) -> Result<u8, AppError> {
    let args: Vec<_> = arguments.into_iter().collect();
    if let Some(command) = args.first().and_then(|a| a.to_str()) {
        let input = args.iter().skip(1).cloned();
        let tool = match command {
            "survival-observe" => Some(tools::survival_observe::run(input, output)),
            "field-replay" => Some(tools::field_replay::run(input, output)),
            "arcane-navigation-replay" => Some(tools::arcane_navigation_replay::run(input, output)),
            "field-input-probe" => Some(tools::field_input_probe::run(input, output)),
            "town-loop-vision" => Some(tools::town_loop_vision::run(input, output)),
            _ => None,
        };
        if let Some(result) = tool {
            return Ok(result?);
        }
    }
    match arguments::parse(args)? {
        arguments::Command::Help => output.write_all(HELP.as_bytes())?,
        arguments::Command::Version => writeln!(output, "d2bot {}", env!("CARGO_PKG_VERSION"))?,
        arguments::Command::Diagnose => {
            let root = runtime_root()?;
            writeln!(
                output,
                "d2bot {}\nOS: {}\nArchitecture: {}\nRuntime directory: {}\nConfig directory: {}\nAssets directory: {}\nRuntime: native Rust GUI, CLI, screen vision, input and MPEG-4 AVI recording\nPython/OpenCV runtime required: false",
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
                root.display(),
                root.join("config").is_dir(),
                root.join("assets").is_dir()
            )?;
        }
        arguments::Command::Main(options) => return Ok(routes::run(*options, output)?),
    }
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn help_tools_version_and_errors_initialize_no_game_effects() -> Result<(), AppError> {
        for command in [
            None,
            Some("survival-observe"),
            Some("field-replay"),
            Some("arcane-navigation-replay"),
            Some("field-input-probe"),
            Some("town-loop-vision"),
        ] {
            let mut args = Vec::new();
            if let Some(c) = command {
                args.push(OsString::from(c));
            }
            args.push("--help".into());
            let mut out = Vec::new();
            assert_eq!(run(args, &mut out)?, 0);
            assert!(!out.is_empty());
        }
        let mut out = Vec::new();
        assert_eq!(run(["--version".into()], &mut out)?, 0);
        assert_eq!(
            out,
            format!("d2bot {}\n", env!("CARGO_PKG_VERSION")).as_bytes()
        );
        assert!(matches!(
            run(["--missing".into()], &mut out),
            Err(AppError::Usage(_))
        ));
        assert!(matches!(
            run(["--config=missing".into()], &mut out),
            Err(AppError::Runtime(_))
        ));
        Ok(())
    }
}
