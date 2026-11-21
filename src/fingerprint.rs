//! Fingerprinting of the exact tree a gate ran on.
//!
//! Ported from the landed Python in `fingerprint.py`. The
//! fingerprint holds the HEAD commit, the tracked files with the
//! git blob hash of their working tree content and the blobs the
//! index holds for them, the dirty files named by kind, the
//! untracked files with the hash of their content, and the
//! generated artifacts matching declared patterns. The members
//! are retained beside the digest so a mismatch can be named
//! and not just detected.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::os::unix::ffi::OsStringExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use crate::errors::FinaltreeError;
use crate::glob;
use crate::jsonvalue::JsonValue;
use crate::render::render_compact;
use crate::sha256::sha256_hex;

pub const FORMAT: &str = "finaltree-fingerprint/1";

/// The member values of a fingerprint stand as the values
/// they are, the texts a computed fingerprint builds and
/// whatever values a record as written carries, because the
/// record reading of the landed Python takes them as they
/// stand and carries them standing. A computed fingerprint
/// holds texts in every field and a null in a blob that does
/// not stand.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackedFile {
    pub path: JsonValue,
    pub blob: JsonValue,
    pub index_blobs: Vec<JsonValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirtyFile {
    pub path: JsonValue,
    pub kind: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedFile {
    pub path: JsonValue,
    pub blob: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UntrackedFile {
    pub path: JsonValue,
    pub blob: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IgnoredFile {
    pub path: JsonValue,
    pub blob: JsonValue,
}

/// One regular file or symbolic link standing under a
/// .git name below the root, its path and the hash of its
/// content under the name blob, a symbolic link standing
/// as the git blob of its target text.
#[derive(Debug, Clone, PartialEq)]
pub struct DotGitFile {
    pub path: JsonValue,
    pub blob: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialFile {
    pub path: JsonValue,
    pub kind: JsonValue,
}

/// The display spelling of a path as the Python path reading
/// spells it in messages, with redundant separators collapsed,
/// a trailing separator dropped, and a leading current
/// directory marker dropped.
pub fn display_path(path: &Path) -> String {
    let raw = path.to_string_lossy().into_owned();
    if raw.is_empty() {
        return ".".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    let mut rooted = false;
    for component in path.components() {
        match component {
            Component::RootDir => rooted = true,
            Component::CurDir => {}
            Component::ParentDir => parts.push("..".to_string()),
            Component::Normal(name) => parts.push(name.to_string_lossy().into_owned()),
            Component::Prefix(_) => {}
        }
    }
    if parts.is_empty() {
        return if rooted {
            "/".to_string()
        } else {
            ".".to_string()
        };
    }
    let joined = parts.join("/");
    if rooted {
        format!("/{joined}")
    } else {
        joined
    }
}

/// The text of a value a computed fingerprint built, which
/// holds texts in every path it carries. Any other value
/// reads as the empty text here, and no computed value is one.
fn as_text(value: &JsonValue) -> &str {
    match value {
        JsonValue::Text(text) => text,
        _ => "",
    }
}

/// The record value of one pattern word standing as its
/// tokens, a text where every token is a character and the
/// surrogate text of its units where a byte stands among
/// them, which is the spelling the fingerprint record carries
/// for the word as given.
pub fn pattern_record(tokens: &[glob::Token]) -> JsonValue {
    if tokens
        .iter()
        .all(|token| matches!(token, glob::Token::Char(_)))
    {
        let text: String = tokens
            .iter()
            .map(|token| match token {
                glob::Token::Char(ch) => *ch,
                glob::Token::Byte(_) => unreachable!("a byte token stands apart"),
            })
            .collect();
        return JsonValue::Text(text);
    }
    let mut units: Vec<u16> = Vec::new();
    for token in tokens {
        match token {
            glob::Token::Char(ch) => {
                let mut holding = [0u16; 2];
                units.extend_from_slice(ch.encode_utf16(&mut holding));
            }
            glob::Token::Byte(byte) => units.push(0xDC00 + u16::from(*byte)),
        }
    }
    JsonValue::SurrogateText(units)
}

/// The document form of a blob that may not stand.
fn blob_value(blob: Option<String>) -> JsonValue {
    match blob {
        Some(blob) => JsonValue::text(blob),
        None => JsonValue::Null,
    }
}

fn tracked_entry(entry: &TrackedFile) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("blob", entry.blob.clone()),
        ("index_blobs", JsonValue::List(entry.index_blobs.clone())),
    ])
}

fn dirty_entry(entry: &DirtyFile) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("kind", entry.kind.clone()),
    ])
}

fn generated_entry(entry: &GeneratedFile) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("blob", entry.blob.clone()),
    ])
}

fn untracked_entry(entry: &UntrackedFile) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("blob", entry.blob.clone()),
    ])
}

fn ignored_entry(entry: &IgnoredFile) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("blob", entry.blob.clone()),
    ])
}

fn dot_git_entry(entry: &DotGitFile) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("blob", entry.blob.clone()),
    ])
}

/// The member record both the digest and the emitted dict
/// shape, with member lists sorted under the Python ordering
/// of the values as they stand. A computed fingerprint holds
/// texts throughout, which always order. A restored record
/// was ordered once already by the record reading before it
/// stood, so the ordering here is the same reading repeated.
fn members_payload(fingerprint: &Fingerprint) -> JsonValue {
    let mut tracked: Vec<JsonValue> = fingerprint.tracked.iter().map(tracked_entry).collect();
    let mut dirty: Vec<JsonValue> = fingerprint.dirty.iter().map(dirty_entry).collect();
    let mut generated: Vec<JsonValue> = fingerprint.generated.iter().map(generated_entry).collect();
    let mut untracked: Vec<JsonValue> = fingerprint.untracked.iter().map(untracked_entry).collect();
    let _ = sort_raw(&mut tracked, &["path"]);
    let _ = sort_raw(&mut dirty, &["path", "kind"]);
    let _ = sort_raw(&mut generated, &["path"]);
    let _ = sort_raw(&mut untracked, &["path"]);
    let mut pairs = vec![
        ("format", JsonValue::text(FORMAT)),
        ("commit", fingerprint.commit.clone()),
        ("tracked", JsonValue::List(tracked)),
        ("dirty", JsonValue::List(dirty)),
        ("generated", JsonValue::List(generated)),
        ("untracked", JsonValue::List(untracked)),
    ];
    if !fingerprint.submodules.is_empty() {
        let mut submodules: Vec<JsonValue> =
            fingerprint.submodules.iter().map(submodule_entry).collect();
        let _ = sort_raw(&mut submodules, &["path"]);
        pairs.push(("submodules", JsonValue::List(submodules)));
    }
    if !fingerprint.embedded.is_empty() {
        let mut embedded: Vec<JsonValue> =
            fingerprint.embedded.iter().map(embedded_entry).collect();
        let _ = sort_raw(&mut embedded, &["path"]);
        pairs.push(("embedded", JsonValue::List(embedded)));
    }
    if !fingerprint.modes.is_empty() {
        let mut modes: Vec<JsonValue> = fingerprint.modes.iter().map(mode_entry).collect();
        let _ = sort_raw(&mut modes, &["path"]);
        pairs.push(("modes", JsonValue::List(modes)));
    }
    if !fingerprint.ignored.is_empty() {
        let mut ignored: Vec<JsonValue> = fingerprint.ignored.iter().map(ignored_entry).collect();
        let _ = sort_raw(&mut ignored, &["path"]);
        pairs.push(("ignored", JsonValue::List(ignored)));
    }
    if !fingerprint.empty_dirs.is_empty() {
        let mut empty_dirs = fingerprint.empty_dirs.clone();
        empty_dirs.sort_by(|a, b| as_text(a).cmp(as_text(b)));
        pairs.push(("empty_dirs", JsonValue::List(empty_dirs)));
    }
    if !fingerprint.special.is_empty() {
        let mut special: Vec<JsonValue> = fingerprint.special.iter().map(special_entry).collect();
        let _ = sort_raw(&mut special, &["path"]);
        pairs.push(("special", JsonValue::List(special)));
    }
    if !fingerprint.dir_modes.is_empty() {
        let mut dir_modes: Vec<JsonValue> =
            fingerprint.dir_modes.iter().map(dir_mode_entry).collect();
        let _ = sort_raw(&mut dir_modes, &["path"]);
        pairs.push(("dir_modes", JsonValue::List(dir_modes)));
    }
    if !fingerprint.file_modes.is_empty() {
        let mut file_modes: Vec<JsonValue> =
            fingerprint.file_modes.iter().map(file_mode_entry).collect();
        let _ = sort_raw(&mut file_modes, &["path"]);
        pairs.push(("file_modes", JsonValue::List(file_modes)));
    }
    if !fingerprint.dot_git.is_empty() {
        let mut dot_git: Vec<JsonValue> = fingerprint.dot_git.iter().map(dot_git_entry).collect();
        let _ = sort_raw(&mut dot_git, &["path"]);
        pairs.push(("dot_git", JsonValue::List(dot_git)));
    }
    JsonValue::object(pairs)
}

fn special_entry(entry: &SpecialFile) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("kind", entry.kind.clone()),
    ])
}

fn mode_entry(entry: &ModeEntry) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("mode", entry.mode.clone()),
    ])
}

fn dir_mode_entry(entry: &DirModeEntry) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("mode", entry.mode.clone()),
    ])
}

fn file_mode_entry(entry: &FileModeEntry) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("mode", entry.mode.clone()),
    ])
}

fn submodule_entry(entry: &SubmoduleEntry) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("index_commit", entry.index_commit.clone()),
        ("head", entry.head.clone()),
        ("digest", entry.digest.clone()),
    ])
}

fn embedded_entry(entry: &EmbeddedEntry) -> JsonValue {
    JsonValue::object(vec![
        ("path", entry.path.clone()),
        ("head", entry.head.clone()),
        ("digest", entry.digest.clone()),
    ])
}

/// Equality as the Python set building applies it, where two
/// references to the one object compare equal before any value
/// comparison runs. The Python document reading hands back one
/// shared object for every NaN token of a document, verified
/// against the reading itself, so two NaN values standing in
/// one record are the one object and do stand as the same
/// member. Numbers of different kinds compare by their value,
/// as the written equality of the values gives it.
pub(crate) fn py_set_eq(a: &JsonValue, b: &JsonValue) -> bool {
    match (a, b) {
        (JsonValue::Float(x), JsonValue::Float(y)) if x.is_nan() && y.is_nan() => true,
        _ => a == b,
    }
}

/// The FIX-7 duplicate member refusal over the member values
/// as written, compared under the equality the Python set
/// building applies. A value the Python set building cannot
/// hash, a list or an object standing as a path or a kind,
/// stops that reading uncaught, and the port refuses it in
/// the decode family.
fn refuse_duplicate_raw(
    path_members: [&[JsonValue]; 10],
    dirty: &[JsonValue],
) -> Result<(), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    for members in path_members {
        let mut seen: Vec<&JsonValue> = Vec::new();
        for entry in members {
            let path = entry.get("path").ok_or_else(decode)?;
            if matches!(path, JsonValue::List(_) | JsonValue::Object(_)) {
                return Err(decode());
            }
            if seen.iter().any(|standing| py_set_eq(standing, path)) {
                return Err(FinaltreeError::new(
                    "the fingerprint record carries the same member twice",
                ));
            }
            seen.push(path);
        }
    }
    let mut pairs: Vec<(&JsonValue, &JsonValue)> = Vec::new();
    for entry in dirty {
        let path = entry.get("path").ok_or_else(decode)?;
        let kind = entry.get("kind").ok_or_else(decode)?;
        if matches!(path, JsonValue::List(_) | JsonValue::Object(_))
            || matches!(kind, JsonValue::List(_) | JsonValue::Object(_))
        {
            return Err(decode());
        }
        if pairs
            .iter()
            .any(|(p, k)| py_set_eq(p, path) && py_set_eq(k, kind))
        {
            return Err(FinaltreeError::new(
                "the fingerprint record carries the same member twice",
            ));
        }
        pairs.push((path, kind));
    }
    Ok(())
}

/// Read a sequence under the Python tuple reading, a list
/// standing as its items, a text as its characters, an object
/// as its keys in standing order. Anything else is a value the
/// tuple reading stops on.
fn raw_sequence(value: &JsonValue) -> Option<Vec<JsonValue>> {
    match value {
        JsonValue::List(items) => Some(items.clone()),
        JsonValue::Text(text) => Some(
            text.chars()
                .map(|ch| JsonValue::Text(ch.to_string()))
                .collect(),
        ),
        JsonValue::Object(pairs) => Some(
            pairs
                .iter()
                .map(|(key, _)| JsonValue::Text(key.clone()))
                .collect(),
        ),
        JsonValue::SurrogateText(_) => value.code_points().map(|points| {
            points
                .iter()
                .map(|point| match char::from_u32(*point) {
                    Some(ch) => JsonValue::Text(ch.to_string()),
                    None => JsonValue::SurrogateText(vec![*point as u16]),
                })
                .collect()
        }),
        _ => None,
    }
}

/// Read one member list of the record into payload entry
/// shape, with values as written. A tracked list also carries
/// the index blobs under the sequence reading, standing
/// absent as an empty list.
fn read_raw_members(
    data: &JsonValue,
    key: &str,
    required: bool,
    tracked_shape: bool,
) -> Result<Vec<JsonValue>, FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get(key) {
        None => {
            if required {
                return Err(decode());
            }
            return Ok(Vec::new());
        }
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        if tracked_shape {
            let blob = entry.get("blob").cloned().ok_or_else(decode)?;
            let index_blobs = match entry.get("index_blobs") {
                None => Vec::new(),
                Some(value) => raw_sequence(value).ok_or_else(decode)?,
            };
            out.push(JsonValue::object(vec![
                ("path", path),
                ("blob", blob),
                ("index_blobs", JsonValue::List(index_blobs)),
            ]));
        } else if key == "dirty" {
            let kind = entry.get("kind").cloned().ok_or_else(decode)?;
            out.push(JsonValue::object(vec![("path", path), ("kind", kind)]));
        } else {
            let blob = entry.get("blob").cloned().ok_or_else(decode)?;
            out.push(JsonValue::object(vec![("path", path), ("blob", blob)]));
        }
    }
    Ok(out)
}

/// Compare two integer spellings as the Python integer
/// comparison does, sign then magnitude over digit text.
fn int_text_cmp(a_negative: bool, a_digits: &str, b_negative: bool, b_digits: &str) -> Ordering {
    let a_digits = a_digits.trim_start_matches('0');
    let b_digits = b_digits.trim_start_matches('0');
    let a_negative = a_negative && !a_digits.is_empty();
    let b_negative = b_negative && !b_digits.is_empty();
    match (a_negative, b_negative) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (false, false) => a_digits
            .len()
            .cmp(&b_digits.len())
            .then_with(|| a_digits.cmp(b_digits)),
        (true, true) => b_digits
            .len()
            .cmp(&a_digits.len())
            .then_with(|| b_digits.cmp(a_digits)),
    }
}

/// The sign and digit text of an integer valued document.
fn int_text_of(value: &JsonValue) -> Option<(bool, String)> {
    match value {
        JsonValue::Bool(flag) => Some((false, if *flag { "1" } else { "0" }.to_string())),
        JsonValue::Int(integer) => {
            let text = integer.to_string();
            match text.strip_prefix('-') {
                Some(digits) => Some((true, digits.to_string())),
                None => Some((false, text)),
            }
        }
        JsonValue::BigInt(text) => match text.strip_prefix('-') {
            Some(digits) => Some((true, digits.to_string())),
            None => Some((false, text.clone())),
        },
        _ => None,
    }
}

/// Whether a document stands as a number in the Python
/// ordering, an integer of either size, a float, or a bool.
fn is_number_value(value: &JsonValue) -> bool {
    matches!(
        value,
        JsonValue::Int(_) | JsonValue::BigInt(_) | JsonValue::Float(_) | JsonValue::Bool(_)
    )
}

/// The Python comparison of two numeric documents, exact
/// across the integer and float kinds. A comparison against
/// a NaN answers false in the Python ordering in both
/// directions, so a sort reads the NaN as standing wherever
/// it stands, and this reading answers equal for it against
/// any number. A NaN compared against a value of another
/// kind stops the Python ordering uncaught, as any cross
/// kind comparison does.
fn py_num_cmp(a: &JsonValue, b: &JsonValue) -> Option<Ordering> {
    let a_float = match a {
        JsonValue::Float(value) => Some(*value),
        _ => None,
    };
    let b_float = match b {
        JsonValue::Float(value) => Some(*value),
        _ => None,
    };
    let a_nan = matches!(a, JsonValue::Float(value) if value.is_nan());
    let b_nan = matches!(b, JsonValue::Float(value) if value.is_nan());
    if a_nan || b_nan {
        if is_number_value(a) && is_number_value(b) {
            return Some(Ordering::Equal);
        }
        return None;
    }
    match (a_float, b_float) {
        (Some(x), Some(y)) => x.partial_cmp(&y),
        (Some(float), None) => Some(float_int_cmp(float, b, true)?),
        (None, Some(float)) => Some(float_int_cmp(float, a, false)?.reverse()),
        (None, None) => {
            let (a_neg, a_digits) = int_text_of(a)?;
            let (b_neg, b_digits) = int_text_of(b)?;
            Some(int_text_cmp(a_neg, &a_digits, b_neg, &b_digits))
        }
    }
}

/// Compare a float against an integer valued document. The
/// float stands first in the reading when `float_first` holds.
fn float_int_cmp(float: f64, other: &JsonValue, _float_first: bool) -> Option<Ordering> {
    let (neg, digits) = int_text_of(other)?;
    if float.is_infinite() {
        return Some(if float > 0.0 {
            Ordering::Greater
        } else {
            Ordering::Less
        });
    }
    if float.fract() == 0.0 {
        // An integral float carries its exact integer value.
        let text = format!("{float:.0}");
        let (f_neg, f_digits) = match text.strip_prefix('-') {
            Some(rest) => (true, rest.to_string()),
            None => (false, text),
        };
        return Some(int_text_cmp(f_neg, &f_digits, neg, &digits));
    }
    // A fractional float stands strictly between its floor and
    // the integer after it, and a fractional float is small
    // enough that its floor fits in a signed 64 bit integer.
    // The comparison of the integer against the floor decides,
    // reversed, because the float stands first in this reading.
    let floor = float.floor() as i64;
    let ordering = int_text_cmp(neg, &digits, floor < 0, &floor.unsigned_abs().to_string());
    Some(match ordering {
        // The integer stands at or below the floor, so the
        // float stands above it.
        Ordering::Less | Ordering::Equal => Ordering::Greater,
        Ordering::Greater => Ordering::Less,
    })
}

/// The Python comparison of two documents standing as sort
/// keys, texts by code point, numbers exactly, lists item by
/// item. Nothing comes back for a pair the Python ordering
/// cannot compare, where that reading stops uncaught.
pub(crate) fn py_value_cmp(a: &JsonValue, b: &JsonValue) -> Option<Ordering> {
    match (a, b) {
        (JsonValue::List(x), JsonValue::List(y)) => {
            for (left, right) in x.iter().zip(y.iter()) {
                match py_value_cmp(left, right) {
                    Some(Ordering::Equal) => continue,
                    other => return other,
                }
            }
            Some(x.len().cmp(&y.len()))
        }
        _ => match (a.code_points(), b.code_points()) {
            (Some(x), Some(y)) => Some(x.cmp(&y)),
            _ => py_num_cmp(a, b),
        },
    }
}

/// Sort raw member entries by the named fields under the
/// Python ordering, in place. Nothing comes back when a pair
/// the ordering cannot compare decides the order.
fn sort_raw(entries: &mut [JsonValue], fields: &[&str]) -> Option<()> {
    let mut incomparable = false;
    entries.sort_by(|a, b| {
        for field in fields {
            let left = a.get(field).expect("a payload entry carries its fields");
            let right = b.get(field).expect("a payload entry carries its fields");
            match py_value_cmp(left, right) {
                Some(Ordering::Equal) => continue,
                Some(ordering) => return ordering,
                None => {
                    incomparable = true;
                    return Ordering::Equal;
                }
            }
        }
        Ordering::Equal
    });
    if incomparable {
        None
    } else {
        Some(())
    }
}

/// Sort documents under the Python ordering of their
/// values. Nothing comes back when a pair the ordering
/// cannot compare decides the order, where the Python
/// reading stops uncaught.
pub(crate) fn sort_values(values: &mut Vec<JsonValue>) -> Option<()> {
    let mut incomparable = false;
    values.sort_by(|a, b| match py_value_cmp(a, b) {
        Some(ordering) => ordering,
        None => {
            incomparable = true;
            Ordering::Equal
        }
    });
    if incomparable {
        None
    } else {
        Some(())
    }
}

/// Sort pairs under the Python ordering, first member then
/// second, as the tuple ordering of the dirty reading runs.
/// Nothing comes back on the same condition as sort_values.
pub(crate) fn sort_pairs(pairs: &mut Vec<(JsonValue, JsonValue)>) -> Option<()> {
    let mut incomparable = false;
    pairs.sort_by(|a, b| {
        match py_value_cmp(&a.0, &b.0) {
            Some(Ordering::Equal) => {}
            Some(ordering) => return ordering,
            None => {
                incomparable = true;
                return Ordering::Equal;
            }
        }
        match py_value_cmp(&a.1, &b.1) {
            Some(ordering) => ordering,
            None => {
                incomparable = true;
                Ordering::Equal
            }
        }
    });
    if incomparable {
        None
    } else {
        Some(())
    }
}

/// The members payload over the values as written, the record
/// the digest of a record read back is computed over.
fn raw_payload(
    commit: &JsonValue,
    tracked: &[JsonValue],
    dirty: &[JsonValue],
    generated: &[JsonValue],
    untracked: &[JsonValue],
    submodules: Option<&[JsonValue]>,
    modes: Option<&[JsonValue]>,
    ignored: Option<&[JsonValue]>,
    empty_dirs: Option<&[JsonValue]>,
    special: Option<&[JsonValue]>,
    dir_modes: Option<&[JsonValue]>,
    file_modes: Option<&[JsonValue]>,
    dot_git: Option<&[JsonValue]>,
) -> Option<JsonValue> {
    let mut tracked = tracked.to_vec();
    sort_raw(&mut tracked, &["path"])?;
    let mut dirty = dirty.to_vec();
    sort_raw(&mut dirty, &["path", "kind"])?;
    let mut generated = generated.to_vec();
    sort_raw(&mut generated, &["path"])?;
    let mut untracked = untracked.to_vec();
    sort_raw(&mut untracked, &["path"])?;
    let mut pairs = vec![
        ("format", JsonValue::text(FORMAT)),
        ("commit", commit.clone()),
        ("tracked", JsonValue::List(tracked)),
        ("dirty", JsonValue::List(dirty)),
        ("generated", JsonValue::List(generated)),
        ("untracked", JsonValue::List(untracked)),
    ];
    if let Some(submodules) = submodules {
        let mut submodules = submodules.to_vec();
        sort_raw(&mut submodules, &["path"])?;
        pairs.push(("submodules", JsonValue::List(submodules)));
    }
    if let Some(modes) = modes {
        let mut modes = modes.to_vec();
        sort_raw(&mut modes, &["path"])?;
        pairs.push(("modes", JsonValue::List(modes)));
    }
    if let Some(ignored) = ignored {
        let mut ignored = ignored.to_vec();
        sort_raw(&mut ignored, &["path"])?;
        pairs.push(("ignored", JsonValue::List(ignored)));
    }
    if let Some(empty_dirs) = empty_dirs {
        let mut empty_dirs = empty_dirs.to_vec();
        empty_dirs.sort_by(|a, b| as_text(a).cmp(as_text(b)));
        pairs.push(("empty_dirs", JsonValue::List(empty_dirs)));
    }
    if let Some(special) = special {
        let mut special = special.to_vec();
        sort_raw(&mut special, &["path"])?;
        pairs.push(("special", JsonValue::List(special)));
    }
    if let Some(dir_modes) = dir_modes {
        let mut dir_modes = dir_modes.to_vec();
        sort_raw(&mut dir_modes, &["path"])?;
        pairs.push(("dir_modes", JsonValue::List(dir_modes)));
    }
    if let Some(file_modes) = file_modes {
        let mut file_modes = file_modes.to_vec();
        sort_raw(&mut file_modes, &["path"])?;
        pairs.push(("file_modes", JsonValue::List(file_modes)));
    }
    if let Some(dot_git) = dot_git {
        let mut dot_git = dot_git.to_vec();
        sort_raw(&mut dot_git, &["path"])?;
        pairs.push(("dot_git", JsonValue::List(dot_git)));
    }
    Some(JsonValue::object(pairs))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub commit: JsonValue,
    pub tracked: Vec<TrackedFile>,
    pub dirty: Vec<DirtyFile>,
    pub generated: Vec<GeneratedFile>,
    pub untracked: Vec<UntrackedFile>,
    pub generated_patterns: Vec<JsonValue>,
    pub submodules: Vec<SubmoduleEntry>,
    pub embedded: Vec<EmbeddedEntry>,
    pub modes: Vec<ModeEntry>,
    pub ignored: Vec<IgnoredFile>,
    pub empty_dirs: Vec<JsonValue>,
    pub special: Vec<SpecialFile>,
    pub dir_modes: Vec<DirModeEntry>,
    pub file_modes: Vec<FileModeEntry>,
    pub dot_git: Vec<DotGitFile>,
}

/// One submodule of the tree, the commit the index records
/// for its gitlink, the commit its checkout stands at, and
/// the digest of its own fingerprint, the head and the
/// digest standing null where the submodule does not stand
/// checked out as its own working tree.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmoduleEntry {
    pub path: JsonValue,
    pub index_commit: JsonValue,
    pub head: JsonValue,
    pub digest: JsonValue,
}

/// One embedded repository of the tree, a directory
/// standing below the root that is itself the top level
/// of a working tree while standing in no gitlink, the
/// commit it stands at and the digest of its own
/// fingerprint, the head and the digest standing null
/// where the repository holds no commit.
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedEntry {
    pub path: JsonValue,
    pub head: JsonValue,
    pub digest: JsonValue,
}

/// One file outside the index whose mode is not the plain
/// standing, its path and its mode in the git tree spelling,
/// 100755 for an executable regular file and 120000 for a
/// symbolic link.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeEntry {
    pub path: JsonValue,
    pub mode: JsonValue,
}

/// One directory whose mode stands outside the plain
/// family, its path and its mode spelled in octal over its
/// permission and special bits, at three digits where those
/// bits fit three and at four where a special bit stands.
#[derive(Debug, Clone, PartialEq)]
pub struct DirModeEntry {
    pub path: JsonValue,
    pub mode: JsonValue,
}

/// One regular file whose permission mode stands outside
/// the plain family for files, its path and its mode
/// spelled in octal over its permission and special bits,
/// at three digits where those bits fit three and at four
/// where a special bit stands.
#[derive(Debug, Clone, PartialEq)]
pub struct FileModeEntry {
    pub path: JsonValue,
    pub mode: JsonValue,
}

impl Fingerprint {
    pub fn digest(&self) -> String {
        let text = render_compact(&members_payload(self));
        sha256_hex(text.as_bytes())
    }

    pub fn to_dict(&self) -> JsonValue {
        let JsonValue::Object(mut pairs) = members_payload(self) else {
            unreachable!("the members payload is an object");
        };
        pairs.push(("digest".to_string(), JsonValue::text(self.digest())));
        pairs.push((
            "generated_patterns".to_string(),
            JsonValue::List(self.generated_patterns.clone()),
        ));
        JsonValue::Object(pairs)
    }

    pub fn from_dict(data: &JsonValue) -> Result<Fingerprint, FinaltreeError> {
        let decode = || FinaltreeError::new("the fingerprint record does not decode");
        if !data.is_object() {
            return Err(decode());
        }
        // Construction, in the landed order. Member values stand
        // as written through this phase, as the Python building
        // of the member dataclasses takes them, and only the
        // structural failures refuse. The raw members are shaped
        // as the payload entries the digest is computed over.
        let raw_tracked = read_raw_members(data, "tracked", true, true)?;
        let raw_dirty = read_raw_members(data, "dirty", true, false)?;
        let raw_generated = read_raw_members(data, "generated", true, false)?;
        let raw_untracked = read_raw_members(data, "untracked", false, false)?;
        let (raw_submodules, submodules_stand) = read_raw_submodules(data)?;
        let (raw_modes, modes_stand) = read_raw_modes(data)?;
        let (raw_ignored, ignored_stand) = read_raw_ignored(data)?;
        let (raw_empty_dirs, empty_dirs_stand) = read_raw_empty_dirs(data)?;
        let (raw_special, special_stand) = read_raw_special(data)?;
        let (raw_dir_modes, dir_modes_stand) = read_raw_dir_modes(data)?;
        let (raw_file_modes, file_modes_stand) = read_raw_file_modes(data)?;
        let (raw_dot_git, dot_git_stand) = read_raw_dot_git(data)?;
        let commit_raw = data.get("commit").cloned().ok_or_else(decode)?;
        let patterns_raw: Vec<JsonValue> = match data.get("generated_patterns") {
            None => Vec::new(),
            Some(value) => raw_sequence(value).ok_or_else(decode)?,
        };
        refuse_duplicate_raw(
            [
                &raw_tracked,
                &raw_generated,
                &raw_untracked,
                &raw_submodules,
                &raw_modes,
                &raw_ignored,
                &raw_special,
                &raw_dir_modes,
                &raw_file_modes,
                &raw_dot_git,
            ],
            &raw_dirty,
        )?;
        refuse_duplicate_empty_dirs(&raw_empty_dirs)?;
        let recorded = match data.get("digest") {
            None => {
                return Err(FinaltreeError::new(
                    "the fingerprint record carries no digest",
                ))
            }
            Some(value) => value.clone(),
        };
        // The digest is computed over the payload the written
        // values shape, with member lists sorted under the
        // Python ordering of the values as written. A pair the
        // ordering cannot compare is a record the Python
        // reading stops on uncaught, and the port refuses it in
        // the decode family.
        let payload = raw_payload(
            &commit_raw,
            &raw_tracked,
            &raw_dirty,
            &raw_generated,
            &raw_untracked,
            if submodules_stand {
                Some(&raw_submodules)
            } else {
                None
            },
            if modes_stand { Some(&raw_modes) } else { None },
            if ignored_stand {
                Some(&raw_ignored)
            } else {
                None
            },
            if empty_dirs_stand {
                Some(&raw_empty_dirs)
            } else {
                None
            },
            if special_stand {
                Some(&raw_special)
            } else {
                None
            },
            if dir_modes_stand {
                Some(&raw_dir_modes)
            } else {
                None
            },
            if file_modes_stand {
                Some(&raw_file_modes)
            } else {
                None
            },
            if dot_git_stand {
                Some(&raw_dot_git)
            } else {
                None
            },
        )
        .ok_or_else(decode)?;
        // A lone surrogate standing in a value the digest
        // covers stops the Python digest computation when the
        // payload is encoded, with the standing words for a
        // string that cannot stand in the serialization.
        if commit_raw.contains_surrogate()
            || [
                &raw_tracked,
                &raw_dirty,
                &raw_generated,
                &raw_untracked,
                &raw_submodules,
                &raw_modes,
                &raw_ignored,
                &raw_empty_dirs,
                &raw_special,
                &raw_dir_modes,
                &raw_file_modes,
                &raw_dot_git,
            ]
            .iter()
            .any(|members| members.iter().any(|entry| entry.contains_surrogate()))
        {
            return Err(FinaltreeError::new(
                "the fingerprint record holds a path that is not valid UTF-8",
            ));
        }
        let digest = sha256_hex(render_compact(&payload).as_bytes());
        if recorded != JsonValue::Text(digest) {
            return Err(FinaltreeError::new(
                "the recorded digest disagrees with the recorded members",
            ));
        }
        // A lone surrogate standing anywhere the digest does
        // not reach, in the generated patterns or in a key the
        // reading does not name or in a member key beyond the
        // fields the reading takes, is refused at the decode,
        // as the limits of the port state. The Python reading
        // carries such a string standing through an accepted
        // record, and the difference stands at that stated
        // boundary, so the refusal stands last, where the
        // record would otherwise stand accepted.
        if patterns_raw.iter().any(|value| value.contains_surrogate())
            || uncovered_surrogate_stands(data)
        {
            return Err(decode());
        }
        // The record stands with its member values as written,
        // as the Python building of the member dataclasses
        // leaves them standing, and it re-emits them standing.
        Ok(Fingerprint {
            commit: commit_raw,
            tracked: raw_tracked.iter().map(member_tracked).collect(),
            dirty: raw_dirty.iter().map(member_dirty).collect(),
            generated: raw_generated.iter().map(member_generated).collect(),
            untracked: raw_untracked.iter().map(member_untracked).collect(),
            generated_patterns: patterns_raw,
            submodules: raw_submodules.iter().map(member_submodule).collect(),
            embedded: Vec::new(),
            modes: raw_modes.iter().map(member_mode).collect(),
            ignored: raw_ignored.iter().map(member_ignored).collect(),
            empty_dirs: raw_empty_dirs,
            special: raw_special.iter().map(member_special).collect(),
            dir_modes: raw_dir_modes.iter().map(member_dir_mode).collect(),
            file_modes: raw_file_modes.iter().map(member_file_mode).collect(),
            dot_git: raw_dot_git.iter().map(member_dot_git).collect(),
        })
    }
}

/// Whether a lone surrogate stands in a record anywhere the
/// digest does not reach, in a top level value under a key
/// the reading does not name, or in a member value under a
/// key beyond the fields the reading takes.
fn uncovered_surrogate_stands(data: &JsonValue) -> bool {
    let named = [
        "format",
        "commit",
        "tracked",
        "dirty",
        "generated",
        "untracked",
        "generated_patterns",
        "digest",
        "submodules",
        "modes",
        "ignored",
        "empty_dirs",
        "special",
        "dir_modes",
        "file_modes",
        "dot_git",
    ];
    let fields: [(&str, &[&str]); 11] = [
        ("tracked", &["path", "blob", "index_blobs"]),
        ("dirty", &["path", "kind"]),
        ("generated", &["path", "blob"]),
        ("untracked", &["path", "blob"]),
        ("submodules", &["path", "index_commit", "head", "digest"]),
        ("modes", &["path", "mode"]),
        ("ignored", &["path", "blob"]),
        ("special", &["path", "kind"]),
        ("dir_modes", &["path", "mode"]),
        ("file_modes", &["path", "mode"]),
        ("dot_git", &["path", "blob"]),
    ];
    let pairs = match data.as_object() {
        Some(pairs) => pairs,
        None => return false,
    };
    for (key, value) in pairs {
        if !named.contains(&key.as_str()) && value.contains_surrogate() {
            return true;
        }
    }
    for (list_key, field_set) in fields {
        if let Some(JsonValue::List(items)) = data.get(list_key) {
            for entry in items {
                if let Some(entry_pairs) = entry.as_object() {
                    for (key, value) in entry_pairs {
                        if !field_set.contains(&key.as_str()) && value.contains_surrogate() {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Take a tracked member out of its payload entry shape.
fn member_tracked(entry: &JsonValue) -> TrackedFile {
    TrackedFile {
        path: entry.get("path").expect("a payload entry stands").clone(),
        blob: entry.get("blob").expect("a payload entry stands").clone(),
        index_blobs: match entry.get("index_blobs") {
            Some(JsonValue::List(items)) => items.clone(),
            _ => Vec::new(),
        },
    }
}

/// Take a dirty member out of its payload entry shape.
fn member_dirty(entry: &JsonValue) -> DirtyFile {
    DirtyFile {
        path: entry.get("path").expect("a payload entry stands").clone(),
        kind: entry.get("kind").expect("a payload entry stands").clone(),
    }
}

/// Take a generated member out of its payload entry shape.
fn member_generated(entry: &JsonValue) -> GeneratedFile {
    GeneratedFile {
        path: entry.get("path").expect("a payload entry stands").clone(),
        blob: entry.get("blob").expect("a payload entry stands").clone(),
    }
}

/// Take an untracked member out of its payload entry shape.
fn member_submodule(entry: &JsonValue) -> SubmoduleEntry {
    SubmoduleEntry {
        path: entry.get("path").expect("a payload entry stands").clone(),
        index_commit: entry
            .get("index_commit")
            .expect("a payload entry stands")
            .clone(),
        head: entry.get("head").expect("a payload entry stands").clone(),
        digest: entry.get("digest").expect("a payload entry stands").clone(),
    }
}

/// Read the submodules member of a record into payload entry
/// shape, with values as written, and whether the member
/// stands in the record at all. The member stands absent as
/// the empty list, and a record carrying it computes its
/// digest over it exactly as written.
fn read_raw_submodules(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("submodules") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        let index_commit = entry.get("index_commit").cloned().ok_or_else(decode)?;
        let head = entry.get("head").cloned().ok_or_else(decode)?;
        let digest = entry.get("digest").cloned().ok_or_else(decode)?;
        out.push(JsonValue::object(vec![
            ("path", path),
            ("index_commit", index_commit),
            ("head", head),
            ("digest", digest),
        ]));
    }
    Ok((out, true))
}

/// Take a mode member out of its payload entry shape.
fn member_mode(entry: &JsonValue) -> ModeEntry {
    ModeEntry {
        path: entry.get("path").expect("a payload entry stands").clone(),
        mode: entry.get("mode").expect("a payload entry stands").clone(),
    }
}

/// Read the modes member of a record into payload entry
/// shape, with values as written, and whether the member
/// stands in the record at all. The member stands absent as
/// the empty list, a record carrying no such member reading
/// as a tree whose untracked and generated files all stand
/// at the plain standing. An entry naming a mode outside
/// the standings the member names refuses in the decode
/// family, as the slice states.
fn read_raw_modes(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("modes") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        let mode = entry.get("mode").cloned().ok_or_else(decode)?;
        if mode != JsonValue::Text("100755".to_string())
            && mode != JsonValue::Text("120000".to_string())
        {
            return Err(decode());
        }
        out.push(JsonValue::object(vec![("path", path), ("mode", mode)]));
    }
    Ok((out, true))
}

/// Take the ignored member out of a record in its raw
/// payload entry shape, on the standing of the modes member
/// reading. A record carrying no ignored member reads as a
/// tree holding no ignored file (IGN-4).
fn read_raw_ignored(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("ignored") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        let blob = entry.get("blob").cloned().ok_or_else(decode)?;
        out.push(JsonValue::object(vec![("path", path), ("blob", blob)]));
    }
    Ok((out, true))
}

/// Take the dot_git member out of a record in its raw
/// payload entry shape, on the standing of the modes member
/// reading. A record carrying no dot_git member reads as a
/// tree holding no file standing under a .git name below
/// the root (DG-4).
fn read_raw_dot_git(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("dot_git") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        let blob = entry.get("blob").cloned().ok_or_else(decode)?;
        out.push(JsonValue::object(vec![("path", path), ("blob", blob)]));
    }
    Ok((out, true))
}

/// Take the empty_dirs member out of a record in its raw
/// payload entry shape. The member stands as a list of path
/// strings, and an entry that does not decode as a path
/// string refuses in the decode family (ED-4). A record
/// carrying no empty_dirs member reads as a tree holding no
/// empty directory.
fn read_raw_empty_dirs(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("empty_dirs") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !matches!(entry, JsonValue::Text(_)) {
            return Err(decode());
        }
        out.push(entry.clone());
    }
    Ok((out, true))
}

/// Take a special member out of its payload entry shape.
fn member_special(entry: &JsonValue) -> SpecialFile {
    SpecialFile {
        path: entry.get("path").expect("a payload entry stands").clone(),
        kind: entry.get("kind").expect("a payload entry stands").clone(),
    }
}

/// Read the special member of a record into payload entry
/// shape, with values as written, and whether the member
/// stands in the record at all. The member stands absent as
/// the empty list, a record carrying no such member reading
/// as a tree holding no special file (SF-4). An entry that
/// does not decode as an object carrying a path and a kind
/// refuses in the decode family, as the slice states.
fn read_raw_special(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("special") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        let kind = entry.get("kind").cloned().ok_or_else(decode)?;
        out.push(JsonValue::object(vec![("path", path), ("kind", kind)]));
    }
    Ok((out, true))
}

/// Take a dir_modes member out of its payload entry shape.
fn member_dir_mode(entry: &JsonValue) -> DirModeEntry {
    DirModeEntry {
        path: entry.get("path").expect("a payload entry stands").clone(),
        mode: entry.get("mode").expect("a payload entry stands").clone(),
    }
}

/// Read the dir_modes member of a record into payload entry
/// shape, with values as written, and whether the member
/// stands in the record at all. The member stands absent as
/// the empty list, a record carrying no such member reading
/// as a tree whose directories all stand inside the plain
/// family (DM-4). An entry that does not decode as an object
/// carrying a path and a mode refuses in the decode family,
/// and a mode standing inside the plain family or spelled
/// otherwise than DM-2 spells it refuses in the decode
/// family with it, the computation never writing one.
fn read_raw_dir_modes(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("dir_modes") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        let mode = entry.get("mode").cloned().ok_or_else(decode)?;
        let JsonValue::Text(spelling) = &mode else {
            return Err(decode());
        };
        let Some(bits) = parse_dir_mode(spelling) else {
            return Err(decode());
        };
        if plain_dir_mode(bits) || spell_dir_mode(bits) != *spelling {
            return Err(decode());
        }
        out.push(JsonValue::object(vec![("path", path), ("mode", mode)]));
    }
    Ok((out, true))
}

/// The plain family of file modes, the standings the
/// common umasks 022 and 002 and 027 and 077 and 007 supply
/// from the creation standings, 0666 where no executable
/// bit stands set and 0777 where one does (FP-1). A file
/// standing at one of these modes stands in no member, so
/// two checkouts of one tree under different common umasks
/// stamp as one. A mode carrying a special bit stands
/// outside the family whatever its remaining bits stand at,
/// no umask supplying one.
const PLAIN_FILE_MODES: [u32; 10] = [
    0o600, 0o640, 0o644, 0o660, 0o664, 0o700, 0o750, 0o755, 0o770, 0o775,
];

fn plain_file_mode(bits: u32) -> bool {
    PLAIN_FILE_MODES.contains(&bits)
}

/// Take a file_modes member out of its payload entry shape.
fn member_file_mode(entry: &JsonValue) -> FileModeEntry {
    FileModeEntry {
        path: entry.get("path").expect("a payload entry stands").clone(),
        mode: entry.get("mode").expect("a payload entry stands").clone(),
    }
}

/// Read the file_modes member of a record into payload
/// entry shape, with values as written, and whether the
/// member stands in the record at all. The member stands
/// absent as the empty list, a record carrying no such
/// member reading as a tree whose files all stand inside
/// the plain family (FP-4). An entry that does not decode
/// as an object carrying a path and a mode refuses in the
/// decode family, and a mode standing inside the plain
/// family or spelled otherwise than the octal spelling
/// refuses in the decode family with it, the computation
/// never writing one.
fn read_raw_file_modes(data: &JsonValue) -> Result<(Vec<JsonValue>, bool), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    let items = match data.get("file_modes") {
        None => return Ok((Vec::new(), false)),
        Some(JsonValue::List(items)) if items.is_empty() => return Err(decode()),
        Some(JsonValue::List(items)) => items,
        Some(_) => return Err(decode()),
    };
    let mut out = Vec::new();
    for entry in items {
        if !entry.is_object() {
            return Err(decode());
        }
        let path = entry.get("path").cloned().ok_or_else(decode)?;
        let mode = entry.get("mode").cloned().ok_or_else(decode)?;
        let JsonValue::Text(spelling) = &mode else {
            return Err(decode());
        };
        let Some(bits) = parse_dir_mode(spelling) else {
            return Err(decode());
        };
        if plain_file_mode(bits) || spell_dir_mode(bits) != *spelling {
            return Err(decode());
        }
        out.push(JsonValue::object(vec![("path", path), ("mode", mode)]));
    }
    Ok((out, true))
}

/// The FIX-7 duplicate member refusal over the empty_dirs
/// member, whose entries stand as the paths themselves,
/// compared under the equality the Python set building
/// applies (ED-4).
fn refuse_duplicate_empty_dirs(empty_dirs: &[JsonValue]) -> Result<(), FinaltreeError> {
    let mut seen: Vec<&JsonValue> = Vec::new();
    for entry in empty_dirs {
        if seen.iter().any(|standing| py_set_eq(standing, entry)) {
            return Err(FinaltreeError::new(
                "the fingerprint record carries the same member twice",
            ));
        }
        seen.push(entry);
    }
    Ok(())
}

fn member_ignored(entry: &JsonValue) -> IgnoredFile {
    IgnoredFile {
        path: entry.get("path").expect("a payload entry stands").clone(),
        blob: entry.get("blob").expect("a payload entry stands").clone(),
    }
}

fn member_dot_git(entry: &JsonValue) -> DotGitFile {
    DotGitFile {
        path: entry.get("path").expect("a payload entry stands").clone(),
        blob: entry.get("blob").expect("a payload entry stands").clone(),
    }
}

fn member_untracked(entry: &JsonValue) -> UntrackedFile {
    UntrackedFile {
        path: entry.get("path").expect("a payload entry stands").clone(),
        blob: entry.get("blob").expect("a payload entry stands").clone(),
    }
}

fn git_output(root: &Path, args: &[&str]) -> Result<Vec<u8>, FinaltreeError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .stdin(Stdio::null())
        .output();
    match output {
        Ok(output) if output.status.success() => Ok(output.stdout),
        Ok(output) => {
            let reason = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(FinaltreeError::new(format!(
                "git {} failed. {}",
                args.join(" "),
                reason
            )))
        }
        Err(error) => Err(FinaltreeError::new(format!(
            "git {} failed. {}",
            args.join(" "),
            error
        ))),
    }
}

fn git_text(root: &Path, args: &[&str]) -> Result<String, FinaltreeError> {
    let bytes = git_output(root, args)?;
    // The Python reading decodes with surrogateescape, so a path
    // git spells in other bytes round trips to the filesystem.
    // The texts read here are a commit hash and a top level path,
    // and the lossy reading serves both in every tree whose top
    // level stands in valid UTF-8, the only trees the recorded
    // surface covers.
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn require_utf8_path(relative: &[u8]) -> Result<String, FinaltreeError> {
    String::from_utf8(relative.to_vec()).map_err(|_| {
        FinaltreeError::new(
            "the tree holds a path git spells with bytes that are not \
             valid UTF-8, so the path cannot stand in a fingerprint",
        )
    })
}

fn blob_hash(root: &Path, relative: &str, input: Option<&[u8]>) -> Result<String, FinaltreeError> {
    let mut command = Command::new("git");
    command.arg("-C").arg(root).arg("hash-object");
    match input {
        Some(_) => {
            command.arg("--stdin");
        }
        None => {
            command.arg("--").arg(relative);
        }
    }
    let output = command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(bytes) = input {
                use std::io::Write;
                let mut stdin = child.stdin.take().expect("piped stdin");
                let _ = stdin.write_all(bytes);
                drop(stdin);
            }
            child.wait_with_output()
        });
    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        }
        Ok(output) => {
            let reason = String::from_utf8_lossy(&output.stderr).trim().to_string();
            Err(FinaltreeError::new(format!(
                "git hash-object failed for {}. {}",
                relative, reason
            )))
        }
        Err(error) => Err(FinaltreeError::new(format!(
            "git hash-object failed for {}. {}",
            relative, error
        ))),
    }
}

/// Whether a failed reading of one entry found nothing
/// standing at its path, the one failure kind that keeps
/// the standing negative answer at every reading of a
/// single entry (EG-2 to EG-4). Every other kind refuses.
fn nothing_standing(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

/// The refusal of EG-2 for a reading of one entry that
/// failed in a kind other than the nothing standing kind.
/// The entry stands named by its relative path, as the
/// directory refusal of RF-2 names its directory.
fn entry_refusal(relative: &str) -> FinaltreeError {
    FinaltreeError::new(format!("the entry at {relative} cannot be read"))
}

/// The blob of what stands at a path, with a link read as itself.
fn content_blob(root: &Path, relative: &str) -> Result<Option<String>, FinaltreeError> {
    let full = root.join(relative);
    let link = match std::fs::symlink_metadata(&full) {
        Ok(meta) => meta.file_type().is_symlink(),
        Err(error) if nothing_standing(&error) => false,
        Err(_) => return Err(entry_refusal(relative)),
    };
    if link {
        let target = match std::fs::read_link(&full) {
            Ok(target) => target,
            Err(error) if nothing_standing(&error) => return Ok(None),
            Err(_) => return Err(entry_refusal(relative)),
        };
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            let bytes = target.as_os_str().as_bytes();
            return blob_hash(root, relative, Some(bytes)).map(Some);
        }
        #[cfg(not(unix))]
        {
            return blob_hash(root, relative, Some(target.to_string_lossy().as_bytes())).map(Some);
        }
    }
    let is_file = match std::fs::metadata(&full) {
        Ok(meta) => meta.is_file(),
        Err(error) if nothing_standing(&error) => false,
        Err(_) => return Err(entry_refusal(relative)),
    };
    if is_file {
        return blob_hash(root, relative, None).map(Some);
    }
    Ok(None)
}

fn generated_files(
    root: &Path,
    patterns: &[Vec<glob::Token>],
    embedded: &std::collections::BTreeSet<String>,
) -> Result<Vec<GeneratedFile>, FinaltreeError> {
    if patterns.is_empty() {
        return Ok(Vec::new());
    }
    let compiled: Vec<glob::Pattern> = patterns
        .iter()
        .map(|tokens| glob::translate_tokens(tokens))
        .collect();
    if compiled.iter().any(|pattern| pattern.refused()) {
        // The expression reading refuses the translation, and
        // the landed Python surfaces that as an uncaught error
        // out of the fingerprint reading. The port refuses in
        // the error family so the call fails on both sides.
        return Err(FinaltreeError::new(
            "a generated pattern does not translate",
        ));
    }
    let mut found = Vec::new();
    let boundary: std::collections::BTreeSet<PathBuf> = embedded
        .iter()
        .map(|relative| root.join(relative))
        .collect();
    walk_dir(root, root, &compiled, &boundary, &mut found)?;
    found.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(found)
}
/// The refusal of RF-2 for a directory whose listing
/// failed. The directory stands named by its relative
/// path, in the byte form the gathers build their
/// entries in and read through the standing UTF-8
/// reading, and the root call, whose stripped form
/// stands empty, names the tree by its display form
/// as the standing root refusals name it.
fn directory_refusal(root: &Path, dir: &Path) -> FinaltreeError {
    let stripped = dir.strip_prefix(root).unwrap_or(dir);
    if stripped.as_os_str().is_empty() {
        return FinaltreeError::new(format!("the tree at {} cannot be read", display_path(root)));
    }
    #[cfg(unix)]
    let relative_bytes = {
        use std::os::unix::ffi::OsStrExt;
        stripped.as_os_str().as_bytes().to_vec()
    };
    #[cfg(not(unix))]
    let relative_bytes = stripped.to_string_lossy().into_owned().into_bytes();
    match require_utf8_path(&relative_bytes) {
        Ok(relative) => FinaltreeError::new(format!("the directory at {relative} cannot be read")),
        Err(error) => error,
    }
}

fn walk_dir(
    root: &Path,
    dir: &Path,
    compiled: &[glob::Pattern],
    boundary: &std::collections::BTreeSet<PathBuf>,
    found: &mut Vec<GeneratedFile>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Err(directory_refusal(root, dir)),
    };
    // Entries are classified as the os.walk reading classifies
    // them. A link naming a folder stands among the folder
    // names, which the walk never follows and never matches as
    // a file, and only a folder standing as itself is descended.
    // A link naming a file, and a link naming nothing, stands
    // among the file names.
    let mut names: Vec<(Vec<u8>, PathBuf, bool, bool)> = Vec::new();
    for entry in entries.filter_map(|entry| entry.ok()) {
        let name = entry.file_name();
        #[cfg(unix)]
        let name_bytes = {
            use std::os::unix::ffi::OsStrExt;
            name.as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let name_bytes = name.to_string_lossy().into_owned().into_bytes();
        let is_link = entry
            .file_type()
            .map(|kind| kind.is_symlink())
            .unwrap_or(false);
        let follows_dir = entry.path().is_dir();
        names.push((name_bytes, entry.path(), follows_dir, is_link));
    }
    // Directory names are visited in sorted order and the dot
    // git directory is pruned at the root alone, as the os.walk
    // reading orders them. The descent takes only the folders
    // standing as themselves, never a link naming a folder.
    let mut dirnames: Vec<&(Vec<u8>, PathBuf, bool, bool)> = names
        .iter()
        .filter(|(_, _, follows_dir, is_link)| *follows_dir && !*is_link)
        .collect();
    dirnames.retain(|(name, _, _, _)| name != b".git" || dir != root);
    dirnames.sort_by(|a, b| a.0.cmp(&b.0));
    let mut filenames: Vec<&(Vec<u8>, PathBuf, bool, bool)> = names
        .iter()
        .filter(|(_, _, follows_dir, _)| !*follows_dir)
        .collect();
    filenames.sort_by(|a, b| a.0.cmp(&b.0));
    for (name, _, _, _) in &filenames {
        #[cfg(unix)]
        let relative_path = {
            use std::os::unix::ffi::OsStringExt;
            PathBuf::from(dir).join(std::ffi::OsString::from_vec(name.clone()))
        };
        #[cfg(not(unix))]
        let relative_path = PathBuf::from(dir).join(String::from_utf8_lossy(name).into_owned());
        #[cfg(unix)]
        let relative_bytes = {
            use std::os::unix::ffi::OsStrExt;
            relative_path
                .strip_prefix(root)
                .unwrap_or(&relative_path)
                .as_os_str()
                .as_bytes()
                .to_vec()
        };
        #[cfg(not(unix))]
        let relative_bytes = relative_path
            .strip_prefix(root)
            .unwrap_or(&relative_path)
            .to_string_lossy()
            .into_owned()
            .into_bytes();
        let relative = require_utf8_path(&relative_bytes)?;
        // A special file standing under a declared pattern is
        // passed by as a directory standing as itself is passed
        // by (SF-3). It has no content to hash and stands in
        // the special member alone.
        if let Ok(meta) = std::fs::symlink_metadata(&relative_path) {
            let kind = meta.file_type();
            if !kind.is_file() && !kind.is_symlink() {
                continue;
            }
        }
        if compiled
            .iter()
            .any(|pattern| glob::matches(pattern, &relative))
        {
            match content_blob(root, &relative)? {
                Some(blob) => found.push(GeneratedFile {
                    path: JsonValue::text(relative),
                    blob: JsonValue::text(blob),
                }),
                None => {
                    return Err(FinaltreeError::new(format!(
                        "the generated artifact at {} cannot be read",
                        relative
                    )));
                }
            }
        }
    }
    for (_, path, _, _) in &dirnames {
        // A directory the detection names as an embedded
        // repository is never descended into, its files
        // binding through its digest under EB-4.
        if boundary.contains(path) {
            continue;
        }
        walk_dir(root, path, compiled, boundary, found)?;
    }
    Ok(())
}

fn index_entries(root: &Path) -> Result<BTreeMap<String, Vec<String>>, FinaltreeError> {
    let listing = git_output(root, &["ls-files", "--stage", "-z"])?;
    let mut entries: BTreeMap<String, Vec<(i64, String)>> = BTreeMap::new();
    for raw in listing.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let (meta, raw_path) = match raw.iter().position(|byte| *byte == b'\t') {
            Some(tab) => (&raw[..tab], &raw[tab + 1..]),
            None => (raw, &[][..]),
        };
        let fields: Vec<&[u8]> = meta.split(|byte| *byte == b' ').collect();
        if fields.len() != 3 {
            return Err(FinaltreeError::new("the index listing does not decode"));
        }
        let stage_text = String::from_utf8_lossy(fields[2]).into_owned();
        let stage: i64 = match crate::intparse::parse_python_int(&stage_text)
            .and_then(|text| text.parse::<i64>().ok())
        {
            Some(stage) => stage,
            None => return Err(FinaltreeError::new("the index listing does not decode")),
        };
        let blob = match std::str::from_utf8(fields[1]) {
            Ok(blob) if blob.is_ascii() => blob.to_string(),
            _ => return Err(FinaltreeError::new("the index listing does not decode")),
        };
        let relative = require_utf8_path(raw_path)?;
        entries.entry(relative).or_default().push((stage, blob));
    }
    let mut out = BTreeMap::new();
    for (relative, mut staged) in entries {
        staged.sort_by_key(|(stage, _)| *stage);
        out.insert(
            relative,
            staged.into_iter().map(|(_, blob)| blob).collect::<Vec<_>>(),
        );
    }
    Ok(out)
}

const UNMERGED_CODES: [(char, char); 7] = [
    ('D', 'D'),
    ('A', 'A'),
    ('A', 'U'),
    ('U', 'A'),
    ('D', 'U'),
    ('U', 'D'),
    ('U', 'U'),
];

/// Compute the fingerprint of a working tree with git only.
/// The gitlinks of a tree, the paths whose stage listing
/// entries carry the submodule mode, each with the commit
/// the index records for it.
fn gitlinks(root: &Path) -> Result<Vec<(String, String)>, FinaltreeError> {
    let listing = git_output(root, &["ls-files", "--stage", "-z"])?;
    let mut out: Vec<(String, String)> = Vec::new();
    for raw in listing.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let (meta, raw_path) = match raw.iter().position(|byte| *byte == b'\t') {
            Some(tab) => (&raw[..tab], &raw[tab + 1..]),
            None => continue,
        };
        let fields: Vec<&[u8]> = meta.split(|byte| *byte == b' ').collect();
        if fields.len() != 3 || fields[0] != b"160000" {
            continue;
        }
        let commit = match std::str::from_utf8(fields[1]) {
            Ok(text) if text.is_ascii() => text.to_string(),
            _ => return Err(FinaltreeError::new("the index listing does not decode")),
        };
        let relative = require_utf8_path(raw_path)?;
        if !out.iter().any(|(standing, _)| *standing == relative) {
            out.push((relative, commit));
        }
    }
    out.sort();
    Ok(out)
}

/// The embedded repositories of a tree, the directories
/// standing below the root that are themselves the top
/// level of a working tree while standing in no gitlink,
/// as the set of their relative path texts (EB-3). The
/// reading walks the tree itself in the form the gathers
/// take, testing every directory standing as itself on
/// the two part test the submodule reading runs for a
/// checkout, a .git entry standing inside the directory
/// and the git reading standing at the directory
/// returning the directory itself as its top level. The
/// walk never names the root itself, never descends into
/// a gitlink path, and never descends into a repository
/// it has named.
fn embedded_repositories(
    root: &Path,
) -> Result<std::collections::BTreeSet<String>, FinaltreeError> {
    let gitlinks: std::collections::BTreeSet<String> = gitlinks(root)?
        .into_iter()
        .map(|(relative, _)| relative)
        .collect();
    let mut out = std::collections::BTreeSet::new();
    gather_embedded(root, root, &gitlinks, &mut out)?;
    Ok(out)
}

fn gather_embedded(
    root: &Path,
    dir: &Path,
    gitlinks: &std::collections::BTreeSet<String>,
    out: &mut std::collections::BTreeSet<String>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Err(directory_refusal(root, dir)),
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name() == ".git" && dir == root {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }
        let full = entry.path();
        let stripped = match full.strip_prefix(root) {
            Ok(stripped) => stripped,
            Err(_) => continue,
        };
        #[cfg(unix)]
        let relative_bytes = {
            use std::os::unix::ffi::OsStrExt;
            stripped.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let relative_bytes = stripped.to_string_lossy().into_owned().into_bytes();
        let relative = require_utf8_path(&relative_bytes)?;
        // A gitlink path is neither named nor descended
        // into, its checkout standing in the submodules
        // member alone.
        if gitlinks.contains(&relative) {
            continue;
        }
        if std::fs::symlink_metadata(full.join(".git")).is_ok() {
            let top = git_output(&full, &["rev-parse", "--show-toplevel"]).ok();
            let itself = match top {
                Some(bytes) => {
                    let top_path =
                        PathBuf::from(std::ffi::OsString::from_vec(bytes.trim_ascii().to_vec()));
                    std::fs::canonicalize(&top_path).ok() == std::fs::canonicalize(&full).ok()
                }
                None => false,
            };
            if itself {
                out.insert(relative);
                continue;
            }
        }
        gather_embedded(root, &full, gitlinks, out)?;
    }
    Ok(())
}

/// The submodule entries of a tree, one per gitlink, under
/// SUB-3. A submodule stands checked out where the directory
/// at its path is itself the top level of a working tree,
/// and its own fingerprint is computed standing there with
/// no declared patterns, so the digest of the tree binds
/// the state of every checkout below it.
fn submodule_entries(root: &Path) -> Result<Vec<SubmoduleEntry>, FinaltreeError> {
    let mut out = Vec::new();
    for (relative, index_commit) in gitlinks(root)? {
        let full = root.join(&relative);
        let dot_git = std::fs::symlink_metadata(full.join(".git")).is_ok();
        let checked_out = if dot_git {
            let top = git_output(&full, &["rev-parse", "--show-toplevel"]).ok();
            match top {
                Some(bytes) => {
                    let top_path =
                        PathBuf::from(std::ffi::OsString::from_vec(bytes.trim_ascii().to_vec()));
                    std::fs::canonicalize(&top_path).ok() == std::fs::canonicalize(&full).ok()
                }
                None => false,
            }
        } else {
            false
        };
        let (head, digest) = if checked_out {
            let head = git_text(&full, &["rev-parse", "HEAD"])?.trim().to_string();
            let own = compute_fingerprint_tokens(&full, &[])?;
            (JsonValue::text(head), JsonValue::text(own.digest()))
        } else {
            (JsonValue::Null, JsonValue::Null)
        };
        out.push(SubmoduleEntry {
            path: JsonValue::text(relative),
            index_commit: JsonValue::text(index_commit),
            head,
            digest,
        });
    }
    Ok(out)
}

/// The embedded entries of a tree, one per repository
/// the detection reading names, under EB-2. The head is
/// the commit the git reading returns standing at the
/// path, and the digest is the repository its own
/// fingerprint computed standing there with no declared
/// patterns, so the digest of the tree binds the state
/// of every repository below it. A repository holding
/// no commit stands with its head null and its digest
/// null, on the form SUB-2 settles for a head that does
/// not stand, a failed head reading standing as that
/// standing.
fn embedded_entries(
    root: &Path,
    embedded: &std::collections::BTreeSet<String>,
) -> Result<Vec<EmbeddedEntry>, FinaltreeError> {
    let mut out = Vec::new();
    for relative in embedded {
        let full = root.join(relative);
        let (head, digest) = match git_text(&full, &["rev-parse", "HEAD"]) {
            Ok(text) => {
                let own = compute_fingerprint_tokens(&full, &[])?;
                (
                    JsonValue::text(text.trim().to_string()),
                    JsonValue::text(own.digest()),
                )
            }
            Err(_) => (JsonValue::Null, JsonValue::Null),
        };
        out.push(EmbeddedEntry {
            path: JsonValue::text(relative.clone()),
            head,
            digest,
        });
    }
    Ok(out)
}

pub fn compute_fingerprint(
    root: &Path,
    generated_patterns: &[String],
) -> Result<Fingerprint, FinaltreeError> {
    let token_patterns: Vec<Vec<glob::Token>> = generated_patterns
        .iter()
        .map(|pattern| glob::tokens_of_text(pattern))
        .collect();
    compute_fingerprint_tokens(root, &token_patterns)
}

/// Compute the fingerprint of a working tree with git only,
/// the declared patterns standing as tokens, so a pattern
/// word carrying a byte no UTF 8 sequence claims is matched
/// in its bytes and carried in the record in its surrogate
/// spelling.
pub fn compute_fingerprint_tokens(
    root: &Path,
    generated_patterns: &[Vec<glob::Token>],
) -> Result<Fingerprint, FinaltreeError> {
    if !root.is_dir() {
        return Err(FinaltreeError::new(format!(
            "the tree at {} does not stand",
            display_path(root)
        )));
    }
    let inside = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .stdin(Stdio::null())
        .output();
    let inside_ok = match &inside {
        Ok(output) => output.status.success() && output.stdout.trim_ascii() == b"true",
        Err(_) => false,
    };
    if !inside_ok {
        return Err(FinaltreeError::new(format!(
            "the tree at {} is not a git working tree",
            display_path(root)
        )));
    }
    // The top level is read as the bytes git prints, so a
    // root spelled with bytes that are not valid UTF 8
    // compares as it stands. A lossy reading would spell the
    // top level with replacement characters and refuse a
    // tree that stands at its top level.
    let top_bytes = git_output(root, &["rev-parse", "--show-toplevel"])?;
    let top_bytes = top_bytes.trim_ascii();
    let top_path = PathBuf::from(std::ffi::OsString::from_vec(top_bytes.to_vec()));
    let resolved_top = std::fs::canonicalize(&top_path).ok();
    let resolved_root = std::fs::canonicalize(root).ok();
    if resolved_top != resolved_root {
        return Err(FinaltreeError::new(format!(
            "the tree at {} is not the top level of its working tree, which stands at {}",
            display_path(root),
            top_path.display()
        )));
    }
    let commit = match git_text(root, &["rev-parse", "HEAD"]) {
        Ok(text) => text.trim().to_string(),
        Err(_) => {
            return Err(FinaltreeError::new(
                "the tree holds no commit, so there is no HEAD to fingerprint",
            ))
        }
    };
    let patterns: Vec<Vec<glob::Token>> = generated_patterns.to_vec();
    let index = index_entries(root)?;
    let mut tracked = Vec::new();
    for (relative, index_blobs) in &index {
        tracked.push(TrackedFile {
            path: JsonValue::text(relative.clone()),
            blob: blob_value(content_blob(root, relative)?),
            index_blobs: index_blobs
                .iter()
                .map(|blob| JsonValue::text(blob.clone()))
                .collect(),
        });
    }
    let embedded = embedded_repositories(root)?;
    let generated = generated_files(root, &patterns, &embedded)?;
    let generated_paths: std::collections::BTreeSet<&str> =
        generated.iter().map(|entry| as_text(&entry.path)).collect();
    let mut dirty: std::collections::BTreeSet<(String, String)> = std::collections::BTreeSet::new();
    let mut untracked: Vec<UntrackedFile> = Vec::new();
    let status = git_output(
        root,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--no-renames",
            "--untracked-files=all",
        ],
    )?;
    for raw in status.split(|byte| *byte == 0) {
        if raw.len() < 2 {
            continue;
        }
        let code_x = raw[0] as char;
        let code_y = raw[1] as char;
        let raw_path: &[u8] = if raw.len() > 3 { &raw[3..] } else { &[] };
        let relative = require_utf8_path(raw_path)?;
        if code_x == '?' && code_y == '?' {
            // The whole directory entry of a repository the
            // detection names stands in the embedded member
            // alone, no untracked or dirty entry for it (EB-4).
            if embedded.contains(relative.trim_end_matches('/')) {
                continue;
            }
            if !generated_paths.contains(relative.as_str()) {
                dirty.insert((relative.clone(), "untracked".to_string()));
                untracked.push(UntrackedFile {
                    path: JsonValue::text(relative.clone()),
                    blob: blob_value(content_blob(root, &relative)?),
                });
            }
            continue;
        }
        if UNMERGED_CODES.contains(&(code_x, code_y)) {
            dirty.insert((relative.clone(), "conflicted".to_string()));
            continue;
        }
        if code_x != ' ' && code_x != '?' {
            dirty.insert((relative.clone(), "staged".to_string()));
        }
        if code_y != ' ' && code_y != '?' {
            dirty.insert((relative.clone(), "modified".to_string()));
        }
    }
    untracked.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    let ignored = ignored_files(root, &generated_paths)?;
    let modes = mode_entries(root, &generated, &untracked, &ignored)?;
    let empty_dirs = empty_directories(root, &embedded)?;
    let special = special_files(root, &embedded)?;
    let dir_modes = directory_modes(root, &embedded)?;
    let file_modes = file_permissions(root, &embedded)?;
    let dot_git = dot_git_files(root, &embedded)?;
    Ok(Fingerprint {
        commit: JsonValue::text(commit),
        tracked,
        dirty: dirty
            .into_iter()
            .map(|(path, kind)| DirtyFile {
                path: JsonValue::text(path),
                kind: JsonValue::text(kind),
            })
            .collect(),
        generated,
        untracked,
        generated_patterns: patterns
            .iter()
            .map(|tokens| pattern_record(tokens))
            .collect(),
        submodules: submodule_entries(root)?,
        embedded: embedded_entries(root, &embedded)?,
        modes,
        ignored,
        empty_dirs,
        special,
        dir_modes,
        file_modes,
        dot_git,
    })
}

/// The empty directories of a tree, every directory standing
/// under the root whose subtree holds no entry that is not
/// itself a directory, as relative path texts sorted by path
/// (ED-2 and ED-3). The reading is the filesystem walk of the
/// tree itself, the git reading naming no directory standing
/// empty. The walk never enters the .git folder standing
/// at the root and never enters the interior of a submodule's
/// checkout, a directory standing at a gitlink path standing
/// named nowhere in this member, and the ignore rules reach
/// no part of it.
fn empty_directories(
    root: &Path,
    embedded: &std::collections::BTreeSet<String>,
) -> Result<Vec<JsonValue>, FinaltreeError> {
    let gitlinks: std::collections::BTreeSet<String> = gitlinks(root)?
        .into_iter()
        .map(|(relative, _)| relative)
        .collect();
    let mut directories: Vec<String> = Vec::new();
    let mut holds_entry: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    gather_directories(
        root,
        root,
        &gitlinks,
        embedded,
        &mut directories,
        &mut holds_entry,
    )?;
    let mut out: Vec<JsonValue> = directories
        .into_iter()
        .filter(|relative| !holds_entry.contains(relative))
        .map(JsonValue::text)
        .collect();
    out.sort_by(|a, b| as_text(a).cmp(as_text(b)));
    Ok(out)
}

fn gather_directories(
    root: &Path,
    dir: &Path,
    gitlinks: &std::collections::BTreeSet<String>,
    embedded: &std::collections::BTreeSet<String>,
    directories: &mut Vec<String>,
    holds_entry: &mut std::collections::BTreeSet<String>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Err(directory_refusal(root, dir)),
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name() == ".git" && dir == root {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        let full = entry.path();
        let stripped = match full.strip_prefix(root) {
            Ok(stripped) => stripped,
            Err(_) => continue,
        };
        #[cfg(unix)]
        let relative_bytes = {
            use std::os::unix::ffi::OsStrExt;
            stripped.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let relative_bytes = stripped.to_string_lossy().into_owned().into_bytes();
        let relative = require_utf8_path(&relative_bytes)?;
        if file_type.is_dir() {
            // A directory standing as itself. A gitlink path is
            // neither named nor descended into, its interior
            // standing in the submodule's own fingerprint, and
            // an embedded repository stands the same under EB-4.
            if gitlinks.contains(&relative) || embedded.contains(&relative) {
                continue;
            }
            directories.push(relative);
            gather_directories(root, &full, gitlinks, embedded, directories, holds_entry)?;
        } else {
            // An entry that is not a directory standing as
            // itself, a file or a link of any standing. Every
            // directory above it holds an entry beneath it.
            let mut ancestor = Path::new(&relative).parent();
            while let Some(path) = ancestor {
                if path.as_os_str().is_empty() {
                    break;
                }
                holds_entry.insert(path.to_string_lossy().into_owned());
                ancestor = path.parent();
            }
        }
    }
    Ok(())
}

/// The plain family of directory modes, the standings the
/// common umasks 022 and 002 and 027 and 077 and 007 supply
/// from the full standing (DM-1, as the build amendment to
/// this slice widens it). A directory standing at one of
/// these modes stands in no member, so two checkouts of one
/// tree under different common umasks stamp as one.
const PLAIN_DIR_MODES: [u32; 5] = [0o755, 0o775, 0o750, 0o700, 0o770];

fn plain_dir_mode(bits: u32) -> bool {
    PLAIN_DIR_MODES.contains(&bits)
}

/// The octal spelling of a directory mode over its
/// permission and special bits, at three digits where those
/// bits fit three and at four where a special bit stands
/// (DM-2).
fn spell_dir_mode(bits: u32) -> String {
    if bits > 0o777 {
        format!("{bits:o}")
    } else {
        format!("{bits:03o}")
    }
}

/// Read a directory mode spelling back to its bits, refusing
/// any spelling DM-2 does not name.
fn parse_dir_mode(spelling: &str) -> Option<u32> {
    if !(spelling.len() == 3 || spelling.len() == 4) {
        return None;
    }
    if !spelling.bytes().all(|byte| (b'0'..=b'7').contains(&byte)) {
        return None;
    }
    u32::from_str_radix(spelling, 8).ok()
}

/// The directory modes of a tree standing outside the plain
/// family, each with its octal spelling, sorted by path
/// (DM-2 and DM-3). The reading is the filesystem walk of
/// the tree itself in the form the sixth slice's walk
/// takes, the git reading naming no directory mode. The
/// walk never enters the .git folder standing at the root
/// and never enters the interior of a submodule's checkout,
/// a submodule's
/// checkout directory standing in the walk as a directory
/// standing to be read and not entered, and the ignore
/// rules reach no part of it. The reading reads the root
/// itself beside the directories below it (RM-3).
fn directory_modes(
    root: &Path,
    embedded: &std::collections::BTreeSet<String>,
) -> Result<Vec<DirModeEntry>, FinaltreeError> {
    let gitlinks: std::collections::BTreeSet<String> = gitlinks(root)?
        .into_iter()
        .map(|(relative, _)| relative)
        .collect();
    let mut out = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(root) {
            let bits = meta.permissions().mode() & 0o7777;
            if !plain_dir_mode(bits) {
                out.push(DirModeEntry {
                    path: JsonValue::text("."),
                    mode: JsonValue::text(spell_dir_mode(bits)),
                });
            }
        }
    }
    gather_dir_modes(root, root, &gitlinks, embedded, &mut out)?;
    out.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(out)
}

fn gather_dir_modes(
    root: &Path,
    dir: &Path,
    gitlinks: &std::collections::BTreeSet<String>,
    embedded: &std::collections::BTreeSet<String>,
    out: &mut Vec<DirModeEntry>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Err(directory_refusal(root, dir)),
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name() == ".git" && dir == root {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        if !file_type.is_dir() {
            continue;
        }
        let full = entry.path();
        let stripped = match full.strip_prefix(root) {
            Ok(stripped) => stripped,
            Err(_) => continue,
        };
        #[cfg(unix)]
        let relative_bytes = {
            use std::os::unix::ffi::OsStrExt;
            stripped.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let relative_bytes = stripped.to_string_lossy().into_owned().into_bytes();
        let relative = require_utf8_path(&relative_bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = match std::fs::symlink_metadata(&full) {
                Ok(meta) => Some(meta),
                Err(error) if nothing_standing(&error) => None,
                Err(_) => return Err(entry_refusal(&relative)),
            };
            if let Some(meta) = meta {
                let bits = meta.permissions().mode() & 0o7777;
                if !plain_dir_mode(bits) {
                    out.push(DirModeEntry {
                        path: JsonValue::text(relative.clone()),
                        mode: JsonValue::text(spell_dir_mode(bits)),
                    });
                }
            }
        }
        // A gitlink path is never descended into, its interior
        // standing in the submodule's own fingerprint. Its own
        // mode stands recorded above, in the parent's record.
        // An embedded repository stands the same under EB-4,
        // its own mode standing recorded above in the same way.
        if gitlinks.contains(&relative) || embedded.contains(&relative) {
            continue;
        }
        gather_dir_modes(root, &full, gitlinks, embedded, out)?;
    }
    Ok(())
}

/// The file permissions of a tree standing outside the
/// plain family for files, each with its octal spelling,
/// sorted by path (FP-2 and FP-3). The reading is the
/// filesystem walk of the tree itself in the form the
/// sixth slice's walk takes, the git reading naming no
/// file permission beyond the executable standing. The
/// walk never enters the .git folder standing at the root
/// and never enters the interior of a submodule's checkout,
/// and the ignore rules reach no part of it. Regular files
/// only stand in
/// the member, a link standing and a special standing
/// standing outside it under the members that carry them.
fn file_permissions(
    root: &Path,
    embedded: &std::collections::BTreeSet<String>,
) -> Result<Vec<FileModeEntry>, FinaltreeError> {
    let gitlinks: std::collections::BTreeSet<String> = gitlinks(root)?
        .into_iter()
        .map(|(relative, _)| relative)
        .collect();
    let mut out = Vec::new();
    gather_file_modes(root, root, &gitlinks, embedded, &mut out)?;
    out.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(out)
}

fn gather_file_modes(
    root: &Path,
    dir: &Path,
    gitlinks: &std::collections::BTreeSet<String>,
    embedded: &std::collections::BTreeSet<String>,
    out: &mut Vec<FileModeEntry>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Err(directory_refusal(root, dir)),
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name() == ".git" && dir == root {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        let full = entry.path();
        let stripped = match full.strip_prefix(root) {
            Ok(stripped) => stripped,
            Err(_) => continue,
        };
        #[cfg(unix)]
        let relative_bytes = {
            use std::os::unix::ffi::OsStrExt;
            stripped.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let relative_bytes = stripped.to_string_lossy().into_owned().into_bytes();
        let relative = require_utf8_path(&relative_bytes)?;
        if file_type.is_dir() {
            // A directory standing as itself. A gitlink path is
            // never descended into, its interior standing in
            // the submodule's own fingerprint.
            if gitlinks.contains(&relative) || embedded.contains(&relative) {
                continue;
            }
            gather_file_modes(root, &full, gitlinks, embedded, out)?;
            continue;
        }
        if !file_type.is_file() {
            // A link standing or a special standing, each
            // carried by the members that name them.
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = match std::fs::symlink_metadata(&full) {
                Ok(meta) => Some(meta),
                Err(error) if nothing_standing(&error) => None,
                Err(_) => return Err(entry_refusal(&relative)),
            };
            if let Some(meta) = meta {
                let bits = meta.permissions().mode() & 0o7777;
                if !plain_file_mode(bits) {
                    out.push(FileModeEntry {
                        path: JsonValue::text(relative.clone()),
                        mode: JsonValue::text(spell_dir_mode(bits)),
                    });
                }
            }
        }
    }
    Ok(())
}

/// The special files of a tree, every entry standing under
/// the root that is neither a regular file nor a directory
/// nor a symbolic link, each with the word its kind carries,
/// sorted by path (SF-2 and SF-3). The reading is the
/// filesystem walk of the tree itself in the form the sixth
/// slice's walk takes, the git reading naming no such entry.
/// The walk never enters the .git folder standing at the
/// root and never enters the interior of a submodule's
/// checkout, and the ignore rules reach no part of it.
fn special_files(
    root: &Path,
    embedded: &std::collections::BTreeSet<String>,
) -> Result<Vec<SpecialFile>, FinaltreeError> {
    let gitlinks: std::collections::BTreeSet<String> = gitlinks(root)?
        .into_iter()
        .map(|(relative, _)| relative)
        .collect();
    let mut out = Vec::new();
    gather_special(root, root, &gitlinks, embedded, &mut out)?;
    out.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(out)
}

fn gather_special(
    root: &Path,
    dir: &Path,
    gitlinks: &std::collections::BTreeSet<String>,
    embedded: &std::collections::BTreeSet<String>,
    out: &mut Vec<SpecialFile>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Err(directory_refusal(root, dir)),
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name() == ".git" && dir == root {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        let full = entry.path();
        let stripped = match full.strip_prefix(root) {
            Ok(stripped) => stripped,
            Err(_) => continue,
        };
        #[cfg(unix)]
        let relative_bytes = {
            use std::os::unix::ffi::OsStrExt;
            stripped.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let relative_bytes = stripped.to_string_lossy().into_owned().into_bytes();
        let relative = require_utf8_path(&relative_bytes)?;
        if file_type.is_dir() {
            // A directory standing as itself. A gitlink path is
            // never descended into, its interior standing in the
            // submodule's own fingerprint.
            if gitlinks.contains(&relative) || embedded.contains(&relative) {
                continue;
            }
            gather_special(root, &full, gitlinks, embedded, out)?;
            continue;
        }
        if file_type.is_file() || file_type.is_symlink() {
            continue;
        }
        #[cfg(unix)]
        let kind = {
            use std::os::unix::fs::FileTypeExt;
            if file_type.is_fifo() {
                "fifo"
            } else if file_type.is_socket() {
                "socket"
            } else if file_type.is_block_device() {
                "block device"
            } else if file_type.is_char_device() {
                "character device"
            } else {
                continue;
            }
        };
        #[cfg(not(unix))]
        continue;
        #[cfg(unix)]
        out.push(SpecialFile {
            path: JsonValue::text(relative),
            kind: JsonValue::text(kind),
        });
    }
    Ok(())
}

/// The ignored files of a tree, every file the git ignore
/// reading sets aside standing in neither the generated nor
/// the untracked member, each with the hash of its content
/// under the standing blob reading, sorted by path (IGN-3).
/// The reading names every file on its own line, an ignored
/// directory never standing collapsed over the files below
/// it, and the rules it applies are the tree's ignore files
/// and the repository's own exclude file and the reader's
/// global exclude file alike.
fn ignored_files(
    root: &Path,
    generated_paths: &std::collections::BTreeSet<&str>,
) -> Result<Vec<IgnoredFile>, FinaltreeError> {
    let listing = git_output(
        root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?;
    let mut out = Vec::new();
    for raw in listing.split(|byte| *byte == 0) {
        if raw.is_empty() {
            continue;
        }
        let relative = require_utf8_path(raw)?;
        if generated_paths.contains(relative.as_str()) {
            continue;
        }
        out.push(IgnoredFile {
            path: JsonValue::text(relative.clone()),
            blob: blob_value(content_blob(root, &relative)?),
        });
    }
    out.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(out)
}

/// The files standing under a .git name below the root,
/// every regular file and symbolic link whose relative
/// path carries a .git component below its first
/// component, each with the hash of its content under the
/// standing blob reading, sorted by path (DG-2 and DG-3).
/// The reading is the filesystem walk of the tree itself,
/// the git reading naming no such file. The walk never
/// enters the .git folder standing at the root and never
/// enters the interior of a submodule's checkout, and the
/// ignore rules reach no part of it.
fn dot_git_files(
    root: &Path,
    embedded: &std::collections::BTreeSet<String>,
) -> Result<Vec<DotGitFile>, FinaltreeError> {
    let gitlinks: std::collections::BTreeSet<String> = gitlinks(root)?
        .into_iter()
        .map(|(relative, _)| relative)
        .collect();
    let mut out = Vec::new();
    gather_dot_git(root, root, &gitlinks, embedded, &mut out)?;
    out.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(out)
}

fn gather_dot_git(
    root: &Path,
    dir: &Path,
    gitlinks: &std::collections::BTreeSet<String>,
    embedded: &std::collections::BTreeSet<String>,
    out: &mut Vec<DotGitFile>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Err(directory_refusal(root, dir)),
    };
    for entry in entries.filter_map(|entry| entry.ok()) {
        if entry.file_name() == ".git" && dir == root {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(kind) => kind,
            Err(_) => continue,
        };
        let full = entry.path();
        let stripped = match full.strip_prefix(root) {
            Ok(stripped) => stripped,
            Err(_) => continue,
        };
        #[cfg(unix)]
        let relative_bytes = {
            use std::os::unix::ffi::OsStrExt;
            stripped.as_os_str().as_bytes().to_vec()
        };
        #[cfg(not(unix))]
        let relative_bytes = stripped.to_string_lossy().into_owned().into_bytes();
        let relative = require_utf8_path(&relative_bytes)?;
        if file_type.is_dir() {
            // A directory standing as itself, a folder named
            // .git below the root among them, descended into
            // like any directory. A gitlink path is never
            // descended into, its interior standing in the
            // submodule's own fingerprint.
            if gitlinks.contains(&relative) || embedded.contains(&relative) {
                continue;
            }
            gather_dot_git(root, &full, gitlinks, embedded, out)?;
            continue;
        }
        if !file_type.is_file() && !file_type.is_symlink() {
            // A special standing, carried by the member that
            // names it.
            continue;
        }
        let below_root = Path::new(&relative)
            .components()
            .skip(1)
            .any(|component| component.as_os_str() == ".git");
        if !below_root {
            continue;
        }
        out.push(DotGitFile {
            path: JsonValue::text(relative.clone()),
            blob: blob_value(content_blob(root, &relative)?),
        });
    }
    Ok(())
}

/// The mode standing of one file outside the index, in the
/// git tree spelling, standing absent where the file stands
/// at the plain standing or where nothing stands at its
/// path, and refusing under EG-4 where its reading is
/// denied. The link standing is read from the link itself
/// and never through it, under the same reading that
/// gathers the file's blob.
fn mode_standing(root: &Path, relative: &str) -> Result<Option<&'static str>, FinaltreeError> {
    let full = root.join(relative);
    let meta = match std::fs::symlink_metadata(&full) {
        Ok(meta) => meta,
        Err(error) if nothing_standing(&error) => return Ok(None),
        Err(_) => return Err(entry_refusal(relative)),
    };
    if meta.file_type().is_symlink() {
        return Ok(Some("120000"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.is_file() && meta.permissions().mode() & 0o111 != 0 {
            return Ok(Some("100755"));
        }
    }
    Ok(None)
}

/// The mode entries of a tree, one for every untracked or
/// generated file whose mode is not the plain standing,
/// sorted by path, a path standing at most once however
/// many of the fingerprint's lists name it.
fn mode_entries(
    root: &Path,
    generated: &[GeneratedFile],
    untracked: &[UntrackedFile],
    ignored: &[IgnoredFile],
) -> Result<Vec<ModeEntry>, FinaltreeError> {
    let mut seen: Vec<&str> = Vec::new();
    let mut out = Vec::new();
    for path in generated
        .iter()
        .map(|entry| as_text(&entry.path))
        .chain(untracked.iter().map(|entry| as_text(&entry.path)))
        .chain(ignored.iter().map(|entry| as_text(&entry.path)))
    {
        if seen.contains(&path) {
            continue;
        }
        seen.push(path);
        if let Some(mode) = mode_standing(root, path)? {
            out.push(ModeEntry {
                path: JsonValue::text(path),
                mode: JsonValue::text(mode),
            });
        }
    }
    out.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(out)
}
