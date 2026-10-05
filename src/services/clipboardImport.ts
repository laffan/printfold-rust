/**
 * "From Clipboard": turn what is on the clipboard into project files —
 * text becomes a markdown file, an image a PNG (other formats converted
 * as for dropped files), copied files are imported as they are.
 *
 * Desktop reads the clipboard natively (`clipboard_read`, no prompt).
 * iPadOS uses WebKit's async clipboard API, which shows the system paste
 * prompt and must start within the user's tap — so call `readClipboard`
 * directly from the click handler, before any other `await`.
 */

import { bridge } from './bridge';
import { env } from './environment';
import { importDroppedFiles, normalizePickedFiles, type DropImport } from './fileImport';
import type { ProjectFile } from '../types';

export interface ClipboardImport extends DropImport {
  /** Nothing usable was on the clipboard. */
  empty: boolean;
}

/** A file name from the first line of pasted text ("# Chapter One" → "Chapter One.md"). */
export function nameForText(text: string): string {
  const first = text.split('\n').map(l => l.trim()).find(l => l.length > 0) ?? '';
  let title = first.replace(/^(#{1,6}\s+|[-*+>]\s+|\d+[.)]\s+)/, '').replace(/[*_`~=[\]]/g, '').trim();
  title = title.replace(/[\\/:]/g, '-');
  if (title.length > 40) {
    const cut = title.slice(0, 40);
    title = cut.slice(0, cut.lastIndexOf(' ') > 20 ? cut.lastIndexOf(' ') : 40).trim();
  }
  return `${title || 'Clipboard'}.md`;
}

/** `name`, or "name 2", "name 3" … if a project file already uses it. */
export function uniqueName(name: string, taken: Iterable<string>): string {
  const used = new Set(Array.from(taken, n => n.toLowerCase()));
  if (!used.has(name.toLowerCase())) return name;
  const dot = name.lastIndexOf('.');
  const stem = dot > 0 ? name.slice(0, dot) : name;
  const ext = dot > 0 ? name.slice(dot) : '';
  for (let n = 2; ; n++) {
    const candidate = `${stem} ${n}${ext}`;
    if (!used.has(candidate.toLowerCase())) return candidate;
  }
}

function textFile(text: string): ProjectFile {
  return {
    id: crypto.randomUUID(),
    name: nameForText(text),
    type: 'markdown',
    content: text,
    isBase64: false,
    lastModified: Date.now(),
  };
}

function result(files: ProjectFile[], skipped: string[] = []): ClipboardImport {
  return { files, skipped, empty: files.length === 0 && skipped.length === 0 };
}

/** WebKit async clipboard API (iPadOS; fallback elsewhere). */
async function readWithWebKit(): Promise<ClipboardImport> {
  if (navigator.clipboard?.read) {
    const items = await navigator.clipboard.read();
    for (const item of items) {
      const imageType = item.types.find(t => t.startsWith('image/'));
      if (imageType) {
        const blob = await item.getType(imageType);
        const ext = imageType.split('/')[1]?.replace('jpeg', 'jpg') || 'png';
        const converted = await importDroppedFiles([new File([blob], `Clipboard image.${ext}`, { type: imageType })]);
        return result(converted.files, converted.skipped);
      }
      if (item.types.includes('text/plain')) {
        const text = await (await item.getType('text/plain')).text();
        if (text.trim()) return result([textFile(text)]);
      }
    }
    return result([]);
  }
  const text = await navigator.clipboard.readText();
  return result(text.trim() ? [textFile(text)] : []);
}

export async function readClipboard(): Promise<ClipboardImport> {
  if (env.isMobile) return readWithWebKit();

  const content = await bridge.clipboardRead();
  switch (content.kind) {
    case 'files': {
      const picked: ProjectFile[] = content.files.map(p => ({
        id: crypto.randomUUID(),
        name: p.name,
        type: p.type,
        content: p.content,
        isBase64: p.isBase64,
        lastModified: Date.now(),
      }));
      const normalized = await normalizePickedFiles(picked);
      return result(normalized.files, normalized.skipped);
    }
    case 'image':
      return result([{
        id: crypto.randomUUID(),
        name: 'Clipboard image.png',
        type: 'image',
        content: content.png,
        isBase64: true,
        lastModified: Date.now(),
      }]);
    case 'text':
      return result([textFile(content.text)]);
    case 'unsupported':
      return readWithWebKit();
    default:
      return result([]);
  }
}
