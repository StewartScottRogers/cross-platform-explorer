//! macOS Finder-tag codec (CPE-829, epic CPE-717): the `com.apple.metadata:_kMDItemUserTags` extended
//! attribute, in which Finder stores a file's user tags as a **binary property list** — an array of
//! strings, each `"Name"` or `"Name\n<colorIndex>"` (colour 0–7).
//!
//! This module encodes/decodes `Vec<`[`FinderTag`]`> ⇄ bplist bytes` (via the pure-Rust `plist` crate)
//! and projects Finder tags down to CPE tag names for the CPE-827 reconciliation. It is cross-platform
//! code — built and round-trip-tested on every OS — so it's fully verifiable headlessly.
//!
//! **Real-Finder byte-compat** (that macOS Finder reads what we write, and we read what Finder writes)
//! can only be confirmed on a Mac; that interop check rides the attended CPE-828 wiring. Here we prove
//! the codec round-trips through the genuine binary-plist format and parses the documented tag shape.

use plist::Value;

/// The extended-attribute name macOS Finder stores user tags under.
pub const FINDER_TAGS_XATTR: &str = "com.apple.metadata:_kMDItemUserTags";

/// One macOS Finder tag: a name plus a colour index. `0` = no colour; `1` grey, `2` green, `3` purple,
/// `4` blue, `5` yellow, `6` red, `7` orange (the Finder palette).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinderTag {
    pub name: String,
    pub color: u8,
}

impl FinderTag {
    pub fn new(name: impl Into<String>, color: u8) -> Self {
        Self {
            name: name.into(),
            color,
        }
    }

    /// Finder's on-disk string form: `"Name\n<color>"`, or just `"Name"` when uncoloured.
    fn to_wire(&self) -> String {
        if self.color == 0 {
            self.name.clone()
        } else {
            format!("{}\n{}", self.name, self.color)
        }
    }

    /// Parse Finder's `"Name\n<color>"` string form. A missing or non-numeric colour is `0`.
    fn from_wire(s: &str) -> Self {
        match s.split_once('\n') {
            Some((name, color)) => Self {
                name: name.to_string(),
                color: color.trim().parse().unwrap_or(0),
            },
            None => Self {
                name: s.to_string(),
                color: 0,
            },
        }
    }
}

/// Encode Finder tags as the `_kMDItemUserTags` binary plist (an array of `"name\ncolor"` strings).
pub fn encode(tags: &[FinderTag]) -> Vec<u8> {
    let arr = Value::Array(tags.iter().map(|t| Value::String(t.to_wire())).collect());
    let mut buf = Vec::new();
    // Encoding a plain string array can't realistically fail; degrade to an empty-array plist rather
    // than surface an error to a metadata write.
    if plist::to_writer_binary(&mut buf, &arr).is_err() {
        buf.clear();
        let _ = plist::to_writer_binary(&mut buf, &Value::Array(Vec::new()));
    }
    buf
}

/// Decode a `_kMDItemUserTags` plist back to Finder tags. **Lenient**: a non-plist / foreign / empty
/// blob yields an empty list rather than an error, so a stray xattr never fails a listing. Only string
/// array elements are taken; any other shape is ignored.
pub fn decode(blob: &[u8]) -> Vec<FinderTag> {
    let value = match Value::from_reader(std::io::Cursor::new(blob)) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|v| v.as_string().map(|s| s.to_string()))
            .map(|s| FinderTag::from_wire(&s))
            .collect(),
        _ => Vec::new(),
    }
}

/// Project Finder tags down to plain CPE tag names (dropping colours) for reconciliation (CPE-827).
pub fn names(tags: &[FinderTag]) -> Vec<String> {
    tags.iter().map(|t| t.name.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_tags_with_and_without_colour() {
        let tags = vec![
            FinderTag::new("Red", 6),
            FinderTag::new("No Colour", 0),
            FinderTag::new("Two Words", 4),
        ];
        let blob = encode(&tags);
        assert_eq!(decode(&blob), tags);
    }

    #[test]
    fn empty_round_trips_to_empty() {
        assert!(decode(&encode(&[])).is_empty());
    }

    #[test]
    fn decode_is_lenient_on_non_plist() {
        assert!(decode(b"not a plist at all").is_empty());
        assert!(decode(b"").is_empty());
    }

    #[test]
    fn wire_form_matches_finder_convention() {
        assert_eq!(FinderTag::new("Red", 6).to_wire(), "Red\n6");
        assert_eq!(FinderTag::new("Plain", 0).to_wire(), "Plain");
        assert_eq!(FinderTag::from_wire("Blue\n4"), FinderTag::new("Blue", 4));
        assert_eq!(FinderTag::from_wire("Naked"), FinderTag::new("Naked", 0));
        // A non-numeric colour degrades to 0 rather than panicking.
        assert_eq!(FinderTag::from_wire("Weird\nx"), FinderTag::new("Weird", 0));
    }

    #[test]
    fn names_projects_dropping_colour() {
        let tags = vec![FinderTag::new("a", 6), FinderTag::new("b", 0)];
        assert_eq!(names(&tags), vec!["a".to_string(), "b".to_string()]);
    }
}
