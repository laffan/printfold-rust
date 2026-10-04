/**
 * Main Application Component
 * Orchestrates all UI components and handles global interactions
 */

import { appState } from '../services/state';
import { bridge, snapshot } from '../services/bridge';
import { env } from '../services/environment';
import { fontService } from '../services/fontService';
import { FileList } from './FileList';
import { FilePreview } from './FilePreview';
import { SpreadEditor } from './SpreadEditor';
import { PDFPreview } from './PDFPreview';
import { OptionsPanel } from './OptionsPanel';
import { updateStylesTab } from './OptionsPanel/stylesTab';
import { importProject, saveProject } from '../services/projectIO';
import { generatePdf } from '../services/pdfExport';
import { projectFile } from '../services/projectFile';
import { recentProjects, type RecentEntry } from '../services/recentProjects';
import { showAlert, showPrompt } from '../services/dialogs';
import { WelcomeScreen, type WelcomeAction } from './WelcomeScreen';
import type { BookletProject, ProjectFile } from '../types';

const AUTOSAVE_DEBOUNCE_MS = 600;

export class App {
  private fileList!: FileList;
  private filePreview!: FilePreview;
  private spreadEditor!: SpreadEditor;
  private pdfPreview!: PDFPreview;
  private optionsPanel!: OptionsPanel;
  private welcomeScreen!: WelcomeScreen;

  /** Debounced auto-save handle. */
  private autoSaveTimer: number | null = null;
  /** Suppress auto-save while loading an existing project (avoids
   *  re-writing the file we just read). */
  private suppressAutoSave = false;

  /** Reflow bookkeeping: one request in flight, re-run if inputs changed. */
  private reflowInFlight: Promise<void> | null = null;
  private reflowAgain = false;

  init(): void {
    // Initialize components
    this.fileList = new FileList();
    this.filePreview = new FilePreview();
    this.spreadEditor = new SpreadEditor();
    this.pdfPreview = new PDFPreview();
    this.optionsPanel = new OptionsPanel();
    this.welcomeScreen = new WelcomeScreen();

    // Mount components
    this.fileList.mount();
    this.filePreview.mount();
    this.spreadEditor.mount();
    this.pdfPreview.mount();
    this.optionsPanel.mount();
    this.welcomeScreen.mount();
    this.welcomeScreen.setOnAction((action) => this.handleWelcomeAction(action));

    // Connect file list to preview
    this.fileList.setOnFileSelect((file) => {
      this.filePreview.showFile(file);
    });

    // Set up event listeners
    this.setupHeaderButtons();
    this.setupHeaderMenus();
    this.setupTabs();
    this.setupOptionsTabs();
    this.setupCollapsiblePanels();
    this.setupStateListeners();
    this.setupNativeFileStore();
    this.setupResizers();
    this.setupAutoSave();
    void this.setupFileAssociations();
    void this.setupNativeMenu();

    // Show the welcome screen — the user must create or open a project
    // before the editor becomes interactive.
    void this.welcomeScreen.show();

    console.log('PrintFold initialized', bridge.platform);
  }

  // -------------------------------------------------------------------
  // Welcome screen / project file lifecycle
  // -------------------------------------------------------------------

  private async handleWelcomeAction(action: WelcomeAction): Promise<void> {
    try {
      if (action.kind === 'new') {
        await this.createNewProject();
      } else if (action.kind === 'open') {
        await this.openExistingProject();
      } else if (action.kind === 'openRecent') {
        await this.openRecentProject(action.entry);
      }
    } catch (e) {
      console.error('Welcome action failed:', e);
      await showAlert(`Could not open project: ${errorMessage(e)}`);
    }
  }

  private async createNewProject(): Promise<void> {
    // iPadOS has no save panel: projects live in PrintFold's Documents
    // folder, so ask for a name instead.
    let name = 'Untitled';
    if (env.isMobile) {
      const entered = await showPrompt('New Project', 'Project name', 'Untitled', 'Create');
      if (entered === null) return;
      name = entered;
    }
    const dest = await bridge.projectNew(name);
    if (!dest) return;

    // Reset to a fresh project, then bind the file destination and
    // perform the initial write so the .printfold file on disk reflects
    // the empty starting state.
    this.suppressAutoSave = true;
    this.markFilesSynced([]);
    appState.reset();
    appState.updateProject({ name: stripExtension(dest.name) });
    projectFile.bind(dest.path, dest.name);
    this.suppressAutoSave = false;

    this.welcomeScreen.hide();
    this.updateHeaderForProject();
    await this.performReflow();
    await this.saveNow();
  }

  private async openExistingProject(): Promise<void> {
    const opened = await bridge.projectOpenDialog();
    if (!opened) return;
    await this.loadOpenedProject(opened);
  }

  private async openRecentProject(entry: RecentEntry): Promise<void> {
    try {
      const opened = await bridge.projectOpenPath(entry.path);
      await this.loadOpenedProject(opened);
    } catch (e) {
      await recentProjects.remove(entry);
      throw e;
    }
  }

  private async loadOpenedProject(opened: Awaited<ReturnType<typeof bridge.projectOpenPath>>): Promise<void> {
    this.suppressAutoSave = true;
    try {
      // Empty file (e.g. a project created but never edited): blank project.
      if (!opened.data) {
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

    this.welcomeScreen.hide();
    this.updateHeaderForProject();
    await this.performReflow();
  }

  // -------------------------------------------------------------------
  // File associations (.printfold opened from Finder / Files)
  // -------------------------------------------------------------------

  private async setupFileAssociations(): Promise<void> {
    try {
      await bridge.onOpenProject((path) => void this.openProjectFromPath(path));
      const pending = await bridge.takePendingOpens();
      for (const path of pending) await this.openProjectFromPath(path);
    } catch (e) {
      console.warn('File association hand-off unavailable:', e);
    }
  }

  private async openProjectFromPath(path: string): Promise<void> {
    try {
      const opened = await bridge.projectOpenPath(path);
      await this.loadOpenedProject(opened);
    } catch (e) {
      console.error('Open from path failed:', e);
      await showAlert(`Could not open project: ${errorMessage(e)}`);
    }
  }

  /** Route macOS menu bar items to the same actions as the UI buttons. */
  private async setupNativeMenu(): Promise<void> {
    const click = (selector: string) => (document.querySelector(selector) as HTMLElement | null)?.click();
    try {
      await bridge.onMenu((id) => {
        const inEditor = document.body.classList.contains('welcome-active') === false;
        switch (id) {
          case 'new-project': void this.handleWelcomeAction({ kind: 'new' }); break;
          case 'open-project': void this.handleWelcomeAction({ kind: 'open' }); break;
          case 'projects': void this.welcomeScreen.show(); break;
          case 'add-files': if (inEditor) click('#btn-add-files'); break;
          case 'export-pdf': if (inEditor) click('#btn-export'); break;
          case 'toggle-sidebar': click('#btn-toggle-sidebar'); break;
          case 'show-editor': click('.column-header .tab[data-tab="editor"]'); break;
          case 'show-preview': click('.column-header .tab[data-tab="preview"]'); break;
        }
      });
    } catch (e) {
      console.warn('Native menu unavailable:', e);
    }
  }

  private updateHeaderForProject(): void {
    const display = document.getElementById('project-name-display');
    if (display) display.textContent = projectFile.getName();
  }

  // -------------------------------------------------------------------
  // Auto-save
  // -------------------------------------------------------------------

  private setupAutoSave(): void {
    appState.onProjectChange(() => {
      if (this.suppressAutoSave) return;
      if (!projectFile.hasFile()) return;
      this.scheduleAutoSave();
    });
  }

  private scheduleAutoSave(): void {
    if (this.autoSaveTimer !== null) {
      clearTimeout(this.autoSaveTimer);
    }
    this.setSaveStatus('Saving…', 'saving');
    this.autoSaveTimer = window.setTimeout(() => {
      this.autoSaveTimer = null;
      void this.saveNow();
    }, AUTOSAVE_DEBOUNCE_MS);
  }

  private async saveNow(): Promise<void> {
    if (!projectFile.hasFile()) return;
    try {
      await this.syncFilesNow();
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

  // -------------------------------------------------------------------
  // Native file store: binary files (images, fonts) are mirrored to Rust
  // once, so saves, reflows and exports don't resend them.
  // -------------------------------------------------------------------

  private syncedFiles = new Map<string, ProjectFile>();
  private fileSync: Promise<void> = Promise.resolve();

  private markFilesSynced(files: ProjectFile[]): void {
    this.syncedFiles.clear();
    for (const f of files) if (f.isBase64) this.syncedFiles.set(f.id, f);
  }

  private setupNativeFileStore(): void {
    appState.onProjectChange((project, prev) => {
      if (project.files !== prev.files) this.fileSync = this.fileSync.then(() => this.syncFiles(project.files));
    });
  }

  private syncFilesNow(): Promise<void> {
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

  private setupHeaderButtons(): void {
    // Re-open the welcome screen (for switching projects)
    document.getElementById('btn-welcome')?.addEventListener('click', () => {
      void this.welcomeScreen.show();
    });

    // Export PDF button
    document.getElementById('btn-export')?.addEventListener('click', async () => {
      const button = document.getElementById('btn-export') as HTMLButtonElement | null;
      if (button) button.disabled = true;
      try {
        await this.syncFilesNow();
        const pdfBytes = await generatePdf();
        await env.saveFile({
          defaultName: `${appState.getProject().name}.pdf`,
          filters: [{ name: 'PDF', extensions: ['pdf'] }],
          content: pdfBytes,
        });
      } catch (error) {
        console.error('PDF generation failed:', error);
        await showAlert(`Failed to generate PDF: ${errorMessage(error)}`);
      } finally {
        if (button) button.disabled = false;
      }
    });
  }

  private setupHeaderMenus(): void {
    const toggleBtn = document.getElementById('btn-toggle-sidebar');
    const sidebar = document.querySelector('.column-input') as HTMLElement | null;

    if (!toggleBtn || !sidebar) return;

    // Start with sidebar visible, button active — except in narrow windows
    // (iPad portrait / Split View), where the canvas needs the room.
    toggleBtn.classList.add('active');
    if (window.innerWidth < 1000) {
      sidebar.classList.add('sidebar-hidden');
      toggleBtn.classList.remove('active');
    }

    const toggleSidebar = () => {
      const isHidden = sidebar.classList.toggle('sidebar-hidden');
      toggleBtn.classList.toggle('active', !isHidden);
      setTimeout(() => this.spreadEditor.resize(), 250);
    };

    toggleBtn.addEventListener('click', toggleSidebar);

    document.addEventListener('keydown', (e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === '\\') {
        e.preventDefault();
        toggleSidebar();
      }
    });
  }

  private setupTabs(): void {
    const tabs = document.querySelectorAll('.column-header .tab');
    // Only the Editor/Preview panels — the options column reuses the
    // `.tab-panel` class, and toggling those here blanked the options panel
    // whenever the user switched between Editor and Preview.
    const panels = document.querySelectorAll('.tab-panels > .tab-panel');

    tabs.forEach(tab => {
      tab.addEventListener('click', () => {
        const tabName = tab.getAttribute('data-tab');

        // Update tab states
        tabs.forEach(t => {
          t.classList.toggle('active', t === tab);
          t.setAttribute('aria-selected', t === tab ? 'true' : 'false');
        });

        // Update panel visibility
        panels.forEach(panel => {
          const isActive = panel.id === `tab-${tabName}`;
          panel.classList.toggle('active', isActive);
        });

        // Update editor state
        appState.updateEditor({
          activeTab: tabName as 'editor' | 'preview',
        });

        // Update options tabs state based on mode
        this.updateOptionsTabsForMode(tabName as 'editor' | 'preview');

        // Trigger resize for canvas components
        if (tabName === 'editor') {
          this.spreadEditor.resize();
        } else if (tabName === 'preview') {
          void this.syncFilesNow().then(() => this.pdfPreview.refresh());
        }
      });
    });
  }

  /**
   * Update options tabs based on editor/preview mode
   * In preview mode, disable the "Selected" tab since there's nothing to select
   */
  private updateOptionsTabsForMode(mode: 'editor' | 'preview'): void {
    const selectedTabBtn = document.querySelector('.options-tabs .tab-btn[data-tab="selected"]') as HTMLButtonElement;
    const stylesTabBtn = document.querySelector('.options-tabs .tab-btn[data-tab="styles"]') as HTMLButtonElement;
    const optionsTabButtons = document.querySelectorAll('.options-tabs .tab-btn');
    const optionsTabPanels = document.querySelectorAll('.options-tab-content > .tab-panel');

    if (!selectedTabBtn) return;

    if (mode === 'preview') {
      // Disable the Selected tab
      selectedTabBtn.disabled = true;

      // If Selected tab is currently active, switch to Output tab
      if (selectedTabBtn.classList.contains('active')) {
        const outputTabBtn = document.querySelector('.options-tabs .tab-btn[data-tab="output"]');
        if (outputTabBtn) {
          optionsTabButtons.forEach(b => b.classList.toggle('active', b === outputTabBtn));
          optionsTabPanels.forEach(panel => {
            panel.classList.toggle('active', panel.id === 'tab-output');
          });
        }
      }
    } else {
      // Re-enable the Selected tab in editor mode
      selectedTabBtn.disabled = false;

      // Refresh styles tab if it's active to ensure proper initialization
      if (stylesTabBtn?.classList.contains('active')) {
        updateStylesTab(true);
      }
    }
  }

  private setupOptionsTabs(): void {
    const tabButtons = document.querySelectorAll('.options-tabs .tab-btn');
    const tabPanels = document.querySelectorAll('.options-tab-content > .tab-panel');

    tabButtons.forEach(btn => {
      btn.addEventListener('click', () => {
        // Don't switch to disabled tabs
        if ((btn as HTMLButtonElement).disabled) return;

        const tabName = btn.getAttribute('data-tab');

        // Update button states
        tabButtons.forEach(b => {
          b.classList.toggle('active', b === btn);
        });

        // Update panel visibility
        tabPanels.forEach(panel => {
          const isActive = panel.id === `tab-${tabName}`;
          panel.classList.toggle('active', isActive);
        });

        // Refresh styles tab when it becomes active to ensure proper initialization
        if (tabName === 'styles') {
          updateStylesTab(true);
        }
      });
    });
  }

  private setupCollapsiblePanels(): void {
    // Handle options panels (including Info panel)
    const optionsPanels = document.querySelectorAll('.panel-options, .panel-info');

    optionsPanels.forEach(panel => {
      const header = panel.querySelector('.panel-header.collapsible');
      if (header) {
        header.addEventListener('click', () => {
          panel.classList.toggle('collapsed');
        });

        // Info and Output panels start expanded, others start collapsed
        const panelName = panel.getAttribute('data-panel');
        if (panelName !== 'output' && panelName !== 'info') {
          panel.classList.add('collapsed');
        }
      }
    });

    // Handle preview panel close button — fully hides the preview pane
    const previewPanel = document.querySelector('.panel-preview') as HTMLElement | null;
    const filesPanel = document.querySelector('.panel-files');
    const previewResizer = document.querySelector('[data-resizer="files-preview"]') as HTMLElement | null;
    const closePreviewBtn = document.getElementById('btn-close-preview');
    if (previewPanel && closePreviewBtn) {
      closePreviewBtn.addEventListener('click', () => {
        previewPanel.classList.add('collapsed');
        filesPanel?.classList.add('expanded');
        if (previewResizer) previewResizer.style.display = 'none';
        this.filePreview.showFile(null);
      });
    }
  }

  private setupResizers(): void {
    document.querySelectorAll('.column-resizer').forEach(resizer => {
      this.setupColumnResizer(resizer as HTMLElement);
    });
    document.querySelectorAll('.panel-resizer').forEach(resizer => {
      this.setupPanelResizer(resizer as HTMLElement);
    });
  }

  /**
   * Drag a resizer with pointer events (mouse, trackpad, touch, Pencil).
   * `onMove` receives the pointer delta since the drag started.
   */
  private bindResizerDrag(
    resizer: HTMLElement,
    cursor: string,
    onStart: () => void,
    onMove: (dx: number, dy: number) => void,
    onEnd?: () => void,
  ): void {
    resizer.style.touchAction = 'none';
    resizer.addEventListener('pointerdown', (e: PointerEvent) => {
      e.preventDefault();
      const startX = e.clientX;
      const startY = e.clientY;
      resizer.setPointerCapture(e.pointerId);
      resizer.classList.add('dragging');
      document.body.style.cursor = cursor;
      document.body.style.userSelect = 'none';
      onStart();

      const move = (ev: PointerEvent) => onMove(ev.clientX - startX, ev.clientY - startY);
      const up = () => {
        resizer.classList.remove('dragging');
        document.body.style.cursor = '';
        document.body.style.userSelect = '';
        resizer.removeEventListener('pointermove', move);
        resizer.removeEventListener('pointerup', up);
        resizer.removeEventListener('pointercancel', up);
        onEnd?.();
      };
      resizer.addEventListener('pointermove', move);
      resizer.addEventListener('pointerup', up);
      resizer.addEventListener('pointercancel', up);
    });
  }

  private setupColumnResizer(resizer: HTMLElement): void {
    const resizerType = resizer.dataset.resizer;
    let prevSibling: HTMLElement | null = null;
    let nextSibling: HTMLElement | null = null;

    // Get the columns on either side
    if (resizerType === 'input-editor') {
      prevSibling = document.querySelector('.column-input');
      nextSibling = document.querySelector('.column-editor');
    } else if (resizerType === 'editor-options') {
      prevSibling = document.querySelector('.column-editor');
      nextSibling = document.querySelector('.column-options');
    }

    if (!prevSibling || !nextSibling) return;
    const prev = prevSibling;
    const next = nextSibling;

    let startPrevWidth = 0;
    let startNextWidth = 0;

    this.bindResizerDrag(
      resizer,
      'col-resize',
      () => {
        startPrevWidth = prev.offsetWidth;
        startNextWidth = next.offsetWidth;
      },
      (dx) => {
        const newPrevWidth = Math.max(200, startPrevWidth + dx);
        const newNextWidth = Math.max(200, startNextWidth - dx);
        if (resizerType === 'input-editor') {
          prev.style.flex = `0 0 ${newPrevWidth}px`;
          next.style.flex = '1';
        } else {
          next.style.flex = `0 0 ${newNextWidth}px`;
        }
      },
      () => this.spreadEditor.resize(),
    );
  }

  private setupPanelResizer(resizer: HTMLElement): void {
    const resizerType = resizer.dataset.resizer;
    let prevSibling: HTMLElement | null = null;
    let nextSibling: HTMLElement | null = null;

    if (resizerType === 'files-preview') {
      prevSibling = document.querySelector('.panel-files');
      nextSibling = document.querySelector('.panel-preview');
    }

    if (!prevSibling || !nextSibling) return;
    const prev = prevSibling;
    const next = nextSibling;

    let startPrevHeight = 0;
    let startNextHeight = 0;

    this.bindResizerDrag(
      resizer,
      'row-resize',
      () => {
        startPrevHeight = prev.offsetHeight;
        startNextHeight = next.offsetHeight;
      },
      (_dx, dy) => {
        const newPrevHeight = Math.max(100, startPrevHeight + dy);
        const newNextHeight = Math.max(50, startNextHeight - dy);
        prev.style.flex = `0 0 ${newPrevHeight}px`;
        prev.style.minHeight = `${newPrevHeight}px`;
        prev.style.maxHeight = 'none';
        next.style.flex = `0 0 ${newNextHeight}px`;
        next.style.minHeight = `${newNextHeight}px`;
        next.style.maxHeight = 'none';
      },
    );
  }

  private setupStateListeners(): void {
    // Reflow when requested
    appState.onReflowRequest(() => {
      void this.performReflow();
    });

    // Keep the font service's custom-font registry in sync with project
    // files. Whenever the file list changes, register any new font files
    // and unregister fonts whose files have been removed.
    const registeredFontFiles = new Map<string, string>(); // fileId -> family
    appState.onProjectChange((project) => {
      this.updateDocumentInfo(project);

      const currentFontFiles = project.files.filter((f) => f.type === 'font');
      const seenIds = new Set<string>();
      for (const file of currentFontFiles) {
        seenIds.add(file.id);
        if (!registeredFontFiles.has(file.id)) {
          const family = fontService.registerCustomFont(file.name, file.content);
          registeredFontFiles.set(file.id, family);
        }
      }
      // Drop any fonts whose files were removed.
      for (const [fileId, family] of Array.from(registeredFontFiles.entries())) {
        if (!seenIds.has(fileId)) {
          fontService.unregisterCustomFont(family);
          registeredFontFiles.delete(fileId);
        }
      }
    });

    // Reflow when fonts finish loading (measurements may change)
    // Use a debounce to avoid multiple reflows if many fonts load in succession
    let fontReflowTimeout: number | null = null;
    fontService.onFontLoaded(() => {
      if (fontReflowTimeout) {
        clearTimeout(fontReflowTimeout);
      }
      fontReflowTimeout = window.setTimeout(() => {
        fontReflowTimeout = null;
        void bridge.clearMeasurementCache().finally(() => this.performReflow());
      }, 100);
    });

    // Listen for navigation requests
    window.addEventListener('navigate-to-page', ((e: CustomEvent<{ pageNumber: number }>) => {
      this.spreadEditor.navigateToPage(e.detail.pageNumber);
    }) as EventListener);
  }

  /**
   * Re-run the text flow in Rust. Calls coalesce: while one reflow is in
   * flight, further requests schedule exactly one follow-up. A result is
   * only applied if the inputs it was computed from are still current —
   * edits made during the round trip (e.g. moving an item on a static
   * page) are never overwritten; the reflow simply runs again.
   */
  private performReflow(): Promise<void> {
    if (this.reflowInFlight) {
      this.reflowAgain = true;
      return this.reflowInFlight;
    }
    const run = async () => {
      do {
        this.reflowAgain = false;
        await this.reflowOnce();
      } while (this.reflowAgain);
    };
    this.reflowInFlight = run().finally(() => {
      this.reflowInFlight = null;
    });
    return this.reflowInFlight;
  }

  private async reflowOnce(): Promise<void> {
    const project = appState.getProject();
    const markdown = project.files.filter(f => f.type === 'markdown').map(f => f.content).join('\n\n');
    // Images must be in the native store so markdown images can be sized.
    await this.syncFilesNow();
    let result;
    try {
      result = await bridge.reflow(markdown, snapshot(project));
    } catch (e) {
      console.error('Reflow failed:', e);
      return;
    }

    const current = appState.getProject();
    const stale =
      current.signatures !== project.signatures ||
      current.files !== project.files ||
      current.outputOptions !== project.outputOptions ||
      current.layoutOptions !== project.layoutOptions ||
      current.fontOptions !== project.fontOptions ||
      current.headerFooter !== project.headerFooter ||
      current.blankPages !== project.blankPages;
    if (stale) {
      this.reflowAgain = true;
      return;
    }

    appState.updateProject({ signatures: result.signatures });
    this.spreadEditor.render();
    this.updateDocumentInfo(appState.getProject());
  }

  private updateDocumentInfo(project: BookletProject): void {
    const pageCount = project.signatures.reduce(
      (sum, sig) => sum + sig.pageCount,
      0
    );
    const spreadCount = project.signatures.reduce(
      (sum, sig) => sum + sig.spreads.length,
      0
    );
    const signatureCount = project.signatures.length;
    const sheetsPerSig = project.outputOptions.pagesPerSignature / 4;
    const sheetCount = signatureCount * sheetsPerSig;

    document.getElementById('info-pages')!.textContent = pageCount.toString();
    document.getElementById('info-spreads')!.textContent = spreadCount.toString();
    document.getElementById('info-signatures')!.textContent = signatureCount.toString();
    document.getElementById('info-sheets')!.textContent = sheetCount.toString();
  }
}

function stripExtension(name: string): string {
  return name.replace(/\.printfold$/i, '');
}

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
