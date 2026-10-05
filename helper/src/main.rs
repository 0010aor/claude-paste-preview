#[cfg(target_os = "linux")]
mod clipboard;
mod preview;
#[cfg(target_os = "linux")]
mod x11;
#[cfg(target_os = "linux")]
mod xclip;

use std::process::ExitCode;

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

const USAGE: &str = "usage: claude-paste-helper preview IMAGE.png...
       claude-paste-helper xclip [-selection clipboard|primary] [-t TARGET] [-o | -i] [FILE...]   (Linux)
       claude-paste-helper --version
On Linux, invoked as `xclip`, it takes xclip's arguments directly.";

fn run(program: &str, args: &[String]) -> Result<()> {
    #[cfg(target_os = "linux")]
    if program == "xclip" {
        return xclip::run(args);
    }
    let _ = program;
    match args {
        [command, paths @ ..] if command == "preview" && !paths.is_empty() => preview::show(paths),
        #[cfg(target_os = "linux")]
        [command, rest @ ..] if command == "xclip" => xclip::run(rest),
        #[cfg(target_os = "linux")]
        [command, selection, target @ ..] if command == xclip::SERVE_COMMAND => {
            xclip::serve(selection, target.first().map(String::as_str))
        }
        [flag] if flag == "--version" => {
            println!("claude-paste-helper {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args();
    let invoked_as = args.next().unwrap_or_default();
    let program = std::path::Path::new(&invoked_as)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_owned();
    match run(&program, &args.collect::<Vec<_>>()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{program}: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_commands_with_the_usage() {
        let args = |line: &str| {
            line.split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert!(run("claude-paste-helper", &args("preview"))
            .unwrap_err()
            .to_string()
            .starts_with("usage:"));
        assert!(run("claude-paste-helper", &args("frobnicate")).is_err());
    }
}
