//! The pattern reading for generated member declarations, the
//! FIX-5 translation of the contract, ported from the landed
//! Python translation in `fingerprint.py`.
//!
//! The Python reading builds a regular expression. A `**/`
//! becomes an optional run of whole leading components, a `**`
//! not followed by a slash becomes a run of any characters
//! except a newline, `*` becomes a run inside one component,
//! `?` becomes one character inside one component, and a
//! bracket list becomes a character class in which a leading
//! `!` negates. Every other character matches itself, and the
//! whole relative path must match. The compiled form here is a
//! small matcher over tokens that answers the same matches
//! on the paths the walk produces.
//!
//! Inside a bracket list the body passes to the expression
//! class reading with every backslash doubled, which leaves
//! each backslash standing as one literal member. A hyphen
//! between two members is a range, a hyphen at either end is
//! itself, a caret standing first negates, and a reversed
//! range is a range the expression reading refuses, which the
//! landed Python surfaces as an uncaught error out of the
//! fingerprint reading. The port refuses such a pattern at
//! the fingerprint reading with a stated reason, so the call
//! fails on both sides rather than the pattern passing
//! silently.
//!
//! A pattern and a path are read as sequences of tokens. A
//! character of the text stands as a character token, and a
//! byte that no UTF 8 sequence claims stands as a byte token
//! of its own kind, the standing the Python runtime gives the
//! byte when it decodes an argument word with its
//! surrogateescape reading and holds it as a lone surrogate.
//! A byte token is never a character token, so no character
//! standing in a path, of the private use block or any other,
//! can stand for a byte, and a byte token stands equal only
//! to the same byte standing as a token. Where a bracket list
//! judges a token, by membership or by a range, the token
//! stands for its code point, a byte token standing for the
//! surrogate code point the Python reading holds for it,
//! which is the comparison the translated expression performs
//! over the Python text.

/// One unit of a pattern or a path as the matching reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    /// A character of the text.
    Char(char),
    /// One byte no UTF 8 sequence claims, standing for the
    /// surrogate the Python text type holds for it.
    Byte(u8),
}

impl Token {
    /// The code point the token stands for in the Python
    /// reading, a character standing for itself and a byte
    /// standing for its surrogate code point.
    pub fn code_point(self) -> u32 {
        match self {
            Token::Char(ch) => u32::from(ch),
            Token::Byte(byte) => 0xDC00 + u32::from(byte),
        }
    }
}

/// The tokens of a text, one character token per character.
pub fn tokens_of_text(text: &str) -> Vec<Token> {
    text.chars().map(Token::Char).collect()
}

/// Decode a word standing in its bytes into tokens, the way
/// the Python runtime decodes its argument words. A valid
/// UTF 8 sequence stands as its character, and each byte no
/// sequence claims stands as a byte token of its own, in the
/// order of the bytes.
pub fn decode_word(bytes: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte < 0x80 {
            tokens.push(Token::Char(byte as char));
            index += 1;
            continue;
        }
        let (width, floor) = match byte {
            0xC2..=0xDF => (2, 0x80),
            0xE0..=0xEF => (3, 0x800),
            0xF0..=0xF4 => (4, 0x10000),
            _ => (0, 0),
        };
        let decoded = if width > 0
            && index + width <= bytes.len()
            && bytes[index + 1..index + width]
                .iter()
                .all(|part| (0x80..0xC0).contains(part))
        {
            let mut point = u32::from(byte) & (0x7F >> width);
            for part in &bytes[index + 1..index + width] {
                point = (point << 6) | u32::from(part & 0x3F);
            }
            if point >= floor && point <= 0x10FFFF && !(0xD800..0xE000).contains(&point) {
                char::from_u32(point)
            } else {
                None
            }
        } else {
            None
        };
        match decoded {
            Some(ch) => {
                tokens.push(Token::Char(ch));
                index += width;
            }
            None => {
                tokens.push(Token::Byte(byte));
                index += 1;
            }
        }
    }
    tokens
}

/// One piece of a translated pattern.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    /// A token that matches itself.
    Literal(Token),
    /// `*`, any run inside one component, never a slash.
    Star,
    /// `**`, any run of characters, newlines excepted.
    StarStar,
    /// `**/`, an optional run of whole leading components, the
    /// run itself holding no newline.
    StarStarSlash,
    /// `?`, one character that is not a slash.
    Question,
    /// A bracket list, negated or not, over ranges and singles
    /// standing as the code points their tokens stand for.
    /// The class reading matches the slash as the expression
    /// class matches it, a listed slash standing as a member
    /// and an unlisted slash matching a negated list, because
    /// the class carries no slash exclusion of its own.
    Class {
        negated: bool,
        singles: Vec<u32>,
        ranges: Vec<(u32, u32)>,
    },
}

/// A translated pattern, matched against a whole path.
#[derive(Debug, Clone)]
pub struct Pattern {
    pieces: Vec<Piece>,
    /// Whether the translation holds a range the expression
    /// reading refuses. Such a pattern is never matched
    /// against, because the fingerprint reading refuses it
    /// first.
    never: bool,
}

impl Pattern {
    /// Whether this pattern's translation is one the
    /// expression reading refuses.
    pub fn refused(&self) -> bool {
        self.never
    }
}

/// Translate a declared pattern under the FIX-5 reading.
pub fn translate(pattern: &str) -> Pattern {
    translate_tokens(&tokens_of_text(pattern))
}

/// Translate a pattern standing as tokens under the FIX-5
/// reading.
pub fn translate_tokens(tokens: &[Token]) -> Pattern {
    let mut pieces = Vec::new();
    let mut never = false;
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        match token {
            Token::Char('*') => {
                if tokens.get(index + 1) == Some(&Token::Char('*')) {
                    if tokens.get(index + 2) == Some(&Token::Char('/')) {
                        pieces.push(Piece::StarStarSlash);
                        index += 3;
                    } else {
                        pieces.push(Piece::StarStar);
                        index += 2;
                    }
                } else {
                    pieces.push(Piece::Star);
                    index += 1;
                }
            }
            Token::Char('?') => {
                pieces.push(Piece::Question);
                index += 1;
            }
            Token::Char('[') => {
                // The scan for the closing bracket skips a
                // negation marker and a bracket standing first
                // inside the list, as the Python scan does. A
                // list with no closing bracket leaves the opening
                // bracket a literal.
                let mut end = index + 1;
                if tokens.get(end) == Some(&Token::Char('!')) {
                    end += 1;
                }
                if tokens.get(end) == Some(&Token::Char(']')) {
                    end += 1;
                }
                while end < tokens.len() && tokens[end] != Token::Char(']') {
                    end += 1;
                }
                if end >= tokens.len() {
                    pieces.push(Piece::Literal(Token::Char('[')));
                    index += 1;
                    continue;
                }
                let mut body: Vec<Token> = tokens[index + 1..end].to_vec();
                let mut negated = false;
                if body.first() == Some(&Token::Char('!')) {
                    // The marker becomes a caret in the
                    // expression, so a caret standing after it is
                    // a member, not a second marker.
                    negated = true;
                    body.remove(0);
                } else if body.first() == Some(&Token::Char('^')) {
                    // A caret standing first in the expression
                    // negates, and is consumed as the marker.
                    negated = true;
                    body.remove(0);
                }
                let (singles, ranges, bad_range) = read_class_body(&body);
                if bad_range {
                    never = true;
                }
                pieces.push(Piece::Class {
                    negated,
                    singles,
                    ranges,
                });
                index = end + 1;
            }
            _ => {
                pieces.push(Piece::Literal(token));
                index += 1;
            }
        }
    }
    Pattern { pieces, never }
}

/// Read a class body into singles and ranges, standing as the
/// code points their tokens stand for. Every token stands as
/// one member, a backslash included, as the doubling in the
/// Python translation leaves it. A hyphen between two members
/// joins them into a range. A reversed range marks the body
/// bad.
fn read_class_body(body: &[Token]) -> (Vec<u32>, Vec<(u32, u32)>, bool) {
    let mut singles = Vec::new();
    let mut ranges = Vec::new();
    let mut bad_range = false;
    let mut index = 0;
    while index < body.len() {
        if index + 2 < body.len() && body[index + 1] == Token::Char('-') {
            let (low, high) = (body[index].code_point(), body[index + 2].code_point());
            if low > high {
                bad_range = true;
            } else {
                ranges.push((low, high));
            }
            index += 3;
        } else {
            singles.push(body[index].code_point());
            index += 1;
        }
    }
    (singles, ranges, bad_range)
}

fn class_matches(piece: &Piece, token: Token) -> bool {
    match piece {
        Piece::Class {
            negated,
            singles,
            ranges,
        } => {
            let point = token.code_point();
            let inside = singles.contains(&point)
                || ranges
                    .iter()
                    .any(|(low, high)| *low <= point && point <= *high);
            inside != *negated
        }
        _ => false,
    }
}

/// Whether the pattern matches the whole path.
pub fn matches(pattern: &Pattern, path: &str) -> bool {
    matches_tokens(pattern, &tokens_of_text(path))
}

/// Whether the pattern matches the whole path standing as
/// tokens.
pub fn matches_tokens(pattern: &Pattern, path: &[Token]) -> bool {
    if pattern.never {
        return false;
    }
    match_pieces(&pattern.pieces, path)
}

fn match_pieces(pieces: &[Piece], tokens: &[Token]) -> bool {
    if pieces.is_empty() {
        return tokens.is_empty();
    }
    match &pieces[0] {
        Piece::Literal(want) => {
            !tokens.is_empty() && tokens[0] == *want && match_pieces(&pieces[1..], &tokens[1..])
        }
        Piece::Question => {
            !tokens.is_empty()
                && tokens[0] != Token::Char('/')
                && match_pieces(&pieces[1..], &tokens[1..])
        }
        Piece::Class { .. } => {
            !tokens.is_empty()
                && class_matches(&pieces[0], tokens[0])
                && match_pieces(&pieces[1..], &tokens[1..])
        }
        Piece::Star => {
            // Any run of tokens that are not a slash,
            // including the empty run.
            let mut end = 0;
            loop {
                if match_pieces(&pieces[1..], &tokens[end..]) {
                    return true;
                }
                if end >= tokens.len() || tokens[end] == Token::Char('/') {
                    return false;
                }
                end += 1;
            }
        }
        Piece::StarStar => {
            // Any run of tokens that are not a newline,
            // including the empty run.
            let mut end = 0;
            loop {
                if match_pieces(&pieces[1..], &tokens[end..]) {
                    return true;
                }
                if end >= tokens.len() || tokens[end] == Token::Char('\n') {
                    return false;
                }
                end += 1;
            }
        }
        Piece::StarStarSlash => {
            // The empty run, or a run holding no newline and
            // ending at a slash, which the slash of the piece
            // consumes.
            if match_pieces(&pieces[1..], tokens) {
                return true;
            }
            let mut end = 0;
            while end < tokens.len() && tokens[end] != Token::Char('\n') {
                if tokens[end] == Token::Char('/') && match_pieces(&pieces[1..], &tokens[end + 1..])
                {
                    return true;
                }
                end += 1;
            }
            false
        }
    }
}
