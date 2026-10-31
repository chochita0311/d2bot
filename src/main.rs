use std::io::{self, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    match d2bot::app::run(std::env::args_os().skip(1), &mut io::stdout()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let _ = writeln!(io::stderr().lock(), "{error}");
            ExitCode::from(error.exit_code())
        }
    }
}
