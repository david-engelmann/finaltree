//! Rendering a document the way the Python `json` encoder renders
//! it, in the spellings the stamped log and the fingerprint
//! record use.
//!
//! The renderer covers the compact raw Unicode form the record
//! digest is computed over, the default separator raw Unicode
//! form the log lines carry, and the indented ASCII escaped form
//! the fingerprint print carries. Keys sort by code point, floats
//! use the shortest round trip spelling, and the nonfinite
//! spellings are the Python ones.

use crate::jsonvalue::JsonValue;

/// The Python float spelling inside JSON text.
///
/// Python spells a float with the shortest string that round
/// trips, in fixed notation when the decimal point stands from
/// four places left of the leading digit to sixteen places right
/// of it, and in scientific notation outside that range, with a
/// signed exponent of at least two digits. Integral values gain
/// a trailing `.0` in fixed notation.
pub fn json_float_text(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    // Rust's LowerExp spelling is also a shortest round trip
    // spelling, always of the shape d.ddddde<exp> with no plus
    // sign and no zero padding on the exponent. Its digit string
    // is the same one the Python repr chooses, so the Python
    // notation is rebuilt from those digits and that exponent.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent_text) = scientific
        .split_once('e')
        .expect("LowerExp always carries an exponent");
    let exponent: i32 = exponent_text
        .parse()
        .expect("LowerExp exponent is an integer");
    let digits: String = mantissa.chars().filter(|ch| *ch != '.').collect();
    let digits = digits.as_bytes();
    let sign = if value < 0.0 { "-" } else { "" };
    // The decimal point stands after the leading digit in the
    // scientific reading, so its position counting digits from
    // the leading digit is exponent + 1. Python uses fixed
    // notation when that position is in -3..=16.
    let point = exponent + 1;
    if (-3..=16).contains(&point) {
        let mut out = String::from(sign);
        if point <= 0 {
            out.push_str("0.");
            for _ in 0..(-point) {
                out.push('0');
            }
            out.push_str(&digits_text(digits));
        } else if (point as usize) >= digits.len() {
            out.push_str(&digits_text(digits));
            for _ in 0..(point as usize - digits.len()) {
                out.push('0');
            }
            out.push_str(".0");
        } else {
            out.push_str(&digits_text(&digits[..point as usize]));
            out.push('.');
            out.push_str(&digits_text(&digits[point as usize..]));
        }
        out
    } else {
        let mut out = String::from(sign);
        out.push(digits[0] as char);
        if digits.len() > 1 {
            out.push('.');
            out.push_str(&digits_text(&digits[1..]));
        }
        out.push('e');
        if exponent >= 0 {
            out.push('+');
        } else {
            out.push('-');
        }
        let magnitude = exponent.unsigned_abs();
        if magnitude < 10 {
            out.push('0');
        }
        out.push_str(&magnitude.to_string());
        out
    }
}

fn digits_text(digits: &[u8]) -> String {
    digits.iter().map(|byte| *byte as char).collect()
}

fn escape_string(text: &str, ensure_ascii: bool, out: &mut String) {
    out.push('"');
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            _ => {
                let code = ch as u32;
                if code < 0x20 || (ensure_ascii && code > 0x7E) {
                    if code > 0xFFFF {
                        let adjusted = code - 0x1_0000;
                        let high = 0xD800 + (adjusted >> 10);
                        let low = 0xDC00 + (adjusted & 0x3FF);
                        out.push_str(&format!("\\u{high:04x}\\u{low:04x}"));
                    } else {
                        out.push_str(&format!("\\u{code:04x}"));
                    }
                } else {
                    out.push(ch);
                }
            }
        }
    }
    out.push('"');
}

struct Style {
    indent: Option<usize>,
    item_separator: &'static str,
    key_separator: &'static str,
    ensure_ascii: bool,
    sort_keys: bool,
}

fn render_into(value: &JsonValue, style: &Style, level: usize, out: &mut String) {
    match value {
        JsonValue::Null => out.push_str("null"),
        JsonValue::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
        JsonValue::Int(integer) => out.push_str(&integer.to_string()),
        JsonValue::BigInt(text) => out.push_str(text),
        JsonValue::Float(float) => out.push_str(&json_float_text(*float)),
        JsonValue::Text(text) => escape_string(text, style.ensure_ascii, out),
        JsonValue::SurrogateText(_) => {
            // A surrogate text renders with each lone unit
            // spelled as its escape, the only spelling that
            // carries it. No accepted record renders one, so
            // this spelling is a floor and not a surface.
            out.push('"');
            if let Some(points) = value.code_points() {
                for point in points {
                    if (0xD800..0xE000).contains(&point) {
                        out.push_str(&format!("\\u{point:04x}"));
                    } else if let Some(ch) = char::from_u32(point) {
                        let mut holding = String::new();
                        escape_string(&ch.to_string(), style.ensure_ascii, &mut holding);
                        out.push_str(&holding[1..holding.len() - 1]);
                    }
                }
            }
            out.push('"');
        }
        JsonValue::List(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push_str(style.item_separator);
                }
                if let Some(width) = style.indent {
                    out.push('\n');
                    out.push_str(&" ".repeat(width * (level + 1)));
                }
                render_into(item, style, level + 1, out);
            }
            if let Some(width) = style.indent {
                out.push('\n');
                out.push_str(&" ".repeat(width * level));
            }
            out.push(']');
        }
        JsonValue::Object(pairs) => {
            if pairs.is_empty() {
                out.push_str("{}");
                return;
            }
            let mut ordered: Vec<&(String, JsonValue)> = pairs.iter().collect();
            if style.sort_keys {
                ordered.sort_by(|a, b| a.0.cmp(&b.0));
            }
            out.push('{');
            for (index, (key, item)) in ordered.iter().enumerate() {
                if index > 0 {
                    out.push_str(style.item_separator);
                }
                if let Some(width) = style.indent {
                    out.push('\n');
                    out.push_str(&" ".repeat(width * (level + 1)));
                }
                escape_string(key, style.ensure_ascii, out);
                out.push_str(style.key_separator);
                render_into(item, style, level + 1, out);
            }
            if let Some(width) = style.indent {
                out.push('\n');
                out.push_str(&" ".repeat(width * level));
            }
            out.push('}');
        }
    }
}

/// The compact sorted raw Unicode form the record digest is
/// computed over, `json.dumps(value, sort_keys=True,
/// separators=(",", ":"), ensure_ascii=False)`.
pub fn render_compact(value: &JsonValue) -> String {
    let style = Style {
        indent: None,
        item_separator: ",",
        key_separator: ":",
        ensure_ascii: false,
        sort_keys: true,
    };
    let mut out = String::new();
    render_into(value, &style, 0, &mut out);
    out
}

/// The default separator sorted raw Unicode form the log lines
/// carry, `json.dumps(value, sort_keys=True,
/// ensure_ascii=False)`.
pub fn render_line(value: &JsonValue) -> String {
    let style = Style {
        indent: None,
        item_separator: ", ",
        key_separator: ": ",
        ensure_ascii: false,
        sort_keys: true,
    };
    let mut out = String::new();
    render_into(value, &style, 0, &mut out);
    out
}

/// The default separator raw Unicode form with keys in
/// standing order, `json.dumps(value, ensure_ascii=False)`, the
/// spelling the command line of a stamped log carries.
pub fn render_line_unsorted(value: &JsonValue) -> String {
    let style = Style {
        indent: None,
        item_separator: ", ",
        key_separator: ": ",
        ensure_ascii: false,
        sort_keys: false,
    };
    let mut out = String::new();
    render_into(value, &style, 0, &mut out);
    out
}

/// The Python string spelling of a float, the spelling the
/// difference messages interpolate, where the nonfinite values
/// spell as words and every finite value spells as its repr.
fn py_float_str(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }
    json_float_text(value)
}

/// The repr spelling of a text standing as a sequence of code
/// points, as the Python repr builds it, with the quote chosen
/// against the quotes the text carries.
fn py_repr_points(points: &[u32]) -> String {
    let has_single = points.contains(&(b'\'' as u32));
    let has_double = points.contains(&(b'"' as u32));
    let quote = if has_single && !has_double { '"' } else { '\'' };
    let mut out = String::new();
    out.push(quote);
    for point in points {
        match *point {
            0x5C => out.push_str("\\\\"),
            0x0A => out.push_str("\\n"),
            0x0D => out.push_str("\\r"),
            0x09 => out.push_str("\\t"),
            point if point == quote as u32 => {
                out.push('\\');
                out.push(quote);
            }
            point if (0xD800..0xE000).contains(&point) => {
                out.push_str(&format!("\\u{point:04x}"));
            }
            point => match char::from_u32(point) {
                Some(ch) if ch.is_control() || point == 0x7F => {
                    if point < 0x100 {
                        out.push_str(&format!("\\x{point:02x}"));
                    } else if point < 0x1_0000 {
                        out.push_str(&format!("\\u{point:04x}"));
                    } else {
                        out.push_str(&format!("\\U{point:08x}"));
                    }
                }
                Some(ch) => out.push(ch),
                None => out.push_str(&format!("\\u{point:04x}")),
            },
        }
    }
    out.push(quote);
    out
}

/// The Python repr of a value, the spelling a container
/// standing in a message carries for its members.
pub fn py_repr(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "None".to_string(),
        JsonValue::Bool(flag) => {
            if *flag {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        JsonValue::Int(integer) => integer.to_string(),
        JsonValue::BigInt(text) => text.clone(),
        JsonValue::Float(value) => py_float_str(*value),
        JsonValue::Text(_) | JsonValue::SurrogateText(_) => {
            py_repr_points(&value.code_points().unwrap_or_default())
        }
        JsonValue::List(items) => {
            let parts: Vec<String> = items.iter().map(py_repr).collect();
            format!("[{}]", parts.join(", "))
        }
        JsonValue::Object(pairs) => {
            let parts: Vec<String> = pairs
                .iter()
                .map(|(key, item)| {
                    format!(
                        "{}: {}",
                        py_repr_points(&key.chars().map(|ch| ch as u32).collect::<Vec<u32>>()),
                        py_repr(item)
                    )
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
    }
}

/// The Python string spelling of a value, the spelling the
/// difference messages of the verification reading
/// interpolate for the commit and the member paths and kinds.
pub fn py_str(value: &JsonValue) -> String {
    match value {
        JsonValue::Text(text) => text.clone(),
        JsonValue::Null => "None".to_string(),
        JsonValue::Bool(flag) => {
            if *flag {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        JsonValue::Int(integer) => integer.to_string(),
        JsonValue::BigInt(text) => text.clone(),
        JsonValue::Float(value) => py_float_str(*value),
        JsonValue::List(_) | JsonValue::Object(_) | JsonValue::SurrogateText(_) => py_repr(value),
    }
}

/// The indented ASCII escaped form the fingerprint print
/// carries, `json.dumps(value, indent=2, sort_keys=True)`.
pub fn render_printed(value: &JsonValue) -> String {
    let style = Style {
        indent: Some(2),
        item_separator: ",",
        key_separator: ": ",
        ensure_ascii: true,
        sort_keys: true,
    };
    let mut out = String::new();
    render_into(value, &style, 0, &mut out);
    out
}
