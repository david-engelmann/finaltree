//! Recorded conformance for the Rust port slice 1.
//!
//! The recordings in `tests/conformance/port1_cases.json` were
//! produced by the generator in the Round 2 scratch, running
//! the landed Python over inputs recreated from the Python
//! suite's own scenarios. This test recreates the same inputs,
//! fixture repos included, and compares the port's answers.
//! Fixture repos build under a fixed identity and fixed dates
//! so commit hashes reproduce, and the placeholder `{ROOT}` in
//! a recorded text stands for the fixture build root.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use finaltree::fingerprint::{compute_fingerprint, Fingerprint};
use finaltree::glob;
use finaltree::intparse::parse_python_int;
use finaltree::jsonvalue::{parse_json, parse_json_lenient, JsonValue};
use finaltree::render::{json_float_text, render_compact, render_line, render_printed};
use finaltree::sha256::sha256_hex;
use finaltree::stamping::{
    compare_fingerprints, read_stamped_log, run_and_stamp, splitlines, verify_stamped_log,
};

fn recordings() -> JsonValue {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("conformance")
        .join("port1_cases.json");
    let text = std::fs::read_to_string(&path).expect("the recordings stand");
    parse_json(&text).expect("the recordings decode")
}

fn field<'a>(value: &'a JsonValue, key: &str) -> &'a JsonValue {
    value
        .get(key)
        .unwrap_or_else(|| panic!("a recorded field {key} stands"))
}

fn text_of(value: &JsonValue) -> &str {
    match value {
        JsonValue::Text(text) => text,
        _ => panic!("a recorded text stands, got {value:?}"),
    }
}

fn list_of(value: &JsonValue) -> &Vec<JsonValue> {
    match value {
        JsonValue::List(items) => items,
        _ => panic!("a recorded list stands, got {value:?}"),
    }
}

fn bool_of(value: &JsonValue) -> bool {
    match value {
        JsonValue::Bool(flag) => *flag,
        _ => panic!("a recorded bool stands, got {value:?}"),
    }
}

fn opt_text(value: &JsonValue) -> Option<&str> {
    match value {
        JsonValue::Null => None,
        JsonValue::Text(text) => Some(text),
        _ => panic!("a recorded optional text stands, got {value:?}"),
    }
}

fn text_list(value: &JsonValue) -> Vec<String> {
    list_of(value)
        .iter()
        .map(|item| text_of(item).to_string())
        .collect()
}

fn base_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "finaltree-conformance-{}-{}",
        std::process::id(),
        tag
    ));
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("a stale fixture clears");
    }
    std::fs::create_dir_all(&dir).expect("the fixture root stands");
    std::fs::canonicalize(&dir).expect("the fixture root resolves")
}

fn git_command() -> Command {
    let mut command = Command::new("git");
    command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Finaltree Tests")
        .env("GIT_AUTHOR_EMAIL", "tests@example.invalid")
        .env("GIT_COMMITTER_NAME", "Finaltree Tests")
        .env("GIT_COMMITTER_EMAIL", "tests@example.invalid")
        .env("GIT_AUTHOR_DATE", "2024-01-01T00:00:00+00:00")
        .env("GIT_COMMITTER_DATE", "2024-01-01T00:00:00+00:00")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdin(Stdio::null());
    command
}

fn git(root: &Path, args: &[&str]) {
    let status = git_command()
        .arg("-C")
        .arg(root)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} stands in the fixture");
}

fn base64_decode(text: &str) -> Vec<u8> {
    let mut table = [255u8; 256];
    for (index, ch) in b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .iter()
        .enumerate()
    {
        table[*ch as usize] = index as u8;
    }
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        buffer = (buffer << 6) | u32::from(table[byte as usize]);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}

fn write_file(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("fixture folders stand");
    }
    std::fs::write(path, text).expect("a fixture file is written");
}

fn apply_steps(root: &Path, steps: &JsonValue) {
    for step in list_of(steps) {
        let op = text_of(field(step, "op"));
        match op {
            "init" => {
                let status = git_command()
                    .args(["init", "-q", "-b", "main"])
                    .arg(root)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .expect("git init runs");
                assert!(status.success());
                write_file(root, "a.txt", "alpha\n");
                write_file(root, "b.txt", "beta\n");
                git(root, &["add", "."]);
                git(root, &["commit", "-q", "-m", "initial"]);
            }
            "plain" => {}
            "git_init" => {
                let status = git_command()
                    .args(["init", "-q", "-b", "main"])
                    .arg(root)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .expect("git init runs");
                assert!(status.success());
            }
            "write" => {
                write_file(
                    root,
                    text_of(field(step, "path")),
                    text_of(field(step, "text")),
                );
            }
            "write_b64" => {
                let name = base64_decode(text_of(field(step, "name_b64")));
                let content = base64_decode(text_of(field(step, "content_b64")));
                #[cfg(unix)]
                {
                    use std::os::unix::ffi::OsStringExt;
                    let path = root.join(std::ffi::OsString::from_vec(name));
                    std::fs::write(path, content).expect("a fixture file is written");
                }
                #[cfg(not(unix))]
                {
                    panic!("the byte named fixture is a unix fixture");
                }
            }
            "git" => {
                let args = text_list(field(step, "args"));
                let refs: Vec<&str> = args.iter().map(|arg| arg.as_str()).collect();
                git(root, &refs);
            }
            "git_b64" => {
                let mut command = git_command();
                command.arg("-C").arg(root);
                for item in list_of(field(step, "args_b64")) {
                    let bytes = base64_decode(text_of(item));
                    #[cfg(unix)]
                    {
                        use std::os::unix::ffi::OsStringExt;
                        command.arg(std::ffi::OsString::from_vec(bytes));
                    }
                    #[cfg(not(unix))]
                    {
                        panic!("the byte named fixture is a unix fixture");
                    }
                }
                let status = command
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .expect("git runs");
                assert!(status.success(), "a byte named git step stands");
            }
            "git_fail" => {
                let args = text_list(field(step, "args"));
                let refs: Vec<&str> = args.iter().map(|arg| arg.as_str()).collect();
                let status = git_command()
                    .arg("-C")
                    .arg(root)
                    .args(&refs)
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .expect("git runs");
                assert!(!status.success(), "git {refs:?} fails in the fixture");
            }
            "symlink" => {
                #[cfg(unix)]
                {
                    std::os::unix::fs::symlink(
                        text_of(field(step, "target")),
                        root.join(text_of(field(step, "path"))),
                    )
                    .expect("a fixture link stands");
                }
                #[cfg(not(unix))]
                {
                    panic!("the link fixture is a unix fixture");
                }
            }
            "unlink" => {
                std::fs::remove_file(root.join(text_of(field(step, "path"))))
                    .expect("a fixture file is removed");
            }
            "commit_all" => {
                git(root, &["add", "."]);
                let message = match step.get("message") {
                    Some(value) => text_of(value).to_string(),
                    None => "change".to_string(),
                };
                git(root, &["commit", "-q", "-m", &message]);
            }
            other => panic!("an unknown recorded step {other}"),
        }
    }
}

fn build_case(base: &Path, name: &str, steps: &JsonValue) -> PathBuf {
    let root = base.join(name);
    std::fs::create_dir_all(&root).expect("a case root stands");
    apply_steps(&root, steps);
    root
}

#[test]
fn conformance_fingerprint_cases() {
    let doc = recordings();
    let base = base_dir("fingerprint");
    let mut checked = 0;
    for case in list_of(field(&doc, "fingerprint_cases")) {
        let name = text_of(field(case, "name"));
        let root = build_case(&base, name, field(case, "steps"));
        let scenario_root = match case.get("root_suffix") {
            Some(suffix) if !matches!(suffix, JsonValue::Null) => root.join(text_of(suffix)),
            _ => root.clone(),
        };
        let patterns = text_list(field(case, "patterns"));
        let answer = field(case, "answer");
        match compute_fingerprint(&scenario_root, &patterns) {
            Ok(fingerprint) => {
                assert_eq!(
                    opt_text(field(answer, "error")),
                    None,
                    "case {name} stands in the port and not in the spec"
                );
                assert_eq!(
                    fingerprint.digest(),
                    text_of(field(answer, "digest")),
                    "case {name} digest"
                );
                let recorded =
                    parse_json(text_of(field(answer, "record_json"))).expect("a record decodes");
                assert_eq!(fingerprint.to_dict(), recorded, "case {name} record");
                assert_eq!(
                    render_printed(&fingerprint.to_dict()),
                    text_of(field(answer, "printed")),
                    "case {name} printed record"
                );
            }
            Err(error) => {
                let expected = opt_text(field(answer, "error"))
                    .unwrap_or_else(|| panic!("case {name} refuses in the port only"))
                    .replace("{ROOT}", &root.to_string_lossy());
                assert_eq!(error.message(), expected, "case {name} refusal");
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 26, "the fingerprint cases stand, saw {checked}");
}

#[test]
fn conformance_record_cases() {
    let doc = recordings();
    let mut checked = 0;
    for case in list_of(field(&doc, "record_cases")) {
        let name = text_of(field(case, "name"));
        let data =
            parse_json_lenient(text_of(field(case, "record_text"))).expect("a record decodes");
        let answer = field(case, "answer");
        match Fingerprint::from_dict(&data) {
            Ok(restored) => {
                assert_eq!(
                    opt_text(field(answer, "error")),
                    None,
                    "case {name} stands in the port and not in the spec"
                );
                assert_eq!(
                    restored.digest(),
                    text_of(field(answer, "digest")),
                    "case {name}"
                );
                let recorded =
                    parse_json(text_of(field(answer, "record_json"))).expect("a record decodes");
                assert_eq!(restored.to_dict(), recorded, "case {name} record");
            }
            Err(error) => {
                // A crash answer names a record the Python
                // reading stops on uncaught. The port refuses
                // the crash family in the decode words.
                let expected = match answer.get("crash") {
                    Some(_) => "the fingerprint record does not decode",
                    None => opt_text(field(answer, "error"))
                        .unwrap_or_else(|| panic!("case {name} refuses in the port only")),
                };
                assert_eq!(error.message(), expected, "case {name} refusal");
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 47, "the record cases stand, saw {checked}");
}

#[test]
fn conformance_log_cases() {
    let doc = recordings();
    let base = base_dir("logs");
    let mut checked = 0;
    for case in list_of(field(&doc, "log_cases")) {
        let name = text_of(field(case, "name"));
        let path = base.join(format!("{name}.log"));
        std::fs::write(&path, text_of(field(case, "log_text"))).expect("a log is written");
        let answer = field(case, "answer");
        match read_stamped_log(&path) {
            Ok(stamped) => {
                assert_eq!(
                    opt_text(field(answer, "error")),
                    None,
                    "case {name} stands in the port and not in the spec"
                );
                assert_eq!(
                    stamped.exit_code_text,
                    text_of(field(answer, "exit_code_text")),
                    "case {name} exit code"
                );
                let command: Vec<String> = stamped
                    .command
                    .iter()
                    .map(|value| match value {
                        JsonValue::Text(text) => text.clone(),
                        other => panic!("a command part is a text, got {other:?}"),
                    })
                    .collect();
                assert_eq!(
                    command,
                    text_list(field(answer, "command")),
                    "case {name} command"
                );
                assert_eq!(
                    stamped.output_lines,
                    text_list(field(answer, "output_lines")),
                    "case {name} output"
                );
                assert_eq!(
                    stamped.evidence_digest,
                    text_of(field(answer, "evidence_digest")),
                    "case {name} evidence"
                );
                assert_eq!(
                    stamped.fingerprint.digest(),
                    text_of(field(answer, "fingerprint_digest")),
                    "case {name} fingerprint"
                );
                let recorded =
                    parse_json(text_of(field(answer, "record_json"))).expect("a record decodes");
                assert_eq!(
                    stamped.fingerprint.to_dict(),
                    recorded,
                    "case {name} record"
                );
            }
            Err(error) => {
                let expected = opt_text(field(answer, "error"))
                    .unwrap_or_else(|| panic!("case {name} refuses in the port only"));
                assert_eq!(error.message(), expected, "case {name} refusal");
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 30, "the log cases stand, saw {checked}");
}

#[test]
fn conformance_stamp_cases() {
    let doc = recordings();
    let base = base_dir("stamp");
    let mut checked = 0;
    for case in list_of(field(&doc, "stamp_cases")) {
        let name = text_of(field(case, "name"));
        let root = build_case(&base, name, field(case, "steps"));
        let log_path = base.join(format!("{name}.log"));
        let patterns = text_list(field(case, "patterns"));
        let command = text_list(field(case, "command"));
        let answer = field(case, "answer");
        let exit = run_and_stamp(&root, &command, &log_path, &patterns)
            .unwrap_or_else(|error| panic!("case {name} stamps, refused. {error}"));
        let expected_exit = match field(answer, "exit_code") {
            JsonValue::Int(value) => *value as i32,
            other => panic!("a recorded exit code stands, got {other:?}"),
        };
        assert_eq!(exit, expected_exit, "case {name} exit code");
        let text = std::fs::read_to_string(&log_path).expect("the log stands");
        assert_eq!(
            text,
            text_of(field(answer, "log_text")),
            "case {name} log text"
        );
        checked += 1;
    }
    assert_eq!(checked, 5, "the stamp cases stand, saw {checked}");
}

#[test]
fn conformance_verify_cases() {
    let doc = recordings();
    let base = base_dir("verify");
    let mut checked = 0;
    for case in list_of(field(&doc, "verify_cases")) {
        let name = text_of(field(case, "name"));
        let root = build_case(&base, name, field(case, "steps"));
        let log_path = base.join(format!("{name}.log"));
        let patterns = text_list(field(case, "patterns"));
        let command = text_list(field(case, "command"));
        run_and_stamp(&root, &command, &log_path, &patterns)
            .unwrap_or_else(|error| panic!("case {name} stamps, refused. {error}"));
        apply_steps(&root, field(case, "mutation"));
        let verify_patterns: Option<Vec<String>> = match case.get("verify_patterns") {
            Some(JsonValue::Null) | None => None,
            Some(value) => Some(text_list(value)),
        };
        let result = verify_stamped_log(&log_path, &root, verify_patterns.as_deref())
            .unwrap_or_else(|error| panic!("case {name} verifies, refused. {error}"));
        let answer = field(case, "answer");
        assert_eq!(
            result.matched,
            bool_of(field(answer, "matched")),
            "case {name} matched"
        );
        assert_eq!(
            result.differences,
            text_list(field(answer, "differences")),
            "case {name} differences"
        );
        checked += 1;
    }
    assert_eq!(checked, 12, "the verify cases stand, saw {checked}");
}

#[test]
fn conformance_glob_battery() {
    let doc = recordings();
    let battery = field(&doc, "glob_battery");
    let paths = text_list(field(battery, "paths"));
    let cases = list_of(field(battery, "cases"));
    assert_eq!(cases.len(), 74, "the glob cases stand, saw {}", cases.len());
    let mut checked = 0;
    for case in cases {
        let pattern_text = text_of(field(case, "pattern"));
        let pattern = glob::translate(pattern_text);
        if bool_of(field(case, "error")) {
            assert!(
                pattern.refused(),
                "pattern {pattern_text} is refused by the port"
            );
            checked += 1;
            continue;
        }
        assert!(
            !pattern.refused(),
            "pattern {pattern_text} stands in the port"
        );
        let answers = field(case, "answers");
        for path in &paths {
            let expected = bool_of(field(answers, path));
            assert_eq!(
                glob::matches(&pattern, path),
                expected,
                "pattern {pattern_text} on path {path}"
            );
            checked += 1;
        }
    }
    assert_eq!(paths.len(), 51, "the glob paths stand, saw {}", paths.len());
    assert_eq!(checked, 3674, "the glob battery stands, saw {checked}");
}

#[test]
fn conformance_int_battery() {
    let doc = recordings();
    let mut checked = 0;
    for case in list_of(field(&doc, "int_battery")) {
        let token = text_of(field(case, "token"));
        assert_eq!(
            parse_python_int(token),
            opt_text(field(case, "answer")).map(|text| text.to_string()),
            "token of length {} starting {:?}",
            token.chars().count(),
            token.chars().take(8).collect::<String>()
        );
        checked += 1;
    }
    assert_eq!(checked, 75, "the int battery stands, saw {checked}");
}

#[test]
fn conformance_float_battery() {
    let doc = recordings();
    let mut checked = 0;
    for case in list_of(field(&doc, "float_battery")) {
        let input = text_of(field(case, "input"));
        let value: f64 = match input {
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            "nan" => f64::NAN,
            _ => input.parse().expect("a recorded float input parses"),
        };
        assert_eq!(
            json_float_text(value),
            text_of(field(case, "answer")),
            "float {input}"
        );
        checked += 1;
    }
    assert_eq!(checked, 29, "the float battery stands, saw {checked}");
}

#[test]
fn conformance_sha_battery() {
    let doc = recordings();
    let mut by_text: BTreeMap<String, String> = BTreeMap::new();
    for case in list_of(field(&doc, "sha_battery")) {
        by_text.insert(
            text_of(field(case, "text")).to_string(),
            text_of(field(case, "answer")).to_string(),
        );
        assert_eq!(
            sha256_hex(text_of(field(case, "text")).as_bytes()),
            text_of(field(case, "answer"))
        );
    }
    assert_eq!(by_text.len(), 5, "the sha battery stands");
}

#[test]
fn conformance_splitlines_battery() {
    let doc = recordings();
    let mut checked = 0;
    for case in list_of(field(&doc, "splitlines_battery")) {
        assert_eq!(
            splitlines(text_of(field(case, "text"))),
            text_list(field(case, "answer")),
            "text {:?}",
            text_of(field(case, "text"))
        );
        checked += 1;
    }
    assert_eq!(checked, 19, "the splitlines battery stands, saw {checked}");
}

#[test]
fn conformance_render_battery() {
    let doc = recordings();
    let mut checked = 0;
    for case in list_of(field(&doc, "render_battery")) {
        let value = parse_json(text_of(field(case, "input"))).expect("a value decodes");
        assert_eq!(render_compact(&value), text_of(field(case, "compact")));
        assert_eq!(render_line(&value), text_of(field(case, "line")));
        assert_eq!(render_printed(&value), text_of(field(case, "printed")));
        checked += 1;
    }
    assert_eq!(checked, 11, "the render battery stands, saw {checked}");
}

#[test]
fn conformance_record_limit_cases() {
    // The nesting witnesses recurse to the stated bound, so
    // the section runs on a thread with a deep stack.
    let doc = recordings();
    let cases = list_of(field(&doc, "record_limit_cases")).clone();
    assert_eq!(cases.len(), 6, "the limit cases stand, saw {}", cases.len());
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let mut checked = 0;
            for case in &cases {
                let name = text_of(field(case, "name"));
                let port = field(case, "port");
                let outcome = parse_json_lenient(text_of(field(case, "record_text")))
                    .map_err(|_| "the fingerprint record does not decode".to_string())
                    .and_then(|data| {
                        Fingerprint::from_dict(&data).map_err(|error| error.message().to_string())
                    });
                match (outcome, opt_text(field(port, "error"))) {
                    (Ok(restored), None) => {
                        assert_eq!(
                            restored.digest(),
                            text_of(field(port, "digest")),
                            "case {name} digest"
                        );
                    }
                    (Err(message), Some(expected)) => {
                        assert_eq!(message, expected, "case {name} refusal");
                    }
                    (Ok(_), Some(expected)) => {
                        panic!("case {name} stands in the port, refuses {expected} at the limit")
                    }
                    (Err(message), None) => {
                        panic!("case {name} refuses in the port at the limit. {message}")
                    }
                }
                checked += 1;
            }
            checked
        })
        .expect("the limit thread stands");
    let checked = handle.join().expect("the limit cases run");
    assert_eq!(checked, 6, "the limit cases stand, saw {checked}");
}

#[test]
fn conformance_compare_cases() {
    let doc = recordings();
    let mut checked = 0;
    for case in list_of(field(&doc, "compare_cases")) {
        let name = text_of(field(case, "name"));
        let expected_data =
            parse_json_lenient(text_of(field(case, "expected"))).expect("a record decodes");
        let actual_data =
            parse_json_lenient(text_of(field(case, "actual"))).expect("a record decodes");
        let expected = Fingerprint::from_dict(&expected_data).expect("the expected stands");
        let actual = Fingerprint::from_dict(&actual_data).expect("the actual stands");
        let answer = field(case, "answer");
        match compare_fingerprints(&expected, &actual) {
            Ok(differences) => {
                assert_eq!(
                    opt_text(field(answer, "error")),
                    None,
                    "case {name} compares in the port and not in the spec"
                );
                assert_eq!(
                    differences,
                    text_list(field(answer, "differences")),
                    "case {name} differences"
                );
            }
            Err(error) => {
                assert!(
                    answer.get("crash").is_some(),
                    "case {name} refuses in the port and not in the spec"
                );
                assert_eq!(
                    error.message(),
                    "the fingerprint record does not decode",
                    "case {name} refusal"
                );
            }
        }
        checked += 1;
    }
    assert_eq!(checked, 4, "the compare cases stand, saw {checked}");
}

#[test]
fn conformance_cli_cases() {
    use std::io::Write;
    use std::os::unix::ffi::OsStringExt;

    let doc = recordings();
    let base = base_dir("cli");
    let mut checked = 0;
    for case in list_of(field(&doc, "cli_cases")) {
        let name = text_of(field(case, "name"));
        let root = match case.get("root_name_b64") {
            Some(encoded) if !matches!(encoded, JsonValue::Null) => {
                let parent = base.join(name);
                std::fs::create_dir_all(&parent).expect("a case parent stands");
                let root = parent.join(std::ffi::OsString::from_vec(base64_decode(text_of(
                    encoded,
                ))));
                std::fs::create_dir_all(&root).expect("a case root stands");
                apply_steps(&root, field(case, "steps"));
                root
            }
            _ => build_case(&base, name, field(case, "steps")),
        };
        let log_path = base.join(format!("{name}.log"));
        // The recorded arguments carry {ROOT} and {LOG}
        // placeholders, the log standing beside the root in
        // the recorded form {ROOT}.log. Substitution runs on
        // bytes so a root spelled in bytes that are not valid
        // UTF 8 reaches the binary as it stands.
        let substitute = |part: &str| -> std::ffi::OsString {
            use std::os::unix::ffi::OsStrExt;
            let bytes = part.as_bytes();
            let mut out: Vec<u8> = Vec::new();
            let mut index = 0;
            while index < bytes.len() {
                if bytes[index..].starts_with(b"{ROOT}") {
                    out.extend_from_slice(root.as_os_str().as_bytes());
                    index += "{ROOT}".len();
                } else if bytes[index..].starts_with(b"{LOG}") {
                    out.extend_from_slice(log_path.as_os_str().as_bytes());
                    index += "{LOG}".len();
                } else {
                    out.push(bytes[index]);
                    index += 1;
                }
            }
            std::ffi::OsString::from_vec(out)
        };
        let argv: Vec<std::ffi::OsString> = list_of(field(case, "argv"))
            .iter()
            .map(|part| substitute(text_of(part)))
            .collect();
        let mut child = Command::new(env!("CARGO_BIN_EXE_finaltree"))
            .args(&argv)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the binary runs");
        child
            .stdin
            .as_mut()
            .expect("the input stands")
            .write_all(text_of(field(case, "stdin")).as_bytes())
            .expect("the input is written");
        let output = child.wait_with_output().expect("the binary answers");
        let expect = field(case, "expect");
        let expected_exit = match field(expect, "exit") {
            JsonValue::Int(value) => *value as i32,
            other => panic!("a recorded exit code stands, got {other:?}"),
        };
        assert_eq!(
            output.status.code(),
            Some(expected_exit),
            "case {name} exit code"
        );
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            text_of(field(expect, "stdout")),
            "case {name} stdout"
        );
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        match expect.get("stderr_prefix") {
            Some(prefix) => {
                assert!(
                    stderr.starts_with(text_of(prefix)),
                    "case {name} stderr {stderr:?} opens with the recorded refusal"
                );
            }
            None => {
                assert_eq!(
                    stderr,
                    text_of(field(expect, "stderr")),
                    "case {name} stderr"
                );
            }
        }
        if let Some(log_text) = case.get("log_text") {
            let written = std::fs::read_to_string(&log_path).expect("the log stands");
            assert_eq!(written, text_of(log_text), "case {name} log text");
        }
        checked += 1;
    }
    assert_eq!(checked, 5, "the cli cases stand, saw {checked}");
}
