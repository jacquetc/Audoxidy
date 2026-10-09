//! Composite string ids made of several parts joined with `|`, such as the
//! album ids of the library (`artist|album hash`) and the node ids of the
//! filter tree (`Type|L1|L2|L3`).
//!
//! The parts are user data (artist, album, genre or folder names), so they
//! may themselves contain `|`. Inside a part a backslash escapes `\` and `|`;
//! every other character is written as is. An id whose parts contain neither
//! character is therefore the plain `|`-joined text, byte for byte.
//!
//! These ids only live in memory (selection, expansion and collapse state);
//! they are never stored in the database or the configuration.

/// Separator between the parts of a composite id.
pub const SEPARATOR: char = '|';

/// Escape character inside a part.
pub const ESCAPE: char = '\\';

/// Appends `part` to `out` with `\` and `|` escaped.
fn push_escaped(out: &mut String, part: &str) {
    for c in part.chars() {
        if c == SEPARATOR || c == ESCAPE {
            out.push(ESCAPE);
        }
        out.push(c);
    }
}

/// Joins `parts` into a composite id: each part escaped, then joined with
/// `|`. [`split_parts`] gives the parts back for any non-empty list.
pub fn join_parts(parts: &[&str]) -> String {
    let mut id = String::with_capacity(parts.iter().map(|p| p.len() + 1).sum());
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            id.push(SEPARATOR);
        }
        push_escaped(&mut id, part);
    }
    id
}

/// Splits a composite id made by [`join_parts`] into its unescaped parts.
///
/// Splits on every `|` that is not escaped, like `str::split`: an id always
/// has at least one part, and an empty id is one empty part. A backslash at
/// the very end (which [`join_parts`] never produces) is kept as is.
pub fn split_parts(id: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = id.chars();
    while let Some(c) = chars.next() {
        if c == ESCAPE {
            current.push(chars.next().unwrap_or(ESCAPE));
        } else if c == SEPARATOR {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(parts: &[&str]) {
        let id = join_parts(parts);
        assert_eq!(split_parts(&id), parts, "id {id:?}");
    }

    #[test]
    fn plain_names_are_joined_byte_for_byte() {
        assert_eq!(join_parts(&["Queen", "abc123"]), "Queen|abc123");
        assert_eq!(
            join_parts(&["Genre", "Rock", "AC/DC", "Back in Black"]),
            "Genre|Rock|AC/DC|Back in Black"
        );
        assert_eq!(join_parts(&["Ñandú — ok"]), "Ñandú — ok");
        assert_eq!(split_parts("Queen|abc123"), ["Queen", "abc123"]);
    }

    #[test]
    fn a_pipe_and_a_backslash_inside_a_part_are_escaped() {
        assert_eq!(join_parts(&["A|B", "h"]), r"A\|B|h");
        assert_eq!(join_parts(&[r"C:\Music", "h"]), r"C:\\Music|h");
        assert_eq!(join_parts(&[r"a\|b", "|"]), r"a\\\|b|\|");
    }

    #[test]
    fn names_with_pipes_backslashes_or_both_round_trip() {
        round_trip(&["A|B", "hash"]);
        round_trip(&[r"back\slash", "hash"]);
        round_trip(&[r"both\|mixed|", r"\"]);
        round_trip(&["Artist", "A|B", r"C\D", r"E\|F|\"]);
        round_trip(&["|", "||", r"\\", r"\|"]);
        round_trip(&["", "", ""]);
        round_trip(&["only"]);
        round_trip(&[""]);
    }

    #[test]
    fn splitting_follows_str_split_on_unescaped_ids() {
        for id in ["", "a", "a|", "|a", "a||b", "Artist|Album|x"] {
            let expected: Vec<String> = id.split('|').map(str::to_string).collect();
            assert_eq!(split_parts(id), expected, "id {id:?}");
        }
    }

    #[test]
    fn a_trailing_backslash_is_kept() {
        assert_eq!(split_parts(r"a|b\"), ["a", r"b\"]);
    }
}
