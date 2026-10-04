//! Dump parser output for a JSON array of markdown documents (used to
//! compare against the original TypeScript parser; see docs/parity.md).

use printfold_core::markdown::{parse_inline_markdown, parse_markdown_with_footnotes};
use serde_json::{json, Value};

fn main() {
    let path = std::env::args().nth(1).expect("usage: parse_dump <corpus.json>");
    let corpus: Vec<String> = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let out: Vec<Value> = corpus
        .iter()
        .map(|md| {
            let p = parse_markdown_with_footnotes(md);
            let sections: Vec<Value> = p
                .sections
                .iter()
                .map(|s| json!({"type": s.section_type, "level": s.level, "content": s.content, "raw": s.raw_markdown, "imageRef": s.image_ref, "refs": s.footnote_refs}))
                .collect();
            let footnotes: Vec<Value> = p.footnotes.iter().map(|f| json!({"id": f.id, "number": f.number, "content": f.content})).collect();
            let inline: Vec<Value> = md
                .split('\n')
                .filter(|l| !l.trim().is_empty() && !l.starts_with('#') && !l.starts_with('>') && !l.starts_with('-') && !l.starts_with("[^"))
                .take(3)
                .map(|l| {
                    Value::Array(
                        parse_inline_markdown(l.trim())
                            .iter()
                            .map(|s| json!({"t": s.text, "b": s.bold, "i": s.italic, "c": s.code, "s": s.strikethrough, "h": s.highlight, "f": s.footnote_number}))
                            .collect(),
                    )
                })
                .collect();
            json!({"sections": sections, "footnotes": footnotes, "inline": inline})
        })
        .collect();
    println!("{}", serde_json::to_string(&out).unwrap());
}
