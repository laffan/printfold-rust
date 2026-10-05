/**
 * Getting one image into the project from a chosen source — the dropdown
 * behind "Add Background" and the image tool:
 *
 *   File            document picker / open panel
 *   Photo           photo library (iPadOS) / Pictures folder (macOS)
 *   From Clipboard  the image on the clipboard
 *
 * The result is a project file (formats the PDF engine can't embed are
 * converted) that the caller adds to the project.
 */

import { bridge } from './bridge';
import { normalizePickedFiles, PICKER_EXTENSIONS } from './fileImport';
import { readClipboard } from './clipboardImport';
import { showAlert } from './dialogs';
import type { ProjectFile } from '../types';

export type ImageSource = 'file' | 'photo' | 'clipboard';

/** Labels for a dropdown, in display order. */
export const IMAGE_SOURCES: { source: ImageSource; label: string }[] = [
  { source: 'file', label: 'File' },
  { source: 'photo', label: 'Photo' },
  { source: 'clipboard', label: 'From Clipboard' },
];

const IMAGE_EXTS = PICKER_EXTENSIONS.filter(e => !['md', 'markdown', 'txt', 'ttf', 'otf', 'woff'].includes(e));

/**
 * Ask for an image. Call straight from the click: the clipboard source
 * needs the user's gesture on iPadOS. Resolves to null when cancelled or
 * when the source holds no usable image (the user is told why).
 */
export async function pickImage(source: ImageSource): Promise<ProjectFile | null> {
  if (source === 'clipboard') {
    const result = await readClipboard();
    const image = result.files.find(f => f.type === 'image');
    if (!image) await showAlert('There is no image on the clipboard.');
    return image ?? null;
  }
  // On iPadOS an image-only filter always opens the photo library, so the
  // File source shows every file and checks the choice afterwards.
  const filters = source === 'photo' ? [{ name: 'Images', extensions: IMAGE_EXTS }] : [];
  const picked = await bridge.pickFiles(filters, false, source === 'photo' ? 'photo' : 'document');
  if (picked.length === 0) return null;
  const normalized = await normalizePickedFiles(picked.map(p => ({
    id: crypto.randomUUID(),
    name: p.name,
    type: p.type,
    content: p.content,
    isBase64: p.isBase64,
    lastModified: Date.now(),
  })));
  const image = normalized.files.find(f => f.type === 'image');
  if (!image) await showAlert(`“${picked[0].name}” is not an image PrintFold can use.`);
  return image ?? null;
}
