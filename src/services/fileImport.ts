/**
 * Turn files dropped from other apps (Finder, Files, Photos …) into project
 * files. The type comes from the extension, falling back to the MIME type
 * (iPadOS drags don't always carry a useful name). Images in formats the
 * PDF engine can't embed (HEIC from Photos, GIF, TIFF, BMP …) are
 * converted to JPEG through WebKit's own decoder.
 */

import type { ProjectFile } from '../types';

const TEXT_EXTS = ['md', 'markdown', 'txt'];
const IMAGE_EXTS = ['png', 'jpg', 'jpeg', 'webp'];
const FONT_EXTS = ['ttf', 'otf', 'woff'];

export interface DropImport {
  files: ProjectFile[];
  /** Names of files that could not be used. */
  skipped: string[];
}

function extensionOf(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : '';
}

function readAsText(file: Blob): Promise<string> {
  return file.text();
}

async function readAsBase64(file: Blob): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

/** Re-encode any image WebKit can decode as JPEG (white behind transparency). */
async function convertToJpeg(file: File): Promise<Blob | null> {
  try {
    const bitmap = await createImageBitmap(file);
    const canvas = document.createElement('canvas');
    canvas.width = bitmap.width;
    canvas.height = bitmap.height;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(bitmap, 0, 0);
    bitmap.close();
    return await new Promise<Blob | null>(resolve => canvas.toBlob(resolve, 'image/jpeg', 0.92));
  } catch {
    return null;
  }
}

function baseName(name: string): string {
  const dot = name.lastIndexOf('.');
  return dot > 0 ? name.slice(0, dot) : name || 'image';
}

export async function importDroppedFiles(list: FileList | File[]): Promise<DropImport> {
  const files: ProjectFile[] = [];
  const skipped: string[] = [];
  for (const file of Array.from(list)) {
    const ext = extensionOf(file.name);
    const mime = file.type.toLowerCase();
    const base = { id: crypto.randomUUID(), lastModified: file.lastModified || Date.now() };
    try {
      if (TEXT_EXTS.includes(ext) || mime === 'text/markdown' || (mime === 'text/plain' && !ext)) {
        const name = ext === 'md' ? file.name : `${baseName(file.name)}.md`;
        files.push({ ...base, name, type: 'markdown', content: await readAsText(file), isBase64: false });
      } else if (IMAGE_EXTS.includes(ext)) {
        files.push({ ...base, name: file.name, type: 'image', content: await readAsBase64(file), isBase64: true });
      } else if (FONT_EXTS.includes(ext)) {
        files.push({ ...base, name: file.name, type: 'font', content: await readAsBase64(file), isBase64: true });
      } else if (mime.startsWith('image/') || ['heic', 'heif', 'gif', 'tif', 'tiff', 'bmp'].includes(ext)) {
        const jpeg = await convertToJpeg(file);
        if (!jpeg) {
          skipped.push(file.name || 'image');
          continue;
        }
        files.push({ ...base, name: `${baseName(file.name)}.jpg`, type: 'image', content: await readAsBase64(jpeg), isBase64: true });
      } else {
        skipped.push(file.name || 'file');
      }
    } catch (e) {
      console.warn(`Could not read ${file.name}:`, e);
      skipped.push(file.name || 'file');
    }
  }
  return { files, skipped };
}

export function skippedMessage(skipped: string[]): string {
  const list = skipped.slice(0, 5).join(', ') + (skipped.length > 5 ? ` and ${skipped.length - 5} more` : '');
  return `Skipped ${list}. PrintFold accepts Markdown (.md, .txt), images (PNG, JPEG, WebP; other image formats are converted) and fonts (.ttf, .otf, .woff).`;
}

/** File-picker extensions: everything `importDroppedFiles` understands. */
export const PICKER_EXTENSIONS = [
  ...TEXT_EXTS, ...IMAGE_EXTS, ...FONT_EXTS, 'heic', 'heif', 'gif', 'tif', 'tiff', 'bmp',
];

/** Convert picked files the engine can't use directly (HEIC, TIFF …). */
export async function normalizePickedFiles(picked: ProjectFile[]): Promise<DropImport> {
  const ready = picked.filter(f => f.type !== 'unknown');
  const convert = picked.filter(f => f.type === 'unknown' && f.isBase64);
  const skipped = picked.filter(f => f.type === 'unknown' && !f.isBase64).map(f => f.name);
  if (convert.length === 0) return { files: ready, skipped };
  const blobs = convert.map(f => {
    const binary = atob(f.content);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
    return new File([bytes], f.name, { lastModified: f.lastModified });
  });
  const converted = await importDroppedFiles(blobs);
  return { files: [...ready, ...converted.files], skipped: [...skipped, ...converted.skipped] };
}
