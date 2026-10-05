/**
 * Project session: the open project's `.printfold` file — creating,
 * opening, renaming and closing it, auto-save, the native file store
 * mirror, and the cover thumbnail shown in the project browser.
 *
 * App.ts owns the UI; this module owns the file.
 */

import { appState } from '../services/state';
import { bridge, type OpenedProject } from '../services/bridge';
import { importProject, saveProject } from '../services/projectIO';
import { projectFile } from '../services/projectFile';
import { renderPageToImage } from '../services/pageRenderer';
import { calculatePageDimensions } from '../services/pageGeometry';
import { dataUrlToBytes } from '../services/pdfExport';
import { showPrompt } from '../services/dialogs';
import type { PageContent, ProjectFile } from '../types';

const AUTOSAVE_DEBOUNCE_MS = 600;
/** Re-render the cover thumbnail at most this often during auto-save. */
const THUMBNAIL_INTERVAL_MS = 20_000;
const THUMBNAIL_WIDTH_PX = 360;

export function stripExtension(name: string): string {
  return name.replace(/\.printfold$/i, '');
}

export class ProjectSession {
  private autoSaveTimer: number | null = null;
  /** Suppress auto-save while loading (avoids re-writing the file just read). */
  private suppressAutoSave = false;
  private lastThumbnailAt = 0;
  private syncedFiles = new Map<string, ProjectFile>();
  private fileSync: Promise<void> = Promise.resolve();

  constructor(private readonly reflow: () => Promise<void>) {}

  setup(): void {
    appState.onProjectChange(() => {
      if (this.suppressAutoSave || !projectFile.hasFile()) return;
      this.scheduleAutoSave();
    });
    appState.onProjectChange((project, prev) => {
      if (project.files !== prev.files) this.fileSync = this.fileSync.then(() => this.syncFiles(project.files));
    });
  }

  get isOpen(): boolean {
    return projectFile.hasFile();
  }

  // ------------------------------------------------------------ lifecycle

  /** New, empty project in the library ("Untitled", "Untitled 2", …). */
  async create(): Promise<void> {
    await this.close();
    const dest = await bridge.libraryCreate();
    this.withoutAutoSave(() => {
      this.markFilesSynced([]);
      appState.reset();
      appState.updateProject({ name: stripExtension(dest.name) });
      projectFile.bind(dest.path, dest.name);
    });
    this.lastThumbnailAt = 0;
    this.updateHeader();
    await this.reflow();
    await this.saveNow(true);
  }

  async openPath(path: string): Promise<void> {
    // Release the current file first: Rust binds the new one while reading.
    await this.close();
    await this.load(await bridge.projectOpenPath(path));
  }

  /** macOS: open a project anywhere on disk (it stays where it is). */
  async openDialog(): Promise<boolean> {
    await this.close();
    const opened = await bridge.projectOpenDialog();
    if (!opened) return false;
    await this.load(opened);
    return true;
  }

  private async load(opened: OpenedProject): Promise<void> {
    this.suppressAutoSave = true;
    try {
      if (!opened.data) {
        // Empty file (created but never edited): blank project.
        this.markFilesSynced([]);
        appState.reset();
      } else {
        // Binary files were loaded into the native store while decoding.
        this.markFilesSynced(opened.data.files);
        await importProject(opened.data);
      }
      appState.updateProject({ name: stripExtension(opened.name) });
      projectFile.bind(opened.path, opened.name);
    } finally {
      this.suppressAutoSave = false;
    }
    this.lastThumbnailAt = Date.now();
    this.updateHeader();
    await this.reflow();
  }

  /**
   * Write pending changes (with a fresh thumbnail) and release the file,
   * so the browser can rename, delete or share it safely.
   */
  async close(): Promise<void> {
    if (!projectFile.hasFile()) return;
    if (this.autoSaveTimer !== null) {
      clearTimeout(this.autoSaveTimer);
      this.autoSaveTimer = null;
    }
    await this.saveNow(true);
    await bridge.projectClose();
    projectFile.clear();
    this.withoutAutoSave(() => {
      this.markFilesSynced([]);
      appState.reset();
    });
    this.updateHeader();
  }

  /** Rename the open project's file (header name click). */
  async renameCurrent(): Promise<void> {
    const path = projectFile.getPath();
    if (!path) return;
    const name = await showPrompt('Rename Project', 'Project name', stripExtension(projectFile.getName()), 'Rename');
    if (name === null) return;
    const loc = await bridge.libraryRename(path, name);
    projectFile.bind(loc.path, loc.name);
    appState.updateProject({ name: stripExtension(loc.name) });
    this.updateHeader();
  }

  updateHeader(): void {
    const display = document.getElementById('project-name-display');
    if (display) display.textContent = projectFile.hasFile() ? stripExtension(projectFile.getName()) : '';
  }

  // ------------------------------------------------------------ auto-save

  private withoutAutoSave(fn: () => void): void {
    this.suppressAutoSave = true;
    try {
      fn();
    } finally {
      this.suppressAutoSave = false;
    }
  }

  private scheduleAutoSave(): void {
    if (this.autoSaveTimer !== null) clearTimeout(this.autoSaveTimer);
    this.setSaveStatus('Saving…', 'saving');
    this.autoSaveTimer = window.setTimeout(() => {
      this.autoSaveTimer = null;
      void this.saveNow();
    }, AUTOSAVE_DEBOUNCE_MS);
  }

  async saveNow(withThumbnail = false): Promise<void> {
    if (!projectFile.hasFile()) return;
    try {
      await this.syncFilesNow();
      if (withThumbnail || Date.now() - this.lastThumbnailAt > THUMBNAIL_INTERVAL_MS) {
        await this.updateThumbnail();
      }
      await saveProject();
      this.setSaveStatus('Saved', '');
    } catch (e) {
      console.error('Auto-save failed:', e);
      this.setSaveStatus('Save failed', 'error');
    }
  }

  private setSaveStatus(text: string, cls: 'saving' | 'error' | ''): void {
    const el = document.getElementById('save-status');
    if (!el) return;
    el.textContent = text;
    el.classList.remove('saving', 'error');
    if (cls) el.classList.add(cls);
  }

  /** Render the cover (first page) small and hand it to Rust for the next save. */
  private async updateThumbnail(): Promise<void> {
    this.lastThumbnailAt = Date.now();
    const project = appState.getProject();
    let cover: PageContent | null = null;
    for (const sig of project.signatures) {
      for (const spread of sig.spreads) {
        for (const page of [spread.verso, spread.recto]) {
          if (page && (!cover || page.pageNumber < cover.pageNumber)) cover = page;
        }
      }
    }
    if (!cover) return;
    try {
      const dims = calculatePageDimensions(project.outputOptions, project.layoutOptions, project.headerFooter);
      const dataUrl = await renderPageToImage(cover, dims.width, dims.height, null, {
        includeTextContent: true,
        includeBackground: true,
        opaque: true,
        scale: THUMBNAIL_WIDTH_PX / dims.width,
      });
      if (dataUrl) await bridge.projectSetThumbnail(dataUrlToBytes(dataUrl));
    } catch (e) {
      console.warn('Thumbnail rendering failed:', e);
    }
  }

  // ---------------------------------------------------- native file store
  // Binary files (images, fonts) are mirrored to Rust once, so saves,
  // reflows and exports don't resend them.

  markFilesSynced(files: ProjectFile[]): void {
    this.syncedFiles.clear();
    for (const f of files) if (f.isBase64) this.syncedFiles.set(f.id, f);
  }

  syncFilesNow(): Promise<void> {
    this.fileSync = this.fileSync.then(() => this.syncFiles(appState.getProject().files));
    return this.fileSync;
  }

  private async syncFiles(files: ProjectFile[]): Promise<void> {
    try {
      const binary = files.filter(f => f.isBase64);
      for (const file of binary) {
        const known = this.syncedFiles.get(file.id);
        if (known === file) continue;
        if (known && known.content === file.content) {
          if (known.name !== file.name) await bridge.fileRename(file.id, file.name);
        } else {
          await bridge.filePut(file);
        }
        this.syncedFiles.set(file.id, file);
      }
      const ids = binary.map(f => f.id);
      if (this.syncedFiles.size !== ids.length) {
        for (const id of Array.from(this.syncedFiles.keys())) {
          if (!ids.includes(id)) this.syncedFiles.delete(id);
        }
        await bridge.filesRetain(ids);
      }
    } catch (e) {
      console.error('File store sync failed:', e);
    }
  }
}
