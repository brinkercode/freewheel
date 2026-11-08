//! The icons a profile can have: every Material Symbols Rounded icon the app
//! bundles (src/assets/symbols/symbols.woff2), by its Material name, like
//! `edit` or `sports_esports`. A name may be written `:edit:` too.
//!
//! icon_names.txt lists them, one `name codepoint` per line, sorted; the
//! font and the list come from scripts/icons/build-icon-font.py, and the
//! settings window reads the same list (src/symbols.js).

use std::sync::OnceLock;

/// Every icon name and its codepoint in the bundled font, sorted by name.
const NAMES: &str = include_str!("icon_names.txt");

/// Where to see every icon (the Rounded style the app draws).
pub const ICON_CATALOGUE: &str = "https://fonts.google.com/icons?icon.style=Rounded";

/// Icon names, sorted.
pub fn names() -> impl Iterator<Item = &'static str> {
    NAMES
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(n, _)| n)
}

/// The bare name `text` refers to (`edit`, `:edit:`, ` Edit `), if the
/// bundled font has it.
pub fn icon_name(text: &str) -> Option<&'static str> {
    static SORTED: OnceLock<Vec<&'static str>> = OnceLock::new();
    let sorted = SORTED.get_or_init(|| names().collect());
    let bare = bare(text).to_ascii_lowercase();
    sorted.binary_search(&bare.as_str()).ok().map(|i| sorted[i])
}

/// `text` without its surrounding spaces and `:` marks.
pub fn bare(text: &str) -> &str {
    let t = text.trim();
    let t = t.strip_prefix(':').unwrap_or(t);
    t.strip_suffix(':').unwrap_or(t).trim()
}

/// How the mappings file writes icon `name`: `:name:`.
pub fn written(name: &str) -> String {
    format!(":{name}:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_sorted_unique_and_names_codepoints_in_the_private_use_area() {
        let all: Vec<&str> = names().collect();
        assert!(all.len() > 4000, "{}", all.len());
        assert!(all.windows(2).all(|w| w[0] < w[1]), "sorted, unique");
        for line in NAMES.lines() {
            let (name, cp) = line.split_once(' ').unwrap();
            assert!(
                name.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "{name}"
            );
            let cp = u32::from_str_radix(cp, 16).unwrap();
            assert!(
                (0xE000..=0xF8FF).contains(&cp) || cp >= 0xF0000,
                "{name} {cp:x}"
            );
        }
    }

    #[test]
    fn names_read_bare_or_between_colons_in_any_case() {
        assert_eq!(icon_name("edit"), Some("edit"));
        assert_eq!(icon_name(":edit:"), Some("edit"));
        assert_eq!(icon_name(" :Sports_Esports: "), Some("sports_esports"));
        assert_eq!(icon_name("10k"), Some("10k"));
        assert_eq!(icon_name("zoom_out_map"), Some("zoom_out_map"));
        // One colon left out still reads.
        assert_eq!(icon_name(":edit"), Some("edit"));
        for unknown in ["", "::", "star_wars", "edit pencil", "edit_"] {
            assert_eq!(icon_name(unknown), None, "{unknown}");
        }
        assert_eq!(written("edit"), ":edit:");
    }

    #[test]
    fn the_icons_the_editor_suggests_and_the_ones_1_0_files_used_are_there() {
        for n in [
            "brush",
            "code",
            "music_note",
            "sports_esports",
            "movie",
            "edit",
            "photo_camera",
            "terminal",
            "keyboard",
            "tab",
        ] {
            assert_eq!(icon_name(n), Some(n));
        }
    }
}
