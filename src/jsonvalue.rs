//! The JSON document model and its decoder.
//!
//! The decoder reads the grammar the Python `json` decoder reads
//! for the documents this tool reads, the fingerprint record and
//! the command record standing inside a stamped log. A key
//! standing twice in one object leaves the last value standing,
//! as the Python dictionary reading leaves it, and this repo
//! carries no duplicate key refusal at decode, the refusal of
//! FIX-7 standing at the member lists of the record instead.
//! Nesting is bounded at 4000 levels and a lone surrogate escape
//! is refused, as the port limits in the contract state. Integer
//! tokens that do not fit in a signed 64 bit integer keep their
//! exact digit text.

/// The deepest nesting the decoder accepts.
pub const MAX_NESTING_DEPTH: usize = 4000;

/// A parsed JSON document, preserving the written order of fields.
///
/// Integer tokens that fit in an `i64` are stored as `Int`. Every
/// other integer token is stored as `BigInt` holding its exact
/// digit text, sign included, so it renders back digit for digit.
///
/// A string carrying a lone surrogate escape, which the strict
/// reading refuses, is stored under the lenient reading as
/// `SurrogateText` holding its UTF-16 code units, so the record
/// reading can name the position the surrogate stands in. The
/// Python text type holds such a string, and the readings that
/// meet it decide by where it stands, so the lenient reading
/// exists for the record surface and nowhere else.
#[derive(Debug, Clone)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Int(i64),
    BigInt(String),
    Float(f64),
    Text(String),
    SurrogateText(Vec<u16>),
    List(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(pairs) => pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&Vec<(String, JsonValue)>> {
        match self {
            JsonValue::Object(pairs) => Some(pairs),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&Vec<JsonValue>> {
        match self {
            JsonValue::List(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            JsonValue::Text(text) => Some(text),
            _ => None,
        }
    }

    pub fn is_object(&self) -> bool {
        matches!(self, JsonValue::Object(_))
    }

    /// Whether the value carries a lone surrogate anywhere
    /// inside it, in a string of its own or in a nested value.
    /// Object keys are read as clean strings by both readings,
    /// so only values are searched.
    pub fn contains_surrogate(&self) -> bool {
        match self {
            JsonValue::SurrogateText(_) => true,
            JsonValue::List(items) => items.iter().any(|item| item.contains_surrogate()),
            JsonValue::Object(pairs) => pairs.iter().any(|(_, value)| value.contains_surrogate()),
            _ => false,
        }
    }

    /// The code points of a text value, where a surrogate pair
    /// stands as the one character it spells and a lone
    /// surrogate stands as its own value, as the Python text
    /// reading orders them. Nothing comes back for a value that
    /// is not a text.
    pub fn code_points(&self) -> Option<Vec<u32>> {
        match self {
            JsonValue::Text(text) => Some(text.chars().map(|ch| ch as u32).collect()),
            JsonValue::SurrogateText(units) => {
                let mut points = Vec::new();
                let mut index = 0;
                while index < units.len() {
                    let unit = units[index];
                    if (0xD800..0xDC00).contains(&unit)
                        && index + 1 < units.len()
                        && (0xDC00..0xE000).contains(&units[index + 1])
                    {
                        points.push(
                            0x1_0000
                                + (((unit as u32) - 0xD800) << 10)
                                + ((units[index + 1] as u32) - 0xDC00),
                        );
                        index += 2;
                    } else {
                        points.push(unit as u32);
                        index += 1;
                    }
                }
                Some(points)
            }
            _ => None,
        }
    }

    pub fn object(pairs: Vec<(&str, JsonValue)>) -> JsonValue {
        JsonValue::Object(
            pairs
                .into_iter()
                .map(|(key, value)| (key.to_string(), value))
                .collect(),
        )
    }

    pub fn text(value: impl Into<String>) -> JsonValue {
        JsonValue::Text(value.into())
    }
}

fn float_integer_text(value: f64) -> Option<String> {
    if !value.is_finite() || value.fract() != 0.0 {
        return None;
    }
    if value == 0.0 {
        return Some("0".to_string());
    }
    Some(format!("{value:.0}"))
}

fn integer_text(value: &JsonValue) -> Option<String> {
    match value {
        JsonValue::Int(integer) => Some(integer.to_string()),
        JsonValue::BigInt(text) => Some(text.clone()),
        JsonValue::Bool(flag) => Some(if *flag { "1" } else { "0" }.to_string()),
        _ => None,
    }
}

fn numbers_equal(a: &JsonValue, b: &JsonValue) -> Option<bool> {
    let a_numeric = matches!(
        a,
        JsonValue::Int(_) | JsonValue::BigInt(_) | JsonValue::Float(_) | JsonValue::Bool(_)
    );
    let b_numeric = matches!(
        b,
        JsonValue::Int(_) | JsonValue::BigInt(_) | JsonValue::Float(_) | JsonValue::Bool(_)
    );
    if !a_numeric || !b_numeric {
        return None;
    }
    if let (JsonValue::Float(x), JsonValue::Float(y)) = (a, b) {
        // The Python dictionary equality reading, under which two
        // references to the one NaN the decoder produces compare
        // equal, as they do for the values this reader builds.
        return Some(x == y || (x.is_nan() && y.is_nan()));
    }
    // Mixed kinds compare exactly, the way Python compares int and
    // float and bool across kinds.
    let a_float = match a {
        JsonValue::Float(value) => Some(*value),
        _ => None,
    };
    let b_float = match b {
        JsonValue::Float(value) => Some(*value),
        _ => None,
    };
    match (a_float, b_float) {
        (Some(float), None) => Some(match integer_text(b) {
            Some(text) => float_integer_text(float).is_some_and(|exact| exact == text),
            None => false,
        }),
        (None, Some(float)) => Some(match integer_text(a) {
            Some(text) => float_integer_text(float).is_some_and(|exact| exact == text),
            None => false,
        }),
        _ => Some(integer_text(a) == integer_text(b)),
    }
}

impl PartialEq for JsonValue {
    fn eq(&self, other: &JsonValue) -> bool {
        if let Some(equal) = numbers_equal(self, other) {
            return equal;
        }
        match (self, other) {
            (JsonValue::Null, JsonValue::Null) => true,
            (JsonValue::Text(a), JsonValue::Text(b)) => a == b,
            (JsonValue::SurrogateText(a), JsonValue::SurrogateText(b)) => a == b,
            (JsonValue::List(a), JsonValue::List(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x == y)
            }
            (JsonValue::Object(a), JsonValue::Object(b)) => {
                a.len() == b.len()
                    && a.iter().all(|(key, value)| match other.get(key) {
                        Some(other_value) => value == other_value,
                        None => false,
                    })
            }
            _ => false,
        }
    }
}

/// Why a document did not decode. The one failure is the invalid
/// document, under which the nesting bound and the lone surrogate
/// refusal also stand, as the contract states.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseFailure {
    Invalid,
}

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// Whether a lone surrogate escape in a string value is
    /// carried as a surrogate text instead of refused. Object
    /// keys are read as clean strings under both readings.
    lenient: bool,
}

/// Whether a run of UTF-16 code units holds a surrogate that
/// stands alone, unpaired with the unit it would pair with.
fn lone_surrogate_stands(units: &[u16]) -> bool {
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        if (0xD800..0xDC00).contains(&unit) {
            if index + 1 < units.len() && (0xDC00..0xE000).contains(&units[index + 1]) {
                index += 2;
                continue;
            }
            return true;
        }
        if (0xDC00..0xE000).contains(&unit) {
            return true;
        }
        index += 1;
    }
    false
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn skip_whitespace(&mut self) {
        while let Some(byte) = self.peek() {
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn parse_value(&mut self, depth: usize) -> Result<JsonValue, ParseFailure> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'{') => self.parse_object(depth),
            Some(b'[') => self.parse_list(depth),
            Some(b'"') => self.parse_string_value(),
            Some(b't') => self.parse_literal("true", JsonValue::Bool(true)),
            Some(b'f') => self.parse_literal("false", JsonValue::Bool(false)),
            Some(b'n') => self.parse_literal("null", JsonValue::Null),
            Some(b'N') => self.parse_literal("NaN", JsonValue::Float(f64::NAN)),
            Some(b'I') => self.parse_literal("Infinity", JsonValue::Float(f64::INFINITY)),
            Some(b'-') | Some(b'0'..=b'9') => self.parse_number(),
            _ => Err(ParseFailure::Invalid),
        }
    }

    fn parse_literal(&mut self, word: &str, value: JsonValue) -> Result<JsonValue, ParseFailure> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(ParseFailure::Invalid)
        }
    }

    fn parse_list(&mut self, depth: usize) -> Result<JsonValue, ParseFailure> {
        if depth >= MAX_NESTING_DEPTH {
            return Err(ParseFailure::Invalid);
        }
        self.pos += 1;
        let mut items = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b']') {
            self.pos += 1;
            return Ok(JsonValue::List(items));
        }
        loop {
            let item = self.parse_value(depth + 1)?;
            items.push(item);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(JsonValue::List(items));
                }
                _ => return Err(ParseFailure::Invalid),
            }
        }
    }

    fn parse_object(&mut self, depth: usize) -> Result<JsonValue, ParseFailure> {
        if depth >= MAX_NESTING_DEPTH {
            return Err(ParseFailure::Invalid);
        }
        self.pos += 1;
        let mut pairs: Vec<(String, JsonValue)> = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            return Ok(JsonValue::Object(pairs));
        }
        loop {
            self.skip_whitespace();
            if self.peek() != Some(b'"') {
                return Err(ParseFailure::Invalid);
            }
            let units = self.parse_string_units()?;
            if lone_surrogate_stands(&units) {
                return Err(ParseFailure::Invalid);
            }
            let key = String::from_utf16(&units).map_err(|_| ParseFailure::Invalid)?;
            self.skip_whitespace();
            if self.peek() != Some(b':') {
                return Err(ParseFailure::Invalid);
            }
            self.pos += 1;
            let value = self.parse_value(depth + 1)?;
            // The dictionary reading keeps the standing pair and
            // replaces its value, so the last value stands at the
            // position the key first stood at.
            match pairs.iter_mut().find(|(name, _)| *name == key) {
                Some((_, standing)) => *standing = value,
                None => pairs.push((key, value)),
            }
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(ParseFailure::Invalid),
            }
        }
        Ok(JsonValue::Object(pairs))
    }

    /// Read a string as its UTF-16 code units, the form in
    /// which a lone surrogate escape stays visible as itself
    /// instead of failing the reading that meets it.
    fn parse_string_units(&mut self) -> Result<Vec<u16>, ParseFailure> {
        // The caller has seen the opening quote.
        self.pos += 1;
        let mut units: Vec<u16> = Vec::new();
        loop {
            let byte = self.peek().ok_or(ParseFailure::Invalid)?;
            match byte {
                b'"' => {
                    self.pos += 1;
                    return Ok(units);
                }
                b'\\' => {
                    self.pos += 1;
                    let escape = self.peek().ok_or(ParseFailure::Invalid)?;
                    self.pos += 1;
                    match escape {
                        b'"' => units.push(u16::from(b'"')),
                        b'\\' => units.push(u16::from(b'\\')),
                        b'/' => units.push(u16::from(b'/')),
                        b'b' => units.push(0x0008),
                        b'f' => units.push(0x000C),
                        b'n' => units.push(u16::from(b'\n')),
                        b'r' => units.push(u16::from(b'\r')),
                        b't' => units.push(u16::from(b'\t')),
                        b'u' => units.push(self.parse_hex4()?),
                        _ => return Err(ParseFailure::Invalid),
                    }
                }
                0x00..=0x1F => return Err(ParseFailure::Invalid),
                _ => {
                    // Copy one whole UTF-8 character. The input is a
                    // string slice, so the bytes are valid UTF-8 and
                    // the cursor stands on a character boundary.
                    let width = if byte < 0x80 {
                        1
                    } else if byte >> 5 == 0b110 {
                        2
                    } else if byte >> 4 == 0b1110 {
                        3
                    } else {
                        4
                    };
                    let text = std::str::from_utf8(&self.bytes[self.pos..self.pos + width])
                        .map_err(|_| ParseFailure::Invalid)?;
                    let ch = text.chars().next().ok_or(ParseFailure::Invalid)?;
                    let mut buffer = [0u16; 2];
                    units.extend_from_slice(ch.encode_utf16(&mut buffer));
                    self.pos += width;
                }
            }
        }
    }

    /// Read a string value, carrying a lone surrogate as a
    /// surrogate text under the lenient reading and refusing it
    /// under the strict one.
    fn parse_string_value(&mut self) -> Result<JsonValue, ParseFailure> {
        let units = self.parse_string_units()?;
        if lone_surrogate_stands(&units) {
            if self.lenient {
                return Ok(JsonValue::SurrogateText(units));
            }
            return Err(ParseFailure::Invalid);
        }
        String::from_utf16(&units)
            .map(JsonValue::Text)
            .map_err(|_| ParseFailure::Invalid)
    }

    fn parse_hex4(&mut self) -> Result<u16, ParseFailure> {
        let mut value: u16 = 0;
        for _ in 0..4 {
            let byte = self.peek().ok_or(ParseFailure::Invalid)?;
            let digit = match byte {
                b'0'..=b'9' => u16::from(byte - b'0'),
                b'a'..=b'f' => u16::from(byte - b'a') + 10,
                b'A'..=b'F' => u16::from(byte - b'A') + 10,
                _ => return Err(ParseFailure::Invalid),
            };
            value = value * 16 + digit;
            self.pos += 1;
        }
        Ok(value)
    }

    fn parse_number(&mut self) -> Result<JsonValue, ParseFailure> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
            if self.bytes[self.pos..].starts_with(b"Infinity") {
                self.pos += "Infinity".len();
                return Ok(JsonValue::Float(f64::NEG_INFINITY));
            }
            if self.bytes[self.pos..].starts_with(b"NaN") {
                self.pos += "NaN".len();
                return Ok(JsonValue::Float(f64::NAN));
            }
        }
        // Integer part: a single zero or a nonzero digit run.
        match self.peek() {
            Some(b'0') => {
                self.pos += 1;
            }
            Some(b'1'..=b'9') => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.pos += 1;
                }
            }
            _ => return Err(ParseFailure::Invalid),
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') {
            is_float = true;
            self.pos += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(ParseFailure::Invalid);
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        if matches!(self.peek(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.pos += 1;
            if matches!(self.peek(), Some(b'+') | Some(b'-')) {
                self.pos += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return Err(ParseFailure::Invalid);
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let token =
            std::str::from_utf8(&self.bytes[start..self.pos]).map_err(|_| ParseFailure::Invalid)?;
        if is_float {
            token
                .parse::<f64>()
                .map(JsonValue::Float)
                .map_err(|_| ParseFailure::Invalid)
        } else {
            match token.parse::<i64>() {
                Ok(integer) => Ok(JsonValue::Int(integer)),
                Err(_) => Ok(JsonValue::BigInt(token.to_string())),
            }
        }
    }
}

/// Decode a JSON document under the Python dictionary reading.
pub fn parse_json(text: &str) -> Result<JsonValue, ParseFailure> {
    parse_document(text, false)
}

/// Decode a JSON document as the strict reading does, except
/// that a lone surrogate escape in a string value is carried
/// as a surrogate text instead of refused, so the record
/// reading can name the position the surrogate stands in.
pub fn parse_json_lenient(text: &str) -> Result<JsonValue, ParseFailure> {
    parse_document(text, true)
}

fn parse_document(text: &str, lenient: bool) -> Result<JsonValue, ParseFailure> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
        lenient,
    };
    let value = parser.parse_value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(ParseFailure::Invalid);
    }
    Ok(value)
}
