//! The command line battery of the closing slice.
//!
//! Replays the recorded invocations of
//! tests/conformance/cli_cases.json against the built binary.
//! Each case builds its synthetic tree in a scratch directory
//! carrying the process id in its name and removed when the
//! case ends, under the same pinned git identity and dates the
//! slice 1 recordings stand under, and compares the exit code,
//! the standard output, the standard error, and the stamped
//! log against the record the landed Python command line left.
//! A step the record marks as an argument reader step asserts
//! the binary writes its own usage line for the door in place
//! of the recorded reader text, because the contract states
//! the argument reader texts stand outside the port. The two
//! pins of the slice stand at the end, one for the pattern
//! word carrying a byte the text encoding cannot carry and
//! one for the stamped log nested past the reader bound, and
//! beside them a record test that builds a record with its
//! member lists standing reversed and asserts the emission
//! stands in canonical order.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use finaltree::fingerprint::{compute_fingerprint, Fingerprint};
use finaltree::jsonvalue::{parse_json_lenient, JsonValue};
use finaltree::render::{render_compact, render_printed};

mod common;
use common::{git, git_succeeds, make_factory_repo, pinned, write_file, Scratch};

const USAGE_TOP: &str = "usage: finaltree {fingerprint,stamp,verify} ...\n";
const USAGE_FINGERPRINT: &str = "usage: finaltree fingerprint [--generated PATTERN ...] ROOT\n";
const USAGE_STAMP: &str =
    "usage: finaltree stamp ROOT --log PATH [--generated PATTERN ...] [COMMAND ...]\n";
const USAGE_VERIFY: &str = "usage: finaltree verify LOG ROOT [--generated PATTERN ...]\n";

fn usage_for(door: &str) -> &'static str {
    match door {
        "top" => USAGE_TOP,
        "fingerprint" => USAGE_FINGERPRINT,
        "stamp" => USAGE_STAMP,
        "verify" => USAGE_VERIFY,
        other => panic!("the record names an unknown door {other}"),
    }
}

fn field<'a>(value: &'a JsonValue, key: &str) -> &'a JsonValue {
    value
        .get(key)
        .unwrap_or_else(|| panic!("a record carries no {key}"))
}

fn text_of(value: &JsonValue) -> &str {
    match value {
        JsonValue::Text(text) => text,
        JsonValue::SurrogateText(_) => panic!("a record field stands as text"),
        other => panic!("a record field stands as text, not {other:?}"),
    }
}

fn list_of(value: &JsonValue) -> &Vec<JsonValue> {
    match value {
        JsonValue::List(items) => items,
        other => panic!("a record field stands as a list, not {other:?}"),
    }
}

fn int_of(value: &JsonValue) -> i64 {
    match value {
        JsonValue::Int(number) => *number,
        other => panic!("a record field stands as a number, not {other:?}"),
    }
}

fn bool_of(value: &JsonValue) -> bool {
    match value {
        JsonValue::Bool(flag) => *flag,
        other => panic!("a record field stands as a boolean, not {other:?}"),
    }
}

fn base64_decode(text: &str) -> Vec<u8> {
    fn value_of(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }
    let mut out = Vec::new();
    let mut window: Vec<u8> = Vec::new();
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        if let Some(value) = value_of(byte) {
            window.push(value);
            if window.len() == 4 {
                out.push((window[0] << 2) | (window[1] >> 4));
                out.push((window[1] << 4) | (window[2] >> 2));
                out.push((window[2] << 6) | window[3]);
                window.clear();
            }
        }
    }
    if window.len() == 2 {
        out.push((window[0] << 2) | (window[1] >> 4));
    } else if window.len() == 3 {
        out.push((window[0] << 2) | (window[1] >> 4));
        out.push((window[1] << 4) | (window[2] >> 2));
    }
    out
}

fn word_bytes(value: &JsonValue) -> Vec<u8> {
    match value {
        JsonValue::Text(text) => text.as_bytes().to_vec(),
        JsonValue::SurrogateText(units) => {
            let mut out = Vec::new();
            let mut index = 0;
            while index < units.len() {
                let unit = units[index];
                if (0xD800..0xDC00).contains(&unit)
                    && index + 1 < units.len()
                    && (0xDC00..0xE000).contains(&units[index + 1])
                {
                    let point = 0x10000
                        + (((unit as u32 - 0xD800) << 10) | (units[index + 1] as u32 - 0xDC00));
                    let ch = char::from_u32(point).expect("a pair stands as a character");
                    let mut buffer = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                    index += 2;
                } else if (0xDC80..0xDD00).contains(&unit) {
                    out.push((unit - 0xDC00) as u8);
                    index += 1;
                } else {
                    let ch =
                        char::from_u32(unit as u32).expect("a word unit stands as a character");
                    let mut buffer = [0u8; 4];
                    out.extend_from_slice(ch.encode_utf8(&mut buffer).as_bytes());
                    index += 1;
                }
            }
            out
        }
        other => panic!("an argv word stands as text, not {other:?}"),
    }
}

fn replace_all(haystack: &[u8], needle: &[u8], replacement: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut index = 0;
    while index < haystack.len() {
        if haystack[index..].starts_with(needle) {
            out.extend_from_slice(replacement);
            index += needle.len();
        } else {
            out.push(haystack[index]);
            index += 1;
        }
    }
    out
}

struct CasePaths {
    case_dir: PathBuf,
    root: PathBuf,
    log: PathBuf,
    logdir: PathBuf,
}

impl CasePaths {
    fn substitute(&self, word: &[u8]) -> Vec<u8> {
        let word = replace_all(word, b"{LOGDIR}", self.logdir.as_os_str().as_bytes());
        let word = replace_all(&word, b"{LOG}", self.log.as_os_str().as_bytes());
        replace_all(&word, b"{ROOT}", self.root.as_os_str().as_bytes())
    }

    fn unsubstitute(&self, bytes: &[u8]) -> String {
        let bytes = replace_all(bytes, self.logdir.as_os_str().as_bytes(), b"{LOGDIR}");
        let bytes = replace_all(&bytes, self.log.as_os_str().as_bytes(), b"{LOG}");
        let bytes = replace_all(&bytes, self.root.as_os_str().as_bytes(), b"{ROOT}");
        String::from_utf8(bytes).expect("a printed text stands as UTF-8")
    }
}

fn apply_op(case: &JsonValue, paths: &CasePaths) {
    let kind = text_of(field(case, "op"));
    let root = &paths.root;
    match kind {
        "init" => make_factory_repo(root),
        "plain" => {}
        "git_init" => {
            fs::create_dir_all(root).expect("the fixture root stands");
            git(root, &["init", "-q", "-b", "main", "."]);
        }
        "write" => {
            write_file(
                root,
                text_of(field(case, "path")),
                text_of(field(case, "text")),
            );
        }
        "write_exec" => {
            let path = write_file(
                root,
                text_of(field(case, "path")),
                text_of(field(case, "text")),
            );
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
                .expect("the fixture script stands runnable");
        }
        "write_b64" => {
            let name = base64_decode(text_of(field(case, "name_b64")));
            let content = base64_decode(text_of(field(case, "content_b64")));
            let path = root.join(OsString::from_vec(name));
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("the fixture folder stands");
            }
            fs::write(&path, content).expect("the fixture file is written");
        }
        "git" => {
            let args: Vec<&str> = list_of(field(case, "args")).iter().map(text_of).collect();
            git(root, &args);
        }
        "git_fail" => {
            let args: Vec<&str> = list_of(field(case, "args")).iter().map(text_of).collect();
            assert!(
                !git_succeeds(root, &args),
                "the git step fails in the fixture"
            );
        }
        "symlink" => {
            std::os::unix::fs::symlink(
                text_of(field(case, "target")),
                root.join(text_of(field(case, "path"))),
            )
            .expect("the fixture link stands");
        }
        "unlink" => {
            fs::remove_file(root.join(text_of(field(case, "path"))))
                .expect("the fixture file falls");
        }
        "commit_all" => {
            git(root, &["add", "."]);
            let message = case.get("message").map(text_of).unwrap_or("change");
            git(root, &["commit", "-q", "-m", message]);
        }
        "write_log" => {
            fs::write(&paths.log, text_of(field(case, "text"))).expect("the log is written");
        }
        "edit_log" => {
            let text = fs::read_to_string(&paths.log).expect("the log stands");
            let from = text_of(field(case, "from"));
            assert!(
                text.contains(from),
                "the log carries the line the edit names"
            );
            fs::write(
                &paths.log,
                text.replacen(from, text_of(field(case, "to")), 1),
            )
            .expect("the log is edited");
        }
        "scramble_log" => {
            let text = fs::read_to_string(&paths.log).expect("the log stands");
            let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
            let mut done = false;
            for line in lines.iter_mut() {
                if let Some(raw) = line.strip_prefix("fingerprint-json ") {
                    let mut record = parse_json_lenient(raw).expect("the record parses");
                    if let JsonValue::Object(pairs) = &mut record {
                        for (key, value) in pairs.iter_mut() {
                            if ["tracked", "dirty", "generated", "untracked"]
                                .contains(&key.as_str())
                            {
                                if let JsonValue::List(items) = value {
                                    items.reverse();
                                }
                            }
                        }
                    }
                    *line = format!("fingerprint-json {}", render_compact(&record));
                    done = true;
                    break;
                }
            }
            assert!(done, "the log carries a fingerprint line to scramble");
            fs::write(&paths.log, lines.join("\n")).expect("the log is written");
        }
        other => panic!("the battery carries an unknown op {other}"),
    }
}

fn run_binary(paths: &CasePaths, argv: &[Vec<u8>], stdin: Option<&str>) -> (i64, Vec<u8>, Vec<u8>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_finaltree"));
    pinned(&mut command);
    command
        .args(argv.iter().map(|word| OsString::from_vec(word.clone())))
        .current_dir(&paths.case_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().expect("the binary runs");
    if let Some(stdin) = stdin {
        child
            .stdin
            .take()
            .expect("the door stands open")
            .write_all(stdin.as_bytes())
            .expect("the input is written");
    }
    let output = child.wait_with_output().expect("the binary answers");
    (
        output.status.code().expect("the binary exits with a code") as i64,
        output.stdout,
        output.stderr,
    )
}

fn check_text(case_name: &str, what: &str, actual: &[u8], paths: &CasePaths, expected: &JsonValue) {
    let expected = text_of(expected);
    let actual = paths.unsubstitute(actual);
    assert_eq!(actual, expected, "case {case_name} {what}");
}

fn run_step(case_name: &str, run: &JsonValue, paths: &CasePaths) {
    let argv: Vec<Vec<u8>> = list_of(field(run, "argv"))
        .iter()
        .map(|word| paths.substitute(&word_bytes(word)))
        .collect();
    let stdin = run.get("stdin").map(text_of);
    let (exit, stdout, stderr) = run_binary(paths, &argv, stdin);
    let expect = field(run, "expect");
    let python = field(run, "python");
    assert_eq!(
        exit,
        int_of(field(expect, "exit")),
        "case {case_name} exit code"
    );
    let crashed = python.get("crash").map(bool_of).unwrap_or(false);
    if !crashed {
        assert_eq!(
            int_of(field(python, "exit")),
            int_of(field(expect, "exit")),
            "case {case_name} record stands consistent"
        );
    }
    if matches!(field(expect, "stdout"), JsonValue::Null) {
        let door = text_of(field(expect, "usage"));
        assert_eq!(
            paths.unsubstitute(&stdout),
            usage_for(door),
            "case {case_name} help prints the door usage line"
        );
        check_text(
            case_name,
            "standard error",
            &stderr,
            paths,
            field(expect, "stderr"),
        );
        return;
    }
    check_text(
        case_name,
        "standard output",
        &stdout,
        paths,
        field(expect, "stdout"),
    );
    if let Some(door) = expect.get("usage") {
        assert_eq!(
            paths.unsubstitute(&stderr),
            usage_for(text_of(door)),
            "case {case_name} refusal prints the door usage line"
        );
    } else if let Some(prefix) = expect.get("stderr_prefix") {
        let stderr_text = paths.unsubstitute(&stderr);
        assert!(
            stderr_text.starts_with(text_of(prefix)),
            "case {case_name} standard error opens with the recorded prefix"
        );
    } else {
        check_text(
            case_name,
            "standard error",
            &stderr,
            paths,
            field(expect, "stderr"),
        );
    }
    if let Some(log_text) = expect.get("log_text") {
        let actual = fs::read(&paths.log).expect("the stamped log stands");
        assert_eq!(
            paths.unsubstitute(&actual),
            text_of(log_text),
            "case {case_name} stamped log"
        );
    }
    if let Some(absent) = expect.get("log_absent") {
        if bool_of(absent) {
            assert!(
                !paths.log.exists(),
                "case {case_name} leaves no log standing"
            );
        }
    }
}

#[test]
fn test_cli_battery_matches_the_python_record() {
    let text = fs::read_to_string("tests/conformance/cli_cases.json")
        .expect("the battery recordings stand");
    let document = parse_json_lenient(&text).expect("the battery recordings parse");
    let cases = list_of(field(&document, "cases"));
    let mut counts: Vec<(String, usize)> = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let section = text_of(field(case, "section")).to_string();
        match counts.iter_mut().find(|(name, _)| *name == section) {
            Some((_, count)) => *count += 1,
            None => counts.push((section, 1)),
        }
        let name = text_of(field(case, "name"));
        let scratch = Scratch::new("cli", &format!("case-{index:03}"));
        let case_dir = scratch.0.clone();
        let root = match case.get("root_name_b64") {
            Some(encoded) => case_dir.join(OsString::from_vec(base64_decode(text_of(encoded)))),
            None => case_dir.join("repo"),
        };
        fs::create_dir_all(&root).expect("the case root stands");
        let mut log_name = root.as_os_str().to_owned();
        log_name.push(".log");
        let paths = CasePaths {
            log: PathBuf::from(log_name),
            logdir: case_dir.join("missing").join("stamped.log"),
            case_dir,
            root,
        };
        for action in list_of(field(case, "actions")) {
            if let Some(run) = action.get("run") {
                run_step(name, run, &paths);
            } else {
                apply_op(action, &paths);
            }
        }
    }
    let count_of = |section: &str| {
        counts
            .iter()
            .find(|(name, _)| name == section)
            .map(|(_, count)| *count)
            .unwrap_or(0)
    };
    assert_eq!(
        count_of("behaviours"),
        28,
        "the behaviours section stands whole"
    );
    assert_eq!(
        count_of("arguments"),
        26,
        "the arguments section stands whole"
    );
    assert_eq!(
        count_of("refusals"),
        13,
        "the refusals section stands whole"
    );
    assert_eq!(cases.len(), 67, "the battery stands whole");
}

fn run_words(words: &[&[u8]], cwd: &Path) -> (i64, Vec<u8>, Vec<u8>) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_finaltree"));
    pinned(&mut command);
    command
        .args(words.iter().map(|word| OsString::from_vec(word.to_vec())))
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().expect("the binary answers");
    (
        output.status.code().expect("the binary exits with a code") as i64,
        output.stdout,
        output.stderr,
    )
}

#[test]
fn test_a_pattern_word_carrying_a_non_utf8_byte_is_carried() {
    let scratch = Scratch::new("cli", "pattern-pin");
    let root = scratch.0.join("repo");
    make_factory_repo(&root);
    write_file(&root, "a\u{FFFD}b", "replacement\n");
    let root_word = root.as_os_str().as_bytes().to_vec();
    let (exit, stdout, _) = run_words(
        &[b"fingerprint", &root_word, b"--generated", b"a\xffb"],
        &scratch.0,
    );
    assert_eq!(exit, 0, "the fingerprint completes over the byte word");
    let printed = String::from_utf8(stdout).expect("the record prints as UTF-8");
    assert!(
        printed.contains("\"a\\udcffb\""),
        "the record spells the pattern word with its surrogate escape"
    );
    let record = parse_json_lenient(&printed).expect("the record parses");
    assert!(
        list_of(field(&record, "generated")).is_empty(),
        "the byte pattern matches no file on the tree"
    );
    let untracked: Vec<&str> = list_of(field(&record, "untracked"))
        .iter()
        .map(|entry| text_of(field(entry, "path")))
        .collect();
    assert!(
        untracked.contains(&"a\u{FFFD}b"),
        "the replacement named file stands as an ordinary untracked file"
    );
    let (exit, stdout, _) = run_words(
        &[
            b"fingerprint",
            &root_word,
            b"--generated",
            "a\u{FFFD}b".as_bytes(),
        ],
        &scratch.0,
    );
    assert_eq!(exit, 0, "the control fingerprint completes");
    let record =
        parse_json_lenient(&String::from_utf8(stdout).expect("the record prints as UTF-8"))
            .expect("the record parses");
    let generated = list_of(field(&record, "generated"));
    assert_eq!(generated.len(), 1, "the spelled pattern matches the file");
    assert_eq!(
        text_of(field(&generated[0], "path")),
        "a\u{FFFD}b",
        "the generated member is the replacement named file"
    );
    let mut log_name = root.as_os_str().to_owned();
    log_name.push(".log");
    let log = PathBuf::from(log_name);
    let log_word = log.as_os_str().as_bytes().to_vec();
    let (exit, _, _) = run_words(
        &[b"stamp", &root_word, b"--log", &log_word, b"--", b"true"],
        &scratch.0,
    );
    assert_eq!(exit, 0, "the stamp completes");
    let (exit, stdout, _) = run_words(
        &[b"verify", &log_word, &root_word, b"--generated", b"a\xffb"],
        &scratch.0,
    );
    assert_eq!(exit, 0, "the verify completes over the byte word");
    assert_eq!(stdout, b"match\n", "the verify answers match");
}

#[test]
fn test_a_stamped_log_nested_past_the_reader_bound_is_refused() {
    let scratch = Scratch::new("cli", "nesting-pin");
    let root = scratch.0.join("repo");
    make_factory_repo(&root);
    let root_word = root.as_os_str().as_bytes().to_vec();
    let mut log_name = root.as_os_str().to_owned();
    log_name.push(".log");
    let log = PathBuf::from(log_name);
    let log_word = log.as_os_str().as_bytes().to_vec();
    let (exit, _, _) = run_words(
        &[b"stamp", &root_word, b"--log", &log_word, b"--", b"true"],
        &scratch.0,
    );
    assert_eq!(exit, 0, "the stamp completes");
    let text = fs::read_to_string(&log).expect("the stamped log stands");
    let nested = format!("{}{}", "[".repeat(4500), "]".repeat(4500));
    let mut lines: Vec<String> = text.split('\n').map(str::to_string).collect();
    let mut done = false;
    for line in lines.iter_mut() {
        if line.starts_with("fingerprint-json ") {
            assert!(line.ends_with('}'), "the record line closes");
            line.insert_str(line.len() - 1, &format!(",\"zz-ignored\":{nested}"));
            done = true;
            break;
        }
    }
    assert!(done, "the log carries a fingerprint line");
    fs::write(&log, lines.join("\n")).expect("the log is rewritten");
    let (exit, stdout, stderr) = run_words(&[b"verify", &log_word, &root_word], &scratch.0);
    assert_eq!(exit, 2, "the port refuses the log at its reader bound");
    assert!(
        stdout.is_empty(),
        "the refusal prints nothing to standard output"
    );
    assert_eq!(
        String::from_utf8(stderr).expect("the refusal prints as UTF-8"),
        "error. the fingerprint record in the stamped log does not decode\n",
        "the refusal carries the reader wording"
    );
}

#[test]
fn test_record_dict_emits_members_in_canonical_order() {
    let scratch = Scratch::new("cli", "record-pin");
    let root = scratch.0.join("repo");
    make_factory_repo(&root);
    write_file(&root, "u1.txt", "one\n");
    write_file(&root, "u2.txt", "two\n");
    write_file(&root, "u3.txt", "three\n");
    write_file(&root, "a.txt", "alpha changed\n");
    let fingerprint = compute_fingerprint(&root, &[]).expect("the fingerprint stands");
    let canonical = render_printed(&fingerprint.to_dict());
    let mut record = parse_json_lenient(&canonical).expect("the record parses");
    let JsonValue::Object(pairs) = &mut record else {
        panic!("the record stands as an object");
    };
    for (key, value) in pairs.iter_mut() {
        if ["tracked", "dirty", "generated", "untracked"].contains(&key.as_str()) {
            if let JsonValue::List(items) = value {
                items.reverse();
            }
        }
    }
    let tracked_paths: Vec<&str> = list_of(field(&record, "tracked"))
        .iter()
        .map(|entry| text_of(field(entry, "path")))
        .collect();
    assert_eq!(
        tracked_paths,
        ["b.txt", "a.txt"],
        "the record under test stands with its member lists reversed"
    );
    let restored = Fingerprint::from_dict(&record).expect("the scrambled record reads back");
    assert_eq!(
        render_printed(&restored.to_dict()),
        canonical,
        "the emission of a scrambled record stands in canonical order"
    );
}
