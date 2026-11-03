//! The integer grammar the stamped log reader applies to the
//! exit code line, the grammar of the Python `int` reading of a
//! string.
//!
//! The reading strips surrounding whitespace, allows one leading
//! sign, and then takes a run of decimal digits in which a single
//! underscore may stand between two digits. A decimal digit here
//! is any code point standing in the Unicode decimal number
//! category, each carrying the value its position in its range
//! gives it. A token beyond the digit limit of the Python
//! runtime is refused here as it is there.

/// The digit limit the Python runtime applies to an integer
/// reading.
const PYTHON_INT_DIGITS: usize = 4300;

/// The code point ranges standing in the Unicode decimal
/// number category, split at every point where the decimal
/// values stop running from the start of the range. The table
/// was generated from the unicodedata of the Python runtime
/// the landed spec runs on, CPython 3.12 over Unicode 15.0.0,
/// by the generator kept with the sibling crate that first
/// needed it, and it is committed here as source data. The
/// split matters where blocks stand together in code point
/// space while their values restart, as the five mathematical
/// blocks do, because a digit carries the value its position
/// in its own block gives it and no value passes nine.
static ND_RANGES: &[(u32, u32)] = &[
    (0x0030, 0x0039),
    (0x0660, 0x0669),
    (0x06f0, 0x06f9),
    (0x07c0, 0x07c9),
    (0x0966, 0x096f),
    (0x09e6, 0x09ef),
    (0x0a66, 0x0a6f),
    (0x0ae6, 0x0aef),
    (0x0b66, 0x0b6f),
    (0x0be6, 0x0bef),
    (0x0c66, 0x0c6f),
    (0x0ce6, 0x0cef),
    (0x0d66, 0x0d6f),
    (0x0de6, 0x0def),
    (0x0e50, 0x0e59),
    (0x0ed0, 0x0ed9),
    (0x0f20, 0x0f29),
    (0x1040, 0x1049),
    (0x1090, 0x1099),
    (0x17e0, 0x17e9),
    (0x1810, 0x1819),
    (0x1946, 0x194f),
    (0x19d0, 0x19d9),
    (0x1a80, 0x1a89),
    (0x1a90, 0x1a99),
    (0x1b50, 0x1b59),
    (0x1bb0, 0x1bb9),
    (0x1c40, 0x1c49),
    (0x1c50, 0x1c59),
    (0xa620, 0xa629),
    (0xa8d0, 0xa8d9),
    (0xa900, 0xa909),
    (0xa9d0, 0xa9d9),
    (0xa9f0, 0xa9f9),
    (0xaa50, 0xaa59),
    (0xabf0, 0xabf9),
    (0xff10, 0xff19),
    (0x104a0, 0x104a9),
    (0x10d30, 0x10d39),
    (0x11066, 0x1106f),
    (0x110f0, 0x110f9),
    (0x11136, 0x1113f),
    (0x111d0, 0x111d9),
    (0x112f0, 0x112f9),
    (0x11450, 0x11459),
    (0x114d0, 0x114d9),
    (0x11650, 0x11659),
    (0x116c0, 0x116c9),
    (0x11730, 0x11739),
    (0x118e0, 0x118e9),
    (0x11950, 0x11959),
    (0x11c50, 0x11c59),
    (0x11d50, 0x11d59),
    (0x11da0, 0x11da9),
    (0x11f50, 0x11f59),
    (0x16a60, 0x16a69),
    (0x16ac0, 0x16ac9),
    (0x16b50, 0x16b59),
    (0x1d7ce, 0x1d7d7),
    (0x1d7d8, 0x1d7e1),
    (0x1d7e2, 0x1d7eb),
    (0x1d7ec, 0x1d7f5),
    (0x1d7f6, 0x1d7ff),
    (0x1e140, 0x1e149),
    (0x1e2f0, 0x1e2f9),
    (0x1e4f0, 0x1e4f9),
    (0x1e950, 0x1e959),
    (0x1fbf0, 0x1fbf9),
];

/// The decimal value of a code point, when it stands in the
/// decimal number category.
fn decimal_digit(ch: char) -> Option<u32> {
    let point = ch as u32;
    ND_RANGES
        .binary_search_by(|&(low, high)| {
            if point < low {
                std::cmp::Ordering::Greater
            } else if point > high {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .ok()
        .map(|index| point - ND_RANGES[index].0)
}

/// Read a string as the Python integer reading reads it.
///
/// The answer carries the value in plain decimal text, the
/// spelling the stamped log reader compares and the evidence
/// recomputation joins on, so a value of any size the grammar
/// admits is carried exactly. Nothing comes back for a token
/// the grammar refuses.
/// The whitespace the Python strip removes around an integer
/// token. The set is the Unicode whitespace set together with
/// the file separator code points the Python reading also names
/// as space.
pub(crate) fn py_is_space(ch: char) -> bool {
    ch.is_whitespace() || ('\u{001C}'..='\u{001F}').contains(&ch)
}

pub fn parse_python_int(text: &str) -> Option<String> {
    let stripped = text.trim_matches(py_is_space);
    let mut chars = stripped.chars().peekable();
    let mut negative = false;
    match chars.peek() {
        Some('+') => {
            chars.next();
        }
        Some('-') => {
            negative = true;
            chars.next();
        }
        _ => {}
    }
    let mut digits: Vec<u32> = Vec::new();
    let mut last_was_digit = false;
    let mut last_was_underscore = false;
    for ch in chars {
        if ch == '_' {
            if !last_was_digit || last_was_underscore {
                return None;
            }
            last_was_digit = false;
            last_was_underscore = true;
            continue;
        }
        let digit = decimal_digit(ch)?;
        digits.push(digit);
        last_was_digit = true;
        last_was_underscore = false;
    }
    if digits.is_empty() || last_was_underscore {
        return None;
    }
    if digits.len() > PYTHON_INT_DIGITS {
        return None;
    }
    // Leading zeros fall away, and an all zero reading is zero,
    // which carries no sign in the decimal spelling.
    let first_significant = digits.iter().position(|digit| *digit != 0);
    let body: String = match first_significant {
        Some(start) => digits[start..]
            .iter()
            .map(|digit| (b'0' + *digit as u8) as char)
            .collect(),
        None => return Some("0".to_string()),
    };
    if negative {
        Some(format!("-{body}"))
    } else {
        Some(body)
    }
}
