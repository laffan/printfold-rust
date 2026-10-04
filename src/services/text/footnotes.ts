/**
 * Footnote helpers needed by the UI. Parsing and pagination of footnotes
 * happen in Rust (`markdown/footnotes.rs`); the editor only needs the
 * shared layout constants and definition stripping for cursor sync.
 */

/** Layout constants shared with the Rust renderers. */
export const FOOTNOTE_RULE_GAP = 4;
export const FOOTNOTE_RULE_THICKNESS = 0.5;
export const FOOTNOTE_RULE_WIDTH_RATIO = 0.3;

const DEF_LINE = /^\[\^([^\]]+)\]:[ \t]*(.*)$/;

/**
 * Remove `[^id]: …` definitions (with indented continuation lines) from
 * markdown, exactly as the engine does before parsing.
 */
export function extractFootnotes(markdown: string): { strippedMarkdown: string } {
  const lines = markdown.split('\n');
  const kept: string[] = [];
  let i = 0;
  while (i < lines.length) {
    if (DEF_LINE.test(lines[i])) {
      let j = i + 1;
      while (j < lines.length) {
        const next = lines[j];
        if (/^[ \t]+\S/.test(next)) {
          j++;
        } else if (next.trim() === '' && j + 1 < lines.length && /^[ \t]+\S/.test(lines[j + 1])) {
          j++;
        } else {
          break;
        }
      }
      i = j;
      continue;
    }
    kept.push(lines[i]);
    i++;
  }
  return { strippedMarkdown: kept.join('\n') };
}
