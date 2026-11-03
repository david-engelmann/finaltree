//! The finaltree command line.
//!
//! The binary carries the fingerprint, stamp, and verify doors
//! end to end with the Python command's outputs and exit codes.
//! Its argument surface is its own, close in shape to the
//! Python command's so a gate script can drive either.
//!
//! The arguments are read as the operating system hands them
//! over, in their bytes, so a tree whose root is spelled with
//! bytes that are not valid UTF-8 is fingerprinted as the
//! Python command fingerprints it, and the argument reading
//! of the standard library never stops the binary on them.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use finaltree::errors::FinaltreeError;
use finaltree::fingerprint::compute_fingerprint;
use finaltree::render::render_printed;
use finaltree::stamping::{run_and_stamp, verify_stamped_log};

struct DoorArgs {
    positionals: Vec<OsString>,
    generated: Vec<String>,
    generated_seen: bool,
    log: Option<OsString>,
}

fn parse_door_args(args: &[OsString], take_log: bool) -> Result<DoorArgs, String> {
    let mut parsed = DoorArgs {
        positionals: Vec::new(),
        generated: Vec::new(),
        generated_seen: false,
        log: None,
    };
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        if arg == "--generated" {
            let value = args
                .get(index + 1)
                .ok_or("the --generated option names no pattern")?;
            parsed.generated.push(value.to_string_lossy().into_owned());
            parsed.generated_seen = true;
            index += 2;
        } else if arg == "--log" && take_log {
            let value = args
                .get(index + 1)
                .ok_or("the --log option names no path")?;
            parsed.log = Some(value.clone());
            index += 2;
        } else if arg.to_str().is_some_and(|text| text.starts_with("--")) {
            return Err(format!(
                "the argument {} is not understood",
                arg.to_string_lossy()
            ));
        } else {
            parsed.positionals.push(arg.clone());
            index += 1;
        }
    }
    Ok(parsed)
}

fn run() -> Result<i32, FinaltreeError> {
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    let Some(door) = argv.first().cloned() else {
        eprintln!("error. the command names no door");
        return Ok(2);
    };
    let door_text = door.to_string_lossy().into_owned();
    // The stamp door takes its gate command after a double
    // dash, as the Python command does, with trailing arguments
    // before the dash standing as the command when none follow.
    let mut tail: Vec<OsString> = Vec::new();
    let mut head: Vec<OsString> = argv.clone();
    if door_text == "stamp" {
        if let Some(cut) = argv.iter().position(|arg| arg == "--") {
            tail = argv[cut + 1..].to_vec();
            head = argv[..cut].to_vec();
        }
    }
    let args = &head[1..];
    match door_text.as_str() {
        "fingerprint" => {
            let parsed = parse_door_args(args, false).map_err(FinaltreeError::new)?;
            let Some(root) = parsed.positionals.first() else {
                eprintln!("error. the fingerprint command names no tree");
                return Ok(2);
            };
            let fingerprint = compute_fingerprint(Path::new(root), &parsed.generated)?;
            println!("{}", render_printed(&fingerprint.to_dict()));
            Ok(0)
        }
        "stamp" => {
            let parsed = parse_door_args(args, true).map_err(FinaltreeError::new)?;
            let Some(root) = parsed.positionals.first() else {
                eprintln!("error. the stamp command names no tree");
                return Ok(2);
            };
            let Some(log) = parsed.log else {
                eprintln!("error. the stamp command names no log");
                return Ok(2);
            };
            let fallback: Vec<OsString> = parsed.positionals.iter().skip(1).cloned().collect();
            let command_os = if !tail.is_empty() { tail } else { fallback };
            if command_os.is_empty() {
                eprintln!("error. the stamp command names no gate command");
                return Ok(2);
            }
            let command: Vec<String> = command_os
                .iter()
                .map(|part| part.to_string_lossy().into_owned())
                .collect();
            run_and_stamp(
                Path::new(root),
                &command,
                &PathBuf::from(log),
                &parsed.generated,
            )
        }
        "verify" => {
            let parsed = parse_door_args(args, false).map_err(FinaltreeError::new)?;
            let (Some(log), Some(root)) = (parsed.positionals.first(), parsed.positionals.get(1))
            else {
                eprintln!("error. the verify command names a log and a tree");
                return Ok(2);
            };
            let patterns = if parsed.generated_seen {
                Some(parsed.generated.as_slice())
            } else {
                None
            };
            let result = verify_stamped_log(Path::new(log), Path::new(root), patterns)?;
            if result.matched {
                println!("match");
                Ok(0)
            } else {
                println!("mismatch");
                for difference in &result.differences {
                    println!("{difference}");
                }
                Ok(1)
            }
        }
        _ => {
            eprintln!("error. the door {door_text} is not a finaltree door");
            Ok(2)
        }
    }
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(2)),
        Err(error) => {
            eprintln!("error. {error}");
            ExitCode::from(2)
        }
    }
}
