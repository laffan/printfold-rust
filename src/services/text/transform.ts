/**
 * CSS-style text-transform applied to content at render time. Mirrors
 * `apply_text_transform` in the Rust engine (`crates/printfold-core/src/text.rs`)
 * so the editor and the PDF agree.
 */

export type TextTransform = 'none' | 'uppercase' | 'lowercase' | 'capitalize';

/**
 * Capitalize upper-cases the first letter of every word. A word starts at
 * the beginning of the string or after any character that is neither a
 * letter/digit nor an apostrophe — unicode-aware, unlike a `\b` regex.
 */
function capitalizeWords(text: string): string {
  let out = '';
  let atWordStart = true;
  for (const ch of text) {
    out += atWordStart && /\p{L}/u.test(ch) ? ch.toUpperCase() : ch;
    atWordStart = !/[\p{L}\p{N}'’_]/u.test(ch);
  }
  return out;
}

export function applyTextTransform(text: string, transform?: TextTransform | string): string {
  if (!transform || transform === 'none') return text;
  if (transform === 'uppercase') return text.toUpperCase();
  if (transform === 'lowercase') return text.toLowerCase();
  if (transform === 'capitalize') return capitalizeWords(text);
  return text;
}
