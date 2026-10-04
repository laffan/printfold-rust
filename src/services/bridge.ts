/**
 * Bridge to the Rust side (Tauri commands in `src-tauri/src/commands`).
 *
 * Every native capability the UI uses goes through this module: layout
 * (reflow), PDF generation, project files, dialogs, fonts and the binary
 * file store. Keeping it in one place documents the IPC surface and makes
 * the rest of the UI independent of Tauri.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { BookletProject, ProjectFile, Signature, FillConfig, PageItem, MarginUnit } from '../types';

export interface PlatformInfo {
  os: string;
  mobile: boolean;
}

export interface FileFilter {
  name: string;
  extensions: string[];
}

export interface PickedFile {
  name: string;
  type: ProjectFile['type'];
  content: string;
  isBase64: boolean;
}

export interface FamilyVariants {
  regular: boolean;
  bold: boolean;
  italic: boolean;
  boldItalic: boolean;
}

export interface RecentEntry {
  id: string;
  name: string;
  path: string;
  lastOpened: number;
}

export interface ProjectLocation {
  name: string;
  path: string;
}

export interface StaticPageData {
  pageNumber: number;
  pageState: 'static' | 'available' | 'text';
  items: PageItem[];
  backgroundFill?: FillConfig;
  customBackgroundImageId?: string;
}

export interface ProjectManifest {
  version: string;
  name: string;
  projectId: string;
  mainDocument: string | null;
  measurementUnit?: MarginUnit;
  outputOptions: BookletProject['outputOptions'];
  layoutOptions: BookletProject['layoutOptions'];
  fontOptions: BookletProject['fontOptions'];
  headerFooter: BookletProject['headerFooter'];
  blankPages: number[];
  files: { id: string; name: string; type: string; path: string; lastModified: number }[];
  fileOrder: string[];
}

export interface ProjectImport {
  manifest: ProjectManifest | null;
  files: ProjectFile[];
  staticPages: StaticPageData[];
}

export interface OpenedProject {
  name: string;
  path: string;
  data: ProjectImport | null;
}

export interface PrerenderPlan {
  overlay: number[];
  background: number[];
}

export interface FlowResult {
  signatures: Signature[];
  totalPages: number;
}

/** The parts of a project the engine reads (no file contents). */
export type ProjectSnapshot = Pick<
  BookletProject,
  'id' | 'name' | 'outputOptions' | 'layoutOptions' | 'fontOptions' | 'headerFooter' | 'signatures' | 'blankPages' | 'staticSpreads'
>;

export function snapshot(project: BookletProject): ProjectSnapshot {
  return {
    id: project.id,
    name: project.name,
    outputOptions: project.outputOptions,
    layoutOptions: project.layoutOptions,
    fontOptions: project.fontOptions,
    headerFooter: project.headerFooter,
    signatures: project.signatures,
    blankPages: project.blankPages,
    staticSpreads: project.staticSpreads,
  };
}

/** File list without binary payloads (the native store already has them). */
export function fileMetadata(project: BookletProject): { id: string; name: string; type: string }[] {
  return project.files.map(f => ({ id: f.id, name: f.name, type: f.type }));
}

/** Percent-encode a header value so non-ASCII file names survive. */
function headerValue(value: string): string {
  return encodeURIComponent(value);
}

function toBytes(data: ArrayBuffer | Uint8Array | number[]): Uint8Array {
  if (data instanceof Uint8Array) return data;
  if (Array.isArray(data)) return new Uint8Array(data);
  return new Uint8Array(data);
}

let platformInfo: PlatformInfo = { os: 'macos', mobile: false };

export const bridge = {
  /** Fetch platform info once at startup (sync access afterwards). */
  async init(): Promise<PlatformInfo> {
    try {
      platformInfo = await invoke<PlatformInfo>('platform_info');
    } catch (e) {
      console.warn('platform_info unavailable (running outside Tauri?)', e);
    }
    document.documentElement.dataset.platform = platformInfo.os;
    if (platformInfo.mobile) document.documentElement.classList.add('is-mobile');
    return platformInfo;
  },

  get platform(): PlatformInfo {
    return platformInfo;
  },

  // ---------------------------------------------------------------- layout

  reflow(markdown: string, project: ProjectSnapshot): Promise<FlowResult> {
    return invoke<FlowResult>('reflow', { markdown, project });
  },

  clearMeasurementCache(): Promise<void> {
    return invoke('clear_measurement_cache');
  },

  // ----------------------------------------------------------------- fonts

  fontsList(): Promise<string[]> {
    return invoke<string[]>('fonts_list');
  },

  fontsVariants(family: string): Promise<FamilyVariants> {
    return invoke<FamilyVariants>('fonts_variants', { family });
  },

  fontsRegister(family: string, content: string): Promise<boolean> {
    return invoke<boolean>('fonts_register', { family, content });
  },

  fontsUnregister(family: string): Promise<void> {
    return invoke('fonts_unregister', { family });
  },

  // ------------------------------------------------------------ file store

  filePut(file: ProjectFile): Promise<void> {
    return invoke('file_put', { id: file.id, name: file.name, fileType: file.type, content: file.content });
  },

  fileRename(id: string, name: string): Promise<void> {
    return invoke('file_rename', { id, name });
  },

  filesRetain(ids: string[]): Promise<void> {
    return invoke('files_retain', { ids });
  },

  // ---------------------------------------------------------------- dialogs

  pickFiles(filters: FileFilter[], multiple: boolean): Promise<PickedFile[]> {
    return invoke<PickedFile[]>('pick_files', { filters, multiple });
  },

  /** Ask where to save `bytes` (macOS save panel / iPadOS "Save to Files"). */
  saveFile(bytes: Uint8Array, fileName: string, filter: FileFilter): Promise<boolean> {
    return invoke<boolean>('save_file', bytes, {
      headers: {
        'x-file-name': headerValue(fileName),
        'x-filter-name': headerValue(filter.name),
        'x-filter-ext': headerValue(filter.extensions.join(',')),
      },
    });
  },

  // --------------------------------------------------------------- projects

  projectNew(name: string): Promise<ProjectLocation | null> {
    return invoke<ProjectLocation | null>('project_new', { name });
  },

  projectOpenDialog(): Promise<OpenedProject | null> {
    return invoke<OpenedProject | null>('project_open_dialog');
  },

  projectOpenPath(path: string): Promise<OpenedProject> {
    return invoke<OpenedProject>('project_open_path', { path });
  },

  projectSave(project: BookletProject): Promise<void> {
    // Binary files travel without content: the native store has them.
    const files = project.files.map(f => (f.isBase64 ? { ...f, content: '' } : f));
    return invoke('project_save', {
      project: {
        id: project.id,
        name: project.name,
        mainDocument: project.mainDocument,
        measurementUnit: project.measurementUnit,
        outputOptions: project.outputOptions,
        layoutOptions: project.layoutOptions,
        fontOptions: project.fontOptions,
        headerFooter: project.headerFooter,
        blankPages: project.blankPages,
        signatures: project.signatures,
        files,
      },
    });
  },

  projectClose(): Promise<void> {
    return invoke('project_close');
  },

  // ---------------------------------------------------------------- recents

  recentsList(): Promise<RecentEntry[]> {
    return invoke<RecentEntry[]>('recents_list');
  },

  recentsAdd(path: string, name: string): Promise<void> {
    return invoke('recents_add', { path, name });
  },

  recentsRemove(path: string): Promise<void> {
    return invoke('recents_remove', { path });
  },

  // -------------------------------------------------------------------- PDF

  pdfPrerenderPlan(project: ProjectSnapshot): Promise<PrerenderPlan> {
    return invoke<PrerenderPlan>('pdf_prerender_plan', { project });
  },

  pdfPutPrerendered(pageNumber: number, png: Uint8Array, layer: 'overlay' | 'background' = 'overlay'): Promise<void> {
    return invoke('pdf_put_prerendered', png, { headers: { 'x-page': String(pageNumber), 'x-layer': layer } });
  },

  pdfClearPrerendered(): Promise<void> {
    return invoke('pdf_clear_prerendered');
  },

  async pdfGenerate(project: ProjectSnapshot, files: { id: string; name: string; type: string }[]): Promise<Uint8Array> {
    return toBytes(await invoke<ArrayBuffer>('pdf_generate', { project, files }));
  },

  async pdfTestPage(project: ProjectSnapshot): Promise<Uint8Array> {
    return toBytes(await invoke<ArrayBuffer>('pdf_test_page', { project }));
  },

  // ------------------------------------------------- OS file-open hand-off

  takePendingOpens(): Promise<string[]> {
    return invoke<string[]>('take_pending_opens');
  },

  onOpenProject(handler: (path: string) => void): Promise<UnlistenFn> {
    return listen<string>('open-project', e => handler(e.payload));
  },

  /** macOS menu bar actions (item ids from `src-tauri/src/menu.rs`). */
  onMenu(handler: (id: string) => void): Promise<UnlistenFn> {
    return listen<string>('menu', e => handler(e.payload));
  },
};
