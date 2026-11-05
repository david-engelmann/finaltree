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
    JsonValue::object(vec![
        ("format", JsonValue::text(FORMAT)),
        ("commit", fingerprint.commit.clone()),
        ("tracked", JsonValue::List(tracked)),
        ("dirty", JsonValue::List(dirty)),
        ("generated", JsonValue::List(generated)),
        ("untracked", JsonValue::List(untracked)),
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
    tracked: &[JsonValue],
    generated: &[JsonValue],
    untracked: &[JsonValue],
    dirty: &[JsonValue],
) -> Result<(), FinaltreeError> {
    let decode = || FinaltreeError::new("the fingerprint record does not decode");
    for members in [tracked, generated, untracked] {
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
) -> Option<JsonValue> {
    let mut tracked = tracked.to_vec();
    sort_raw(&mut tracked, &["path"])?;
    let mut dirty = dirty.to_vec();
    sort_raw(&mut dirty, &["path", "kind"])?;
    let mut generated = generated.to_vec();
    sort_raw(&mut generated, &["path"])?;
    let mut untracked = untracked.to_vec();
    sort_raw(&mut untracked, &["path"])?;
    Some(JsonValue::object(vec![
        ("format", JsonValue::text(FORMAT)),
        ("commit", commit.clone()),
        ("tracked", JsonValue::List(tracked)),
        ("dirty", JsonValue::List(dirty)),
        ("generated", JsonValue::List(generated)),
        ("untracked", JsonValue::List(untracked)),
    ]))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Fingerprint {
    pub commit: JsonValue,
    pub tracked: Vec<TrackedFile>,
    pub dirty: Vec<DirtyFile>,
    pub generated: Vec<GeneratedFile>,
    pub untracked: Vec<UntrackedFile>,
    pub generated_patterns: Vec<JsonValue>,
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
        let commit_raw = data.get("commit").cloned().ok_or_else(decode)?;
        let patterns_raw: Vec<JsonValue> = match data.get("generated_patterns") {
            None => Vec::new(),
            Some(value) => raw_sequence(value).ok_or_else(decode)?,
        };
        refuse_duplicate_raw(&raw_tracked, &raw_generated, &raw_untracked, &raw_dirty)?;
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
        )
        .ok_or_else(decode)?;
        // A lone surrogate standing in a value the digest
        // covers stops the Python digest computation when the
        // payload is encoded, with the standing words for a
        // string that cannot stand in the serialization.
        if commit_raw.contains_surrogate()
            || [&raw_tracked, &raw_dirty, &raw_generated, &raw_untracked]
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
    ];
    let fields: [(&str, &[&str]); 4] = [
        ("tracked", &["path", "blob", "index_blobs"]),
        ("dirty", &["path", "kind"]),
        ("generated", &["path", "blob"]),
        ("untracked", &["path", "blob"]),
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

/// The blob of what stands at a path, with a link read as itself.
fn content_blob(root: &Path, relative: &str) -> Result<Option<String>, FinaltreeError> {
    let full = root.join(relative);
    let link = std::fs::symlink_metadata(&full)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);
    if link {
        let target = std::fs::read_link(&full)
            .map_err(|_| FinaltreeError::new(format!("the path at {relative} cannot be read")))?;
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
    let is_file = std::fs::metadata(&full)
        .map(|meta| meta.is_file())
        .unwrap_or(false);
    if is_file {
        return blob_hash(root, relative, None).map(Some);
    }
    Ok(None)
}

fn generated_files(
    root: &Path,
    patterns: &[Vec<glob::Token>],
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
    walk_dir(root, root, &compiled, &mut found)?;
    found.sort_by(|a, b| as_text(&a.path).cmp(as_text(&b.path)));
    Ok(found)
}

fn walk_dir(
    root: &Path,
    dir: &Path,
    compiled: &[glob::Pattern],
    found: &mut Vec<GeneratedFile>,
) -> Result<(), FinaltreeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Ok(()),
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
    // git directory is pruned at every level, as the os.walk
    // reading orders them. The descent takes only the folders
    // standing as themselves, never a link naming a folder.
    let mut dirnames: Vec<&(Vec<u8>, PathBuf, bool, bool)> = names
        .iter()
        .filter(|(_, _, follows_dir, is_link)| *follows_dir && !*is_link)
        .collect();
    dirnames.retain(|(name, _, _, _)| name != b".git");
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
        walk_dir(root, path, compiled, found)?;
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
    let generated = generated_files(root, &patterns)?;
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
    })
}
