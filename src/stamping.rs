//! Stamped logs and verification against a candidate tree.
//!
//! Ported from the landed Python in `stamping.py`. A stamped
//! log binds a fingerprint record to the command that ran, its
//! exit code, and its output lines, each output line carrying
//! the fingerprint digest as its prefix, with an evidence
//! digest covering the command line, the exit code line, and
//! the prefixed output lines as written.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::errors::FinaltreeError;
use crate::fingerprint::{
    compute_fingerprint_tokens, display_path, py_set_eq, sort_pairs, sort_values, DirtyFile,
    Fingerprint, GeneratedFile, TrackedFile, UntrackedFile,
};
use crate::intparse::{parse_python_int, py_is_space};
use crate::jsonvalue::{parse_json, parse_json_lenient, JsonValue};
use crate::render::{py_str, render_line, render_line_unsorted};
use crate::sha256::sha256_hex;

pub const STAMP_FORMAT: &str = "finaltree-stamped-log/1";

#[derive(Debug, Clone, PartialEq)]
pub struct StampedLog {
    pub fingerprint: Fingerprint,
    pub command: Vec<JsonValue>,
    /// The exit code the integer reading read, in plain decimal
    /// text, the spelling the evidence recomputation joins on.
    pub exit_code_text: String,
    pub output_lines: Vec<String>,
    pub evidence_digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationResult {
    pub matched: bool,
    pub differences: Vec<String>,
    pub expected: Fingerprint,
    pub actual: Fingerprint,
}

/// Split a text on the boundary set the Python splitlines
/// reading splits on, the line feed, the carriage return with
/// an optional line feed after it, the line and paragraph
/// separators, and the control boundaries between them.
pub fn splitlines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\n' => {
                lines.push(std::mem::take(&mut current));
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                lines.push(std::mem::take(&mut current));
            }
            '\u{000B}' | '\u{000C}' | '\u{001C}' | '\u{001D}' | '\u{001E}' | '\u{0085}'
            | '\u{2028}' | '\u{2029}' => {
                lines.push(std::mem::take(&mut current));
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

fn evidence_digest(command_line: &str, exit_code_line: &str, output_lines: &[String]) -> String {
    let mut material = String::from(command_line);
    material.push('\n');
    material.push_str(exit_code_line);
    for line in output_lines {
        material.push('\n');
        material.push_str(line);
    }
    sha256_hex(material.as_bytes())
}

/// Render the stamped log text for one gate run. The exit code
/// stands in plain decimal text, as the integer reading spells
/// the process exit code after a run.
pub fn render_stamped_log(
    fingerprint: &Fingerprint,
    command: &[JsonValue],
    exit_code_text: &str,
    output: &str,
) -> String {
    let command_line = format!(
        "command-json {}",
        render_line_unsorted(&JsonValue::List(command.to_vec()))
    );
    let exit_code_line = format!("exit-code {exit_code_text}");
    let digest = fingerprint.digest();
    let output_lines: Vec<String> = splitlines(output)
        .iter()
        .map(|line| format!("{digest} | {line}"))
        .collect();
    let mut lines: Vec<String> = vec![
        STAMP_FORMAT.to_string(),
        format!("fingerprint {digest}"),
        format!("fingerprint-json {}", render_line(&fingerprint.to_dict())),
        command_line.clone(),
        exit_code_line.clone(),
        format!(
            "evidence-digest {}",
            evidence_digest(&command_line, &exit_code_line, &output_lines)
        ),
        "begin-output".to_string(),
    ];
    lines.extend(output_lines);
    lines.push("end-output".to_string());
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn py_strip(text: &str) -> &str {
    text.trim_matches(py_is_space)
}

/// Read a stamped log back, refusing any log that does not parse.
pub fn read_stamped_log(path: &Path) -> Result<StampedLog, FinaltreeError> {
    let bytes = std::fs::read(path).map_err(|_| {
        FinaltreeError::new(format!(
            "the stamped log at {} cannot be read",
            display_path(path)
        ))
    })?;
    let text = String::from_utf8(bytes).map_err(|_| {
        FinaltreeError::new(format!(
            "the stamped log at {} does not decode",
            display_path(path)
        ))
    })?;
    let lines = splitlines(&text);
    if lines.is_empty() || lines[0] != STAMP_FORMAT {
        return Err(FinaltreeError::new(
            "the stamped log carries no finaltree header",
        ));
    }
    let mut recorded_digest: Option<String> = None;
    let mut fingerprint: Option<Fingerprint> = None;
    let mut command: Option<Vec<JsonValue>> = None;
    let mut command_line: Option<String> = None;
    let mut exit_code_text: Option<String> = None;
    let mut exit_code_line: Option<String> = None;
    let mut evidence: Option<String> = None;
    let mut output_lines: Vec<String> = Vec::new();
    let mut raw_output_lines: Vec<String> = Vec::new();
    let mut in_output = false;
    let mut saw_end = false;
    for line in &lines[1..] {
        if saw_end {
            return Err(FinaltreeError::new(
                "the stamped log carries content after its output section",
            ));
        }
        if in_output {
            if line == "end-output" {
                in_output = false;
                saw_end = true;
                continue;
            }
            let prefix = match &recorded_digest {
                Some(digest) => format!("{digest} | "),
                None => {
                    return Err(FinaltreeError::new(
                        "an output line in the stamped log carries no fingerprint prefix",
                    ))
                }
            };
            if !line.starts_with(&prefix) {
                return Err(FinaltreeError::new(
                    "an output line in the stamped log carries no fingerprint prefix",
                ));
            }
            raw_output_lines.push(line.clone());
            output_lines.push(line[prefix.len()..].to_string());
            continue;
        }
        if line == "begin-output" {
            in_output = true;
        } else if let Some(raw) = line.strip_prefix("fingerprint-json ") {
            if fingerprint.is_some() {
                return Err(FinaltreeError::new(
                    "the stamped log carries its fingerprint record twice",
                ));
            }
            let data = parse_json_lenient(raw).map_err(|_| {
                FinaltreeError::new("the fingerprint record in the stamped log does not decode")
            })?;
            fingerprint = Some(Fingerprint::from_dict(&data)?);
        } else if let Some(rest) = line.strip_prefix("fingerprint ") {
            if recorded_digest.is_some() {
                return Err(FinaltreeError::new(
                    "the stamped log carries its fingerprint line twice",
                ));
            }
            recorded_digest = Some(py_strip(rest).to_string());
        } else if line.strip_prefix("command-json ").is_some() {
            if command.is_some() {
                return Err(FinaltreeError::new(
                    "the stamped log carries its command line twice",
                ));
            }
            let raw = &line["command-json ".len()..];
            let parsed = parse_json(raw).map_err(|_| {
                FinaltreeError::new("the command record in the stamped log does not decode")
            })?;
            match parsed {
                JsonValue::List(items) => {
                    command = Some(items);
                    command_line = Some(line.clone());
                }
                _ => {
                    return Err(FinaltreeError::new(
                        "the command record in the stamped log does not decode",
                    ))
                }
            }
        } else if let Some(rest) = line.strip_prefix("exit-code ") {
            if exit_code_text.is_some() {
                return Err(FinaltreeError::new(
                    "the stamped log carries its exit code line twice",
                ));
            }
            match parse_python_int(rest) {
                Some(value) => {
                    exit_code_text = Some(value);
                    exit_code_line = Some(line.clone());
                }
                None => {
                    return Err(FinaltreeError::new(
                        "the exit code in the stamped log does not decode",
                    ))
                }
            }
        } else if let Some(rest) = line.strip_prefix("evidence-digest ") {
            if evidence.is_some() {
                return Err(FinaltreeError::new(
                    "the stamped log carries its evidence line twice",
                ));
            }
            evidence = Some(py_strip(rest).to_string());
        }
    }
    let fingerprint = match fingerprint {
        Some(fingerprint) => fingerprint,
        None => {
            return Err(FinaltreeError::new(
                "the stamped log lacks the fingerprint record a verifier reads",
            ))
        }
    };
    let recorded_digest = match recorded_digest {
        Some(digest) => digest,
        None => {
            return Err(FinaltreeError::new(
                "the stamped log lacks the fingerprint line its header promises",
            ))
        }
    };
    if recorded_digest != fingerprint.digest() {
        return Err(FinaltreeError::new(
            "the stamped log digest line disagrees with its fingerprint record",
        ));
    }
    let command = match command {
        Some(command) => command,
        None => {
            return Err(FinaltreeError::new(
                "the stamped log lacks the command line its header promises",
            ))
        }
    };
    let (exit_code_text, exit_code_line) = match (exit_code_text, exit_code_line) {
        (Some(text), Some(line)) => (text, line),
        _ => {
            return Err(FinaltreeError::new(
                "the stamped log lacks the exit code line its header promises",
            ))
        }
    };
    let evidence = match evidence {
        Some(evidence) => evidence,
        None => {
            return Err(FinaltreeError::new(
                "the stamped log lacks the evidence line its header promises",
            ))
        }
    };
    if !saw_end {
        return Err(FinaltreeError::new(
            "the stamped log output section never closes",
        ));
    }
    let command_line = command_line.expect("a command carries its line");
    if evidence_digest(&command_line, &exit_code_line, &raw_output_lines) != evidence {
        return Err(FinaltreeError::new(
            "the evidence digest in the stamped log disagrees with the \
             command and exit code and output it covers",
        ));
    }
    Ok(StampedLog {
        fingerprint,
        command,
        exit_code_text,
        output_lines,
        evidence_digest: evidence,
    })
}

/// Fingerprint the tree, run the gate command, write the
/// stamped log. The fingerprint is computed before the command
/// runs, from the tree the command will run on. Returns the
/// exit code of the gate command, with a signal death reported
/// as one.
pub fn run_and_stamp(
    root: &Path,
    command: &[String],
    log_path: &Path,
    generated_patterns: &[String],
) -> Result<i32, FinaltreeError> {
    let token_patterns: Vec<Vec<crate::glob::Token>> = generated_patterns
        .iter()
        .map(|pattern| crate::glob::tokens_of_text(pattern))
        .collect();
    run_and_stamp_tokens(root, command, log_path, &token_patterns)
}

/// Fingerprint the tree, run the gate command, write the
/// stamped log, the declared patterns standing as tokens.
pub fn run_and_stamp_tokens(
    root: &Path,
    command: &[String],
    log_path: &Path,
    generated_patterns: &[Vec<crate::glob::Token>],
) -> Result<i32, FinaltreeError> {
    let fingerprint = compute_fingerprint_tokens(root, generated_patterns)?;
    if command.is_empty() {
        // The Python reading raises on an empty command before
        // any run. The port refuses it in the error family.
        return Err(FinaltreeError::new("the gate command is empty"));
    }
    // The command's stdout and stderr share one file, as the
    // Python merged stream stands, so the stamped output is the
    // merged output in arrival order.
    let stamp_file = std::env::temp_dir().join(format!(
        "finaltree-stamp-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|span| span.as_nanos())
            .unwrap_or(0)
    ));
    let file = std::fs::File::create(&stamp_file).map_err(|error| {
        FinaltreeError::new(format!("the gate command could not be run. {error}"))
    })?;
    let stdout = Stdio::from(file.try_clone().map_err(|error| {
        FinaltreeError::new(format!("the gate command could not be run. {error}"))
    })?);
    let stderr = Stdio::from(file);
    let status = Command::new(&command[0])
        .args(&command[1..])
        .current_dir(root)
        .stdin(Stdio::inherit())
        .stdout(stdout)
        .stderr(stderr)
        .status()
        .map_err(|error| {
            FinaltreeError::new(format!("the gate command could not be run. {error}"))
        })?;
    let exit_code = status.code().unwrap_or(1);
    let output_bytes = std::fs::read(&stamp_file).unwrap_or_default();
    let _ = std::fs::remove_file(&stamp_file);
    let output = String::from_utf8_lossy(&output_bytes).into_owned();
    let command_values: Vec<JsonValue> = command
        .iter()
        .map(|part| JsonValue::text(part.clone()))
        .collect();
    let text = render_stamped_log(
        &fingerprint,
        &command_values,
        &exit_code.to_string(),
        &output,
    );
    std::fs::write(log_path, text).map_err(|_| {
        FinaltreeError::new(format!(
            "the stamped log at {} cannot be written",
            display_path(log_path)
        ))
    })?;
    Ok(exit_code)
}

/// Group member entries by their path value, in the order
/// the paths first stand, as the dictionary building of the
/// Python comparison groups them.
fn group_tracked<'a>(entries: &'a [TrackedFile]) -> Vec<(&'a JsonValue, Vec<&'a TrackedFile>)> {
    let mut groups: Vec<(&JsonValue, Vec<&TrackedFile>)> = Vec::new();
    for entry in entries {
        match groups
            .iter_mut()
            .find(|(path, _)| py_set_eq(path, &entry.path))
        {
            Some((_, bucket)) => bucket.push(entry),
            None => groups.push((&entry.path, vec![entry])),
        }
    }
    groups
}

fn group_generated<'a>(entries: &'a [GeneratedFile]) -> Vec<(&'a JsonValue, &'a JsonValue)> {
    let mut groups: Vec<(&JsonValue, &JsonValue)> = Vec::new();
    for entry in entries {
        match groups
            .iter_mut()
            .find(|(path, _)| py_set_eq(path, &entry.path))
        {
            Some((_, blob)) => *blob = &entry.blob,
            None => groups.push((&entry.path, &entry.blob)),
        }
    }
    groups
}

fn group_untracked<'a>(entries: &'a [UntrackedFile]) -> Vec<(&'a JsonValue, &'a JsonValue)> {
    let mut groups: Vec<(&JsonValue, &JsonValue)> = Vec::new();
    for entry in entries {
        match groups
            .iter_mut()
            .find(|(path, _)| py_set_eq(path, &entry.path))
        {
            Some((_, blob)) => *blob = &entry.blob,
            None => groups.push((&entry.path, &entry.blob)),
        }
    }
    groups
}

/// The paths standing on either side, each once, in standing
/// order before the comparison sorts them.
fn union_paths<'a>(
    expected: impl Iterator<Item = &'a JsonValue>,
    actual: impl Iterator<Item = &'a JsonValue>,
) -> Vec<JsonValue> {
    let mut paths: Vec<JsonValue> = Vec::new();
    for path in expected.chain(actual) {
        if !paths.iter().any(|standing| py_set_eq(standing, path)) {
            paths.push(path.clone());
        }
    }
    paths
}

/// The blob a path stands with in a generated or untracked
/// grouping, when the path stands there.
fn blob_lookup<'a>(
    groups: &[(&'a JsonValue, &'a JsonValue)],
    path: &JsonValue,
) -> Option<&'a JsonValue> {
    groups
        .iter()
        .find(|(standing, _)| py_set_eq(standing, path))
        .map(|(_, blob)| *blob)
}

/// Count entries by a key the closure reads, under the
/// equality the Python counter applies.
fn count_by<T: PartialEq>(
    entries: &[&TrackedFile],
    key: impl Fn(&TrackedFile) -> T,
) -> Vec<(T, usize)> {
    let mut counts: Vec<(T, usize)> = Vec::new();
    for entry in entries {
        let item = key(entry);
        match counts.iter_mut().find(|(standing, _)| *standing == item) {
            Some((_, count)) => *count += 1,
            None => counts.push((item, 1)),
        }
    }
    counts
}

/// Name every member that differs between two fingerprints.
///
/// The comparison reads member values as they stand, as the
/// Python comparison does, and its refusal is the decode
/// family of the record reading. A sort the comparison runs
/// over values the Python ordering cannot compare stops that
/// reading uncaught, and the port refuses it here.
pub fn compare_fingerprints(
    expected: &Fingerprint,
    actual: &Fingerprint,
) -> Result<Vec<String>, FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let mut differences = Vec::new();
    if expected.commit != actual.commit {
        differences.push(format!(
            "commit moved from {} to {}",
            py_str(&expected.commit),
            py_str(&actual.commit)
        ));
    }
    let expected_generated = group_generated(&expected.generated);
    let actual_generated = group_generated(&actual.generated);
    let generated_paths = union_paths(
        expected_generated.iter().map(|(path, _)| *path),
        actual_generated.iter().map(|(path, _)| *path),
    );
    let generated_differs = |path: &JsonValue| -> bool {
        let in_paths = generated_paths
            .iter()
            .any(|standing| py_set_eq(standing, path));
        let expected_blob = blob_lookup(&expected_generated, path);
        let actual_blob = blob_lookup(&actual_generated, path);
        in_paths
            && (expected_blob.is_none() != actual_blob.is_none()
                || (expected_blob.is_some()
                    && actual_blob.is_some()
                    && expected_blob != actual_blob))
    };
    let expected_tracked = group_tracked(&expected.tracked);
    let actual_tracked = group_tracked(&actual.tracked);
    let mut tracked_paths = union_paths(
        expected_tracked.iter().map(|(path, _)| *path),
        actual_tracked.iter().map(|(path, _)| *path),
    );
    sort_values(&mut tracked_paths).ok_or_else(decode)?;
    for path in &tracked_paths {
        if generated_differs(path) {
            continue;
        }
        let expected_entries = expected_tracked
            .iter()
            .find(|(standing, _)| py_set_eq(standing, path))
            .map(|(_, bucket)| bucket);
        let actual_entries = actual_tracked
            .iter()
            .find(|(standing, _)| py_set_eq(standing, path))
            .map(|(_, bucket)| bucket);
        match (expected_entries, actual_entries) {
            (Some(_), None) => {
                differences.push(format!("tracked file removed {}", py_str(path)));
            }
            (None, Some(_)) => {
                differences.push(format!("tracked file added {}", py_str(path)));
            }
            (Some(expected_entries), Some(actual_entries)) => {
                let pairs = |entries: &[&TrackedFile]| {
                    count_by(entries, |entry| {
                        (entry.blob.clone(), entry.index_blobs.clone())
                    })
                };
                if pairs(expected_entries) != pairs(actual_entries) {
                    if count_by(expected_entries, |entry| entry.blob.clone())
                        != count_by(actual_entries, |entry| entry.blob.clone())
                    {
                        differences.push(format!("file content changed {}", py_str(path)));
                    }
                    if count_by(expected_entries, |entry| entry.index_blobs.clone())
                        != count_by(actual_entries, |entry| entry.index_blobs.clone())
                    {
                        differences.push(format!("index content changed {}", py_str(path)));
                    }
                }
            }
            (None, None) => unreachable!(),
        }
    }
    let expected_untracked = group_untracked(&expected.untracked);
    let actual_untracked = group_untracked(&actual.untracked);
    let mut shared: Vec<JsonValue> = Vec::new();
    for (path, _) in &expected_untracked {
        if actual_untracked
            .iter()
            .any(|(standing, _)| py_set_eq(standing, path))
        {
            shared.push((*path).clone());
        }
    }
    sort_values(&mut shared).ok_or_else(decode)?;
    for path in &shared {
        let expected_blob = blob_lookup(&expected_untracked, path);
        let actual_blob = blob_lookup(&actual_untracked, path);
        if expected_blob != actual_blob {
            differences.push(format!("untracked content changed {}", py_str(path)));
        }
    }
    let dirty_pairs = |entries: &[DirtyFile]| {
        entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.kind.clone()))
            .collect::<Vec<(JsonValue, JsonValue)>>()
    };
    let expected_dirty = dirty_pairs(&expected.dirty);
    let actual_dirty = dirty_pairs(&actual.dirty);
    let pair_stands = |pairs: &[(JsonValue, JsonValue)], pair: &(JsonValue, JsonValue)| {
        pairs
            .iter()
            .any(|(p, k)| py_set_eq(p, &pair.0) && py_set_eq(k, &pair.1))
    };
    let mut cleaned: Vec<(JsonValue, JsonValue)> = expected_dirty
        .iter()
        .filter(|pair| !pair_stands(&actual_dirty, pair))
        .cloned()
        .collect();
    let mut added: Vec<(JsonValue, JsonValue)> = actual_dirty
        .iter()
        .filter(|pair| !pair_stands(&expected_dirty, pair))
        .cloned()
        .collect();
    sort_pairs(&mut cleaned).ok_or_else(decode)?;
    sort_pairs(&mut added).ok_or_else(decode)?;
    for (path, kind) in &cleaned {
        differences.push(format!(
            "dirty file cleaned {} ({})",
            py_str(path),
            py_str(kind)
        ));
    }
    for (path, kind) in &added {
        differences.push(format!(
            "dirty file added {} ({})",
            py_str(path),
            py_str(kind)
        ));
    }
    let mut generated_sorted = generated_paths.clone();
    sort_values(&mut generated_sorted).ok_or_else(decode)?;
    for path in &generated_sorted {
        let expected_blob = blob_lookup(&expected_generated, path);
        let actual_blob = blob_lookup(&actual_generated, path);
        match (expected_blob, actual_blob) {
            (Some(_), None) => {
                differences.push(format!("generated artifact removed {}", py_str(path)));
            }
            (None, Some(_)) => {
                differences.push(format!("generated artifact added {}", py_str(path)));
            }
            (Some(expected_blob), Some(actual_blob)) => {
                if expected_blob != actual_blob {
                    differences.push(format!("generated artifact differs {}", py_str(path)));
                }
            }
            (None, None) => unreachable!(),
        }
    }
    Ok(differences)
}

/// Verify a stamped log against a candidate tree.
pub fn verify_stamped_log(
    log_path: &Path,
    candidate_root: &Path,
    generated_patterns: Option<&[String]>,
) -> Result<VerificationResult, FinaltreeError> {
    let token_patterns: Option<Vec<Vec<crate::glob::Token>>> = generated_patterns.map(|patterns| {
        patterns
            .iter()
            .map(|pattern| crate::glob::tokens_of_text(pattern))
            .collect()
    });
    verify_stamped_log_tokens(log_path, candidate_root, token_patterns.as_deref())
}

/// Verify a stamped log against a candidate tree, the
/// declared patterns standing as tokens where they stand.
pub fn verify_stamped_log_tokens(
    log_path: &Path,
    candidate_root: &Path,
    generated_patterns: Option<&[Vec<crate::glob::Token>]>,
) -> Result<VerificationResult, FinaltreeError> {
    let stamped = read_stamped_log(log_path)?;
    let patterns: Vec<Vec<crate::glob::Token>> = match generated_patterns {
        Some(patterns) => patterns.to_vec(),
        None => {
            // The patterns of the record stand as its values
            // stand. A pattern that is not a text stops the
            // Python matching uncaught, and the port refuses
            // it in the decode family of the record reading.
            let mut texts = Vec::new();
            for pattern in &stamped.fingerprint.generated_patterns {
                match pattern {
                    JsonValue::Text(text) => texts.push(crate::glob::tokens_of_text(text)),
                    _ => {
                        return Err(FinaltreeError::new(
                            "the fingerprint record does not decode",
                        ))
                    }
                }
            }
            texts
        }
    };
    let actual = compute_fingerprint_tokens(candidate_root, &patterns)?;
    let differences = compare_fingerprints(&stamped.fingerprint, &actual)?;
    Ok(VerificationResult {
        matched: differences.is_empty(),
        differences,
        expected: stamped.fingerprint,
        actual,
    })
}
