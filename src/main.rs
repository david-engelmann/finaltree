//! The finaltree command line.
//!
//! The binary carries the three doors of the Python command
//! line whole, fingerprint and stamp and verify, over the
//! ported library. Its argument reading follows the model the
//! contract probe recorded for the Python argument reader, the
//! subcommand shape and the flag forms and the exit code
//! discipline standing as the parser stands, with the texts
//! the Python reader writes standing outside parity as the
//! contract states, the binary writing its own short usage
//! line per door. A usage refusal exits 2 with that line on
//! standard error and nothing read and nothing written, and
//! the help flag on any door and at the top level prints the
//! same line to standard output and exits 0.
//!
//! The arguments are read as the operating system hands
//! them over, and on Unix, whose arguments stand in bytes,
//! no value the shell hands to the filesystem or to the gate
//! command is lossy converted. On a platform without byte
//! arguments the reading of a word is the lossy reading the
//! word_bytes note below states. A generated pattern word
//! carrying bytes that are not UTF 8 text is carried in its
//! bytes. It is decoded the way the Python runtime decodes
//! its argument words, each byte that no UTF 8 sequence
//! claims standing as a byte token of its own kind beside
//! the character tokens, and the record the fingerprint
//! door prints spells it with the JSON surrogate escape, as
//! the Python print spells it. The matching the library
//! runs reads the paths of the tree in the same token
//! alphabet, so a genuine character standing in a path, of
//! the private use block or any other, is never a standing
//! in for a byte, and every path spelled with bytes that
//! are not UTF 8 text is refused by the reading before a
//! match is tried. On the stamp door such a word, whether
//! it stands as a pattern or as a word of the gate
//! command, is refused ahead of the run in the error
//! family, the tree the door reads standing read before
//! the refusal fires, because the Python command dies
//! uncaught when the stamped log carrying the word is
//! written, and the crash limit of the port states the
//! translation.

use std::ffi::OsString;
use std::path::Path;
use std::process::ExitCode;

use finaltree::errors::FinaltreeError;
use finaltree::fingerprint::compute_fingerprint_tokens;
use finaltree::glob::{decode_word, Token};
use finaltree::render::render_printed;
use finaltree::stamping::{run_and_stamp_tokens, verify_stamped_log_tokens};

const USAGE_TOP: &str = "usage: finaltree {fingerprint,stamp,verify} ...\n";
const USAGE_FINGERPRINT: &str = "usage: finaltree fingerprint [--generated PATTERN ...] ROOT\n";
const USAGE_STAMP: &str =
    "usage: finaltree stamp ROOT --log PATH [--generated PATTERN ...] [COMMAND ...]\n";
const USAGE_VERIFY: &str = "usage: finaltree verify LOG ROOT [--generated PATTERN ...]\n";

/// What a run of the command line comes to. A done run
/// carries the exit code the door stands at, a help run
/// carries the usage line to print to standard output, a
/// usage run carries the line to print to standard error,
/// and a refused run carries the library refusal to print
/// behind the error prefix.
enum Outcome {
    Done(i32),
    Help(&'static str),
    Usage(&'static str),
    Refuse(FinaltreeError),
}

/// The doors of the command line, with the top level standing
/// as its own reading for the words before the door word.
#[derive(Clone, Copy, PartialEq)]
enum Door {
    Top,
    Fingerprint,
    Stamp,
    Verify,
}

impl Door {
    fn usage(self) -> &'static str {
        match self {
            Door::Top => USAGE_TOP,
            Door::Fingerprint => USAGE_FINGERPRINT,
            Door::Stamp => USAGE_STAMP,
            Door::Verify => USAGE_VERIFY,
        }
    }
}

/// The bytes of one argument word as the operating system
/// handed it over.
#[cfg(unix)]
fn word_bytes(word: &OsString) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    word.as_os_str().as_bytes().to_vec()
}

/// The bytes of one argument word on a platform without byte
/// arguments, where the lossy reading is the only reading.
#[cfg(not(unix))]
fn word_bytes(word: &OsString) -> Vec<u8> {
    word.to_string_lossy().into_owned().into_bytes()
}

/// Whether a word is a negative number word to the argument
/// reader, an optional minus and digits with an optional
/// point and more digits, the digits before the point
/// standing empty where digits stand after it, the whole
/// word matching.
fn is_negative_number(bytes: &[u8]) -> bool {
    if bytes.first() != Some(&b'-') {
        return false;
    }
    let rest = &bytes[1..];
    if rest.is_empty() {
        return false;
    }
    if rest.iter().all(|byte| byte.is_ascii_digit()) {
        return true;
    }
    match rest.iter().position(|byte| *byte == b'.') {
        Some(dot) => {
            let before = &rest[..dot];
            let after = &rest[dot + 1..];
            before.iter().all(|byte| byte.is_ascii_digit())
                && !after.is_empty()
                && after.iter().all(|byte| byte.is_ascii_digit())
        }
        None => false,
    }
}

/// A long flag the reading resolves a name to.
#[derive(Clone, Copy, PartialEq)]
enum LongFlag {
    Generated,
    Log,
    Help,
}

/// Resolve a long flag name, whole or as the one abbreviation
/// that stands for it, against the flags a door carries. The
/// help flag stands on every door, the generated flag stands
/// on every door, and the log flag stands on the stamp door.
fn resolve_long(name: &[u8], door: Door) -> Option<LongFlag> {
    let flags: &[(&[u8], LongFlag)] = match door {
        Door::Top => &[(b"--help", LongFlag::Help)],
        Door::Fingerprint | Door::Verify => &[
            (b"--generated", LongFlag::Generated),
            (b"--help", LongFlag::Help),
        ],
        Door::Stamp => &[
            (b"--generated", LongFlag::Generated),
            (b"--log", LongFlag::Log),
            (b"--help", LongFlag::Help),
        ],
    };
    for (spelling, flag) in flags {
        if name == *spelling {
            return Some(*flag);
        }
    }
    let mut found: Option<LongFlag> = None;
    for (spelling, flag) in flags {
        if spelling.len() > name.len() && spelling.starts_with(name) {
            match found {
                None => found = Some(*flag),
                Some(_) => return None,
            }
        }
    }
    found
}

/// What one argument word stands as in the reading of a door.
#[derive(Clone, Copy, PartialEq)]
enum WordKind {
    /// A word the parser takes as a positional or as the
    /// value of a flag, including a lone dash, a negative
    /// number word, and any word carrying a space.
    Positional,
    /// The double dash, after which every word is a positional.
    Separator,
    /// The help flag, which answers when the scan reaches it.
    Help,
    /// The help flag carrying an explicit value, which the
    /// reader refuses in the scan.
    MisvaluedHelp,
    /// A flag taking a value.
    Flag(LongFlag),
    /// An option word the door does not carry, whose refusal
    /// waits for the end of the scan.
    Unknown,
}

/// Classify one argument word the way the argument reader
/// classifies it, as the probe recorded.
fn classify(word: &OsString, door: Door) -> WordKind {
    let bytes = word_bytes(word);
    if bytes == b"--" {
        return WordKind::Separator;
    }
    if bytes.len() > 2 && bytes.starts_with(b"--") {
        let (name, explicit) = match bytes.iter().position(|byte| *byte == b'=') {
            Some(at) => (&bytes[..at], true),
            None => (&bytes[..], false),
        };
        return match resolve_long(name, door) {
            Some(LongFlag::Help) => {
                if explicit {
                    WordKind::MisvaluedHelp
                } else {
                    WordKind::Help
                }
            }
            Some(flag) => WordKind::Flag(flag),
            None => {
                if bytes.contains(&b' ') {
                    WordKind::Positional
                } else {
                    WordKind::Unknown
                }
            }
        };
    }
    if bytes.len() >= 2 && bytes[0] == b'-' {
        if bytes[1] == b'h' {
            if bytes.len() > 2 && bytes[2] == b'=' {
                return WordKind::MisvaluedHelp;
            }
            return WordKind::Help;
        }
        if is_negative_number(&bytes) {
            return WordKind::Positional;
        }
        if bytes.contains(&b' ') {
            return WordKind::Positional;
        }
        return WordKind::Unknown;
    }
    WordKind::Positional
}

/// The explicit value a long flag word carries after its
/// equals sign, where it carries one.
fn explicit_value(word: &OsString) -> Option<OsString> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let bytes = word.as_os_str().as_bytes();
        if bytes.starts_with(b"--") {
            if let Some(at) = bytes.iter().position(|byte| *byte == b'=') {
                return Some(OsString::from_vec(bytes[at + 1..].to_vec()));
            }
        }
        None
    }
    #[cfg(not(unix))]
    {
        let text = word.to_string_lossy().into_owned();
        if text.starts_with("--") {
            if let Some(at) = text.find('=') {
                return Some(OsString::from(&text[at + 1..]));
            }
        }
        None
    }
}

/// The standing of one door parse.
struct Scan {
    /// Every positional word, in the order the words stood.
    positionals: Vec<OsString>,
    /// Whether a positional word stood after the first run of
    /// positionals was sealed by an option word, which the
    /// stamp door refuses and the other doors count.
    later_positional: bool,
    /// The pattern words the generated flag gathered, in order.
    generated: Vec<OsString>,
    /// The log word the log flag carried, the last standing.
    log: Option<OsString>,
    /// Whether an option word the door does not carry stood.
    extras: bool,
}

enum ScanOutcome {
    Scanned(Scan),
    Help,
    Usage,
}

/// Read the argument words of one door under the model the
/// probe recorded. Option words are consumed in the order
/// they stand, the help flag answers when the scan reaches
/// it, and a flag whose value does not stand refuses in the
/// scan. Unknown option words and surplus positionals wait
/// for the end of the scan, as the reader leaves them.
fn scan(door: Door, args: &[OsString]) -> ScanOutcome {
    let mut scan = Scan {
        positionals: Vec::new(),
        later_positional: false,
        generated: Vec::new(),
        log: None,
        extras: false,
    };
    let mut sealed = false;
    let mut separator = false;
    let mut index = 0;
    while index < args.len() {
        let word = &args[index];
        if separator {
            if sealed {
                scan.later_positional = true;
            }
            scan.positionals.push(word.clone());
            index += 1;
            continue;
        }
        match classify(word, door) {
            WordKind::Separator => {
                separator = true;
                index += 1;
            }
            WordKind::Help => return ScanOutcome::Help,
            WordKind::MisvaluedHelp => return ScanOutcome::Usage,
            WordKind::Flag(flag) => {
                let value = match explicit_value(word) {
                    Some(value) => value,
                    None => match args.get(index + 1) {
                        Some(next) if classify(next, door) == WordKind::Positional => {
                            index += 1;
                            next.clone()
                        }
                        _ => return ScanOutcome::Usage,
                    },
                };
                if !scan.positionals.is_empty() {
                    sealed = true;
                }
                match flag {
                    LongFlag::Generated => scan.generated.push(value),
                    LongFlag::Log => scan.log = Some(value),
                    LongFlag::Help => unreachable!("the help flag carries no value"),
                }
                index += 1;
            }
            WordKind::Unknown => {
                scan.extras = true;
                if !scan.positionals.is_empty() {
                    sealed = true;
                }
                index += 1;
            }
            WordKind::Positional => {
                if sealed {
                    scan.later_positional = true;
                }
                scan.positionals.push(word.clone());
                index += 1;
            }
        }
    }
    ScanOutcome::Scanned(scan)
}

/// Carry one pattern word in its bytes, decoding it into
/// tokens the way the Python runtime decodes its argument
/// words, each byte no UTF 8 sequence claims standing as a
/// byte token of its own kind.
fn decode_pattern(word: &OsString) -> Vec<Token> {
    decode_word(&word_bytes(word))
}

/// The fingerprint door. The record the library computes is
/// printed in the spelling the Python print carries, with
/// the pattern words standing in the record as the words
/// were given, and the door exits 0. The caller has already
/// answered a scan standing with extras or with a positional
/// count the door does not carry, so the scan stands whole.
fn run_fingerprint(scan: &Scan) -> Outcome {
    let root = &scan.positionals[0];
    let patterns: Vec<Vec<Token>> = scan.generated.iter().map(decode_pattern).collect();
    match compute_fingerprint_tokens(Path::new(root), &patterns) {
        Ok(fingerprint) => {
            println!("{}", render_printed(&fingerprint.to_dict()));
            Outcome::Done(0)
        }
        Err(error) => Outcome::Refuse(error),
    }
}

/// A stamp call as the parse of the door leaves it, ready to
/// run against the library.
struct StampCall {
    root: OsString,
    log: OsString,
    command: Vec<OsString>,
    generated: Vec<OsString>,
}

/// Parse the stamp door. The argument list is pre-split at
/// the first double dash standing after the door word where
/// the door word stood first, everything past the split
/// standing as the gate command in its bytes, and where no
/// double dash stands the gate command is what the scan
/// collected into the trailing positionals of its first run.
fn parse_stamp(args: &[OsString], presplit: bool) -> Result<StampCall, Outcome> {
    let (head, tail): (&[OsString], &[OsString]) = if presplit {
        match args.iter().position(|word| word_bytes(word) == b"--") {
            Some(cut) => (&args[..cut], &args[cut + 1..]),
            None => (args, &[]),
        }
    } else {
        (args, &[])
    };
    let scan = match scan(Door::Stamp, head) {
        ScanOutcome::Scanned(scan) => scan,
        ScanOutcome::Help => return Err(Outcome::Help(USAGE_STAMP)),
        ScanOutcome::Usage => return Err(Outcome::Usage(USAGE_STAMP)),
    };
    if scan.extras || scan.later_positional {
        return Err(Outcome::Usage(USAGE_STAMP));
    }
    let Some(root) = scan.positionals.first() else {
        return Err(Outcome::Usage(USAGE_STAMP));
    };
    let Some(log) = &scan.log else {
        return Err(Outcome::Usage(USAGE_STAMP));
    };
    let mut command: Vec<OsString> = tail.to_vec();
    if command.is_empty() {
        command = scan.positionals[1..].to_vec();
    }
    Ok(StampCall {
        root: root.clone(),
        log: log.clone(),
        command,
        generated: scan.generated.clone(),
    })
}

/// Run a parsed stamp call against the library. A command
/// standing empty is the door line of the Python command
/// itself. The tree is read before any word refusal fires,
/// as the Python reading reads the tree in its fingerprint
/// computation ahead of the standings where its words stop
/// it, so a tree that refuses speaks ahead of a pattern
/// word or a command word carrying bytes that are not UTF 8
/// text, and such a word then refuses ahead of the run, as
/// the module note states.
fn run_stamp(call: &StampCall) -> Outcome {
    if call.command.is_empty() {
        return Outcome::Refuse(FinaltreeError::new(
            "the stamp command names no gate command",
        ));
    }
    let patterns: Vec<Vec<Token>> = call.generated.iter().map(decode_pattern).collect();
    if let Err(error) = compute_fingerprint_tokens(Path::new(&call.root), &patterns) {
        return Outcome::Refuse(error);
    }
    for word in &call.generated {
        if word.to_str().is_none() {
            return Outcome::Refuse(FinaltreeError::new("a generated pattern is not UTF-8 text"));
        }
    }
    let mut command_words: Vec<String> = Vec::new();
    for word in &call.command {
        match word.to_str() {
            Some(text) => command_words.push(text.to_string()),
            None => {
                return Outcome::Refuse(FinaltreeError::new(
                    "a gate command word is not UTF-8 text",
                ))
            }
        }
    }
    match run_and_stamp_tokens(
        Path::new(&call.root),
        &command_words,
        Path::new(&call.log),
        &patterns,
    ) {
        Ok(code) => Outcome::Done(code),
        Err(error) => Outcome::Refuse(error),
    }
}

/// The verify door. The patterns govern as the log records
/// them where the generated flag never stood, and as given
/// where it stood even once. The answer stands on standard
/// output, match at exit 0 and mismatch with one line per
/// difference at exit 1. The caller has already answered a
/// scan standing with extras or with a positional count the
/// door does not carry, so the scan stands whole.
fn run_verify(scan: &Scan) -> Outcome {
    let log = &scan.positionals[0];
    let root = &scan.positionals[1];
    let patterns: Vec<Vec<Token>> = scan.generated.iter().map(decode_pattern).collect();
    let patterns = if scan.generated.is_empty() {
        None
    } else {
        Some(patterns.as_slice())
    };
    match verify_stamped_log_tokens(Path::new(log), Path::new(root), patterns) {
        Ok(result) => {
            if result.matched {
                println!("match");
                Outcome::Done(0)
            } else {
                println!("mismatch");
                for difference in &result.differences {
                    println!("{difference}");
                }
                Outcome::Done(1)
            }
        }
        Err(error) => Outcome::Refuse(error),
    }
}

/// Read the words before the door word the way the top level
/// of the parser reads them. The door word is the first
/// positional word. An unknown option word waits for the end
/// of the parse, a help word answers where it stands ahead
/// of the door word, and a positional word that is not a
/// door word is refused where it stands, as the reader
/// refuses it before any later word is read.
fn run() -> Outcome {
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    if argv.is_empty() {
        return Outcome::Usage(USAGE_TOP);
    }
    let mut top_extras = false;
    let mut index = 0;
    let (door, door_index) = loop {
        if index >= argv.len() {
            return Outcome::Usage(USAGE_TOP);
        }
        let word = &argv[index];
        let bytes = word_bytes(word);
        if bytes == b"fingerprint" {
            break (Door::Fingerprint, index);
        }
        if bytes == b"stamp" {
            break (Door::Stamp, index);
        }
        if bytes == b"verify" {
            break (Door::Verify, index);
        }
        match classify(word, Door::Top) {
            WordKind::Help => return Outcome::Help(USAGE_TOP),
            WordKind::MisvaluedHelp => return Outcome::Usage(USAGE_TOP),
            WordKind::Separator | WordKind::Positional => return Outcome::Usage(USAGE_TOP),
            WordKind::Flag(_) => unreachable!("the top level carries no flag"),
            WordKind::Unknown => {
                top_extras = true;
                index += 1;
            }
        }
    };
    let args = &argv[door_index + 1..];
    // An unknown option word standing ahead of the door word
    // refuses at the end of the top level parse, which is
    // after the door parse has run and before the door runs,
    // so every door is parsed whole before it is run and the
    // standing of that word is applied between the two.
    if door == Door::Stamp {
        return match parse_stamp(args, door_index == 0) {
            Ok(call) => {
                if top_extras {
                    Outcome::Usage(USAGE_TOP)
                } else {
                    run_stamp(&call)
                }
            }
            Err(outcome) => outcome,
        };
    }
    match scan(door, args) {
        ScanOutcome::Scanned(scan) => {
            let wanted = match door {
                Door::Fingerprint => 1,
                Door::Verify => 2,
                Door::Stamp | Door::Top => unreachable!("the door stands resolved"),
            };
            if scan.extras || scan.positionals.len() != wanted {
                return Outcome::Usage(door.usage());
            }
            if top_extras {
                return Outcome::Usage(USAGE_TOP);
            }
            match door {
                Door::Fingerprint => run_fingerprint(&scan),
                Door::Verify => run_verify(&scan),
                Door::Stamp | Door::Top => unreachable!("the door stands resolved"),
            }
        }
        ScanOutcome::Help => Outcome::Help(door.usage()),
        ScanOutcome::Usage => Outcome::Usage(door.usage()),
    }
}

fn main() -> ExitCode {
    match run() {
        Outcome::Done(code) => ExitCode::from(u8::try_from(code).unwrap_or(2)),
        Outcome::Help(line) => {
            print!("{line}");
            ExitCode::from(0)
        }
        Outcome::Usage(line) => {
            eprint!("{line}");
            ExitCode::from(2)
        }
        Outcome::Refuse(error) => {
            eprintln!("error. {error}");
            ExitCode::from(2)
        }
    }
}
