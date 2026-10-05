use std::io::{Read, Write};
use std::process::{Command, Stdio};

use crate::clipboard;
use crate::Result;

pub const SERVE_COMMAND: &str = "__serve";

#[derive(Debug, Default, PartialEq)]
struct XclipArgs {
    selection: Option<String>,
    target: Option<String>,
    is_output: bool,
    files: Vec<String>,
}

fn parse_xclip_args(args: &[String]) -> Result<XclipArgs> {
    let mut parsed = XclipArgs::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-selection" | "-sel" | "-se" => parsed.selection = args.next().cloned(),
            "-t" | "-target" => parsed.target = args.next().cloned(),
            "-o" | "-out" => parsed.is_output = true,
            "-i" | "-in" => parsed.is_output = false,
            "-quiet" | "-silent" | "-verbose" | "-noutf8" | "-rmlastnl" | "-r" | "-f"
            | "-filter" => {}
            "-l" | "-loops" | "-d" | "-display" => {
                args.next();
            }
            option if option.starts_with('-') => {
                return Err(format!("unsupported option {option}").into())
            }
            file => parsed.files.push(file.to_owned()),
        }
    }
    Ok(parsed)
}

pub fn read_stdin() -> Result<Vec<u8>> {
    let mut data = Vec::new();
    std::io::stdin().read_to_end(&mut data)?;
    Ok(data)
}

fn read_input(files: &[String]) -> Result<Vec<u8>> {
    if files.is_empty() {
        return read_stdin();
    }
    let mut data = Vec::new();
    for file in files {
        data.extend(std::fs::read(file)?);
    }
    Ok(data)
}

fn serve_in_background(selection: &str, target: Option<&str>, data: &[u8]) -> Result<()> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg(SERVE_COMMAND).arg(selection);
    if let Some(target) = target {
        command.arg(target);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    child
        .stdin
        .take()
        .ok_or("no stdin for the clipboard server")?
        .write_all(data)?;
    Ok(())
}

pub fn run(args: &[String]) -> Result<()> {
    let args = parse_xclip_args(args)?;
    let selection = args.selection.as_deref().unwrap_or("primary");
    if args.is_output {
        let target = args.target.as_deref().unwrap_or("UTF8_STRING");
        std::io::stdout().write_all(&clipboard::read(selection, target)?)?;
        return Ok(());
    }
    serve_in_background(selection, args.target.as_deref(), &read_input(&args.files)?)
}

pub fn serve(selection: &str, target: Option<&str>) -> Result<()> {
    clipboard::serve(selection, target, read_stdin()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn parses_the_commands_claude_code_runs() {
        assert_eq!(
            parse_xclip_args(&args("-selection clipboard -t TARGETS -o")).unwrap(),
            XclipArgs {
                selection: Some("clipboard".into()),
                target: Some("TARGETS".into()),
                is_output: true,
                files: vec![]
            }
        );
        assert_eq!(
            parse_xclip_args(&args("-selection clipboard -t image/png -o"))
                .unwrap()
                .target
                .as_deref(),
            Some("image/png")
        );
        assert!(
            !parse_xclip_args(&args("-selection clipboard"))
                .unwrap()
                .is_output
        );
    }

    #[test]
    fn accepts_xclip_flags_it_can_ignore_and_input_files() {
        let parsed = parse_xclip_args(&args("-sel p -quiet -loops 2 -i a.txt b.txt")).unwrap();
        assert_eq!(parsed.selection.as_deref(), Some("p"));
        assert_eq!(parsed.files, vec!["a.txt", "b.txt"]);
    }

    #[test]
    fn refuses_options_it_does_not_implement() {
        assert!(parse_xclip_args(&args("-selection clipboard -version")).is_err());
    }
}
