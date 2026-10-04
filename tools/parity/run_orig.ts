import { parseMarkdownWithFootnotes, parseInlineMarkdown } from './orig/textFlow/parsing';
import * as fs from 'fs';
const corpus: string[] = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const out = corpus.map(md => {
  const p = parseMarkdownWithFootnotes(md);
  return {
    sections: p.sections.map(s => ({ type: s.type, level: s.level ?? null, content: s.content, raw: s.rawMarkdown, imageRef: s.imageRef ?? null, refs: s.footnoteRefs ?? null })),
    footnotes: p.footnotes.map(f => ({ id: f.id, number: f.number, content: f.content })),
    inline: md.split('\n').filter(l => l.trim() && !l.startsWith('#') && !l.startsWith('>') && !l.startsWith('-') && !l.startsWith('[^')).slice(0, 3).map(l => parseInlineMarkdown(l.trim()).map(s => ({ t: s.text, b: !!s.bold, i: !!s.italic, c: !!s.code, s: !!s.strikethrough, h: !!s.highlight, f: s.footnoteNumber ?? null }))),
  };
});
console.log(JSON.stringify(out));
