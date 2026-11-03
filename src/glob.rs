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
//! small matcher over characters that answers the same matches
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

/// One piece of a translated pattern.
#[derive(Debug, Clone, PartialEq)]
enum Piece {
    /// A character that matches itself.
    Literal(char),
    /// `*`, any run inside one component, never a slash.
    Star,
    /// `**`, any run of characters, newlines excepted.
    StarStar,
    /// `**/`, an optional run of whole leading components, the
    /// run itself holding no newline.
    StarStarSlash,
    /// `?`, one character that is not a slash.
    Question,
    /// A bracket list, negated or not, over ranges and singles.
    /// The class reading matches the slash as the expression
    /// class matches it, a listed slash standing as a member
    /// and an unlisted slash matching a negated list, because
    /// the class carries no slash exclusion of its own.
    Class {
        negated: bool,
        singles: Vec<char>,
        ranges: Vec<(char, char)>,
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
    let chars: Vec<char> = pattern.chars().collect();
    let mut pieces = Vec::new();
    let mut never = false;
    let mut index = 0;
    while index < chars.len() {
        let ch = chars[index];
        match ch {
            '*' => {
                if chars.get(index + 1) == Some(&'*') {
                    if chars.get(index + 2) == Some(&'/') {
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
            '?' => {
                pieces.push(Piece::Question);
                index += 1;
            }
            '[' => {
                // The scan for the closing bracket skips a
                // negation marker and a bracket standing first
                // inside the list, as the Python scan does. A
                // list with no closing bracket leaves the opening
                // bracket a literal.
                let mut end = index + 1;
                if chars.get(end) == Some(&'!') {
                    end += 1;
                }
                if chars.get(end) == Some(&']') {
                    end += 1;
                }
                while end < chars.len() && chars[end] != ']' {
                    end += 1;
                }
                if end >= chars.len() {
                    pieces.push(Piece::Literal('['));
                    index += 1;
                    continue;
                }
                let mut body: Vec<char> = chars[index + 1..end].to_vec();
                let mut negated = false;
                if body.first() == Some(&'!') {
                    // The marker becomes a caret in the
                    // expression, so a caret standing after it is
                    // a member, not a second marker.
                    negated = true;
                    body.remove(0);
                } else if body.first() == Some(&'^') {
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
                pieces.push(Piece::Literal(ch));
                index += 1;
            }
        }
    }
    Pattern { pieces, never }
}

/// Read a class body into singles and ranges. Every character
/// stands as one member token, a backslash included, as the
/// doubling in the Python translation leaves it. A hyphen
/// between two members joins them into a range. A reversed
/// range marks the body bad.
fn read_class_body(body: &[char]) -> (Vec<char>, Vec<(char, char)>, bool) {
    let mut singles = Vec::new();
    let mut ranges = Vec::new();
    let mut bad_range = false;
    let mut index = 0;
    while index < body.len() {
        if index + 2 < body.len() && body[index + 1] == '-' {
            let (low, high) = (body[index], body[index + 2]);
            if low > high {
                bad_range = true;
            } else {
                ranges.push((low, high));
            }
            index += 3;
        } else {
            singles.push(body[index]);
            index += 1;
        }
    }
    (singles, ranges, bad_range)
}

fn class_matches(piece: &Piece, ch: char) -> bool {
    match piece {
        Piece::Class {
            negated,
            singles,
            ranges,
        } => {
            let inside =
                singles.contains(&ch) || ranges.iter().any(|(low, high)| *low <= ch && ch <= *high);
            inside != *negated
        }
        _ => false,
    }
}

/// Whether the pattern matches the whole path.
pub fn matches(pattern: &Pattern, path: &str) -> bool {
    if pattern.never {
        return false;
    }
    let chars: Vec<char> = path.chars().collect();
    match_pieces(&pattern.pieces, &chars)
}

fn match_pieces(pieces: &[Piece], chars: &[char]) -> bool {
    if pieces.is_empty() {
        return chars.is_empty();
    }
    match &pieces[0] {
        Piece::Literal(want) => {
            !chars.is_empty() && chars[0] == *want && match_pieces(&pieces[1..], &chars[1..])
        }
        Piece::Question => {
            !chars.is_empty() && chars[0] != '/' && match_pieces(&pieces[1..], &chars[1..])
        }
        Piece::Class { .. } => {
            !chars.is_empty()
                && class_matches(&pieces[0], chars[0])
                && match_pieces(&pieces[1..], &chars[1..])
        }
        Piece::Star => {
            // Any run of characters that are not a slash,
            // including the empty run.
            let mut end = 0;
            loop {
                if match_pieces(&pieces[1..], &chars[end..]) {
                    return true;
                }
                if end >= chars.len() || chars[end] == '/' {
                    return false;
                }
                end += 1;
            }
        }
        Piece::StarStar => {
            // Any run of characters that are not a newline,
            // including the empty run.
            let mut end = 0;
            loop {
                if match_pieces(&pieces[1..], &chars[end..]) {
                    return true;
                }
                if end >= chars.len() || chars[end] == '\n' {
                    return false;
                }
                end += 1;
            }
        }
        Piece::StarStarSlash => {
            // The empty run, or a run holding no newline and
            // ending at a slash, which the slash of the piece
            // consumes.
            if match_pieces(&pieces[1..], chars) {
                return true;
            }
            let mut end = 0;
            while end < chars.len() && chars[end] != '\n' {
                if chars[end] == '/' && match_pieces(&pieces[1..], &chars[end + 1..]) {
                    return true;
                }
                end += 1;
            }
            false
        }
    }
}
