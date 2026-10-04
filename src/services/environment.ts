/**
 * Environment abstraction.
 *
 * The original app had Web and Electron implementations behind this API;
 * the port has a single native (Tauri) implementation backed by
 * `bridge.ts`. Keeping the same `env` surface keeps components unchanged.
 */

import { bridge, type FileFilter } from './bridge';
import type { ProjectFile } from '../types';

export type { FileFilter };

export interface SaveFileOptions {
  defaultName?: string;
  filters?: FileFilter[];
  content: Uint8Array | string;
}

export interface OpenFilesOptions {
  filters?: FileFilter[];
  multiple?: boolean;
}

function filterFor(fileName: string, filters?: FileFilter[]): FileFilter {
  if (filters && filters.length > 0) return filters[0];
  const ext = fileName.split('.').pop()?.toLowerCase() || '';
  const names: Record<string, string> = {
    pdf: 'PDF Document',
    png: 'PNG Image',
    svg: 'SVG Image',
    md: 'Markdown',
    printfold: 'PrintFold Project',
  };
  return { name: names[ext] || 'File', extensions: ext ? [ext] : [] };
}

export const env = {
  /** Always true: the app runs inside the native Tauri shell. */
  isNative: true,

  /** iPadOS (touch-first) vs macOS. */
  get isMobile(): boolean {
    return bridge.platform.mobile;
  },

  /** Pick files and read them as project files. */
  async openFiles(options?: OpenFilesOptions): Promise<ProjectFile[] | null> {
    const picked = await bridge.pickFiles(options?.filters ?? [], options?.multiple ?? true);
    if (picked.length === 0) return null;
    return picked.map(p => ({
      id: crypto.randomUUID(),
      name: p.name,
      type: p.type,
      content: p.content,
      isBase64: p.isBase64,
      lastModified: Date.now(),
    }));
  },

  /** Ask where to save and write the content. Returns false if cancelled. */
  async saveFile(options: SaveFileOptions): Promise<boolean> {
    const name = options.defaultName || 'export';
    const bytes = typeof options.content === 'string' ? new TextEncoder().encode(options.content) : options.content;
    return bridge.saveFile(bytes, name, filterFor(name, options.filters));
  },

  /** Save a generated file (PNG/SVG exports etc.) via the save dialog. */
  async downloadFile(filename: string, content: Uint8Array | Blob): Promise<void> {
    const bytes = content instanceof Blob ? new Uint8Array(await content.arrayBuffer()) : content;
    await bridge.saveFile(bytes, filename, filterFor(filename));
  },
};
