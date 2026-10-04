//! Markdown parsing: blocks → sections, inline → styled spans, footnotes.

pub mod blocks;
pub mod footnotes;
pub mod inline;

pub use blocks::{parse_markdown, parse_markdown_with_footnotes, strip_blockquote_markers, ParsedMarkdown};
pub use inline::{inline_plain_text, merge_adjacent_spans, parse_inline_markdown};
