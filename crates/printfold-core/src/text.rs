//! Small text utilities shared by parsing, measurement and rendering.

/// Whitespace as matched by JavaScript's `\s` (the original engine split
/// words with `/\s+/`; keeping the same character class keeps line breaks
/// identical).
pub fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{000B}' | '\u{000C}' | '\r' | ' ' | '\u{00A0}' | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}' | '\u{2029}' | '\u{202F}' | '\u{205F}' | '\u{3000}' | '\u{FEFF}'
    )
}

/// Equivalent of JS `text.split(/\s+/)`: leading/trailing whitespace yields
/// an empty first/last element, exactly like the browser does.
pub fn split_ws_js(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut in_ws = false;
    let mut ws_start = 0;
    for (i, c) in text.char_indices() {
        if is_js_space(c) {
            if !in_ws {
                in_ws = true;
                ws_start = i;
            }
        } else if in_ws {
            parts.push(&text[start..ws_start]);
            start = i;
            in_ws = false;
        }
    }
    if in_ws {
        parts.push(&text[start..ws_start]);
        parts.push("");
    } else {
        parts.push(&text[start..]);
    }
    parts
}

/// Words of a string with empty pieces removed (JS
/// `split(/\s+/).filter(Boolean)`).
pub fn words(text: &str) -> Vec<&str> {
    text.split(is_js_space).filter(|w| !w.is_empty()).collect()
}

/// Apply a CSS-style `text-transform` (`none`, `uppercase`, `lowercase`,
/// `capitalize`). Capitalize upper-cases the first letter of every word; a
/// word starts at the beginning of the string or after any character that is
/// neither alphanumeric nor an apostrophe (so "l'été" → "L'été", not the
/// "L'éTé" a JS `\b` regex produces).
pub fn apply_text_transform(text: &str, transform: Option<&str>) -> String {
    match transform {
        Some("uppercase") => text.to_uppercase(),
        Some("lowercase") => text.to_lowercase(),
        Some("capitalize") => capitalize_words(text),
        _ => text.to_string(),
    }
}

fn capitalize_words(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_word_start = true;
    for c in text.chars() {
        if c.is_alphabetic() && at_word_start {
            out.extend(c.to_uppercase());
        } else {
            out.push(c);
        }
        at_word_start = !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}' || c == '_');
    }
    out
}

/// Number of UTF-16 code units (JS `string.length`), used when the frontend
/// needs character offsets compatible with JavaScript strings.
/// Remove `==highlight==` markers (in pairs, per line) from plain text.
/// Plain `content` keeps them; text drawn without spans must not show them.
pub fn strip_highlight_markers(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let parts: Vec<&str> = line.split("==").collect();
            let pairs = (parts.len() - 1) / 2;
            let mut out = String::with_capacity(line.len());
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    // Markers 1..=2*pairs are removed; an unpaired last one stays.
                    if i > pairs * 2 {
                        out.push_str("==");
                    }
                }
                out.push_str(part);
            }
            out
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_paired_highlight_markers() {
        assert_eq!(strip_highlight_markers("a ==b c== d"), "a b c d");
        assert_eq!(strip_highlight_markers("x == y"), "x == y");
        assert_eq!(strip_highlight_markers("==a== ==b== ==c"), "a b ==c");
        assert_eq!(strip_highlight_markers("==a==\nb"), "a\nb");
    }

    #[test]
    fn js_split_semantics() {
        assert_eq!(split_ws_js("a b"), vec!["a", "b"]);
        assert_eq!(split_ws_js("  a  b "), vec!["", "a", "b", ""]);
        assert_eq!(split_ws_js(""), vec![""]);
        assert_eq!(split_ws_js("   "), vec!["", ""]);
        assert_eq!(words("  a  b "), vec!["a", "b"]);
    }

    #[test]
    fn transforms() {
        assert_eq!(apply_text_transform("hello world", Some("capitalize")), "Hello World");
        assert_eq!(apply_text_transform("l'été", Some("capitalize")), "L'été");
        assert_eq!(apply_text_transform("Ab", Some("uppercase")), "AB");
        assert_eq!(apply_text_transform("Ab", Some("none")), "Ab");
    }
}
