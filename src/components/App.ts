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
import { generatePdf } from '../services/pdfExport';
import { showAlert } from '../services/dialogs';
import { ProjectBrowser } from './ProjectBrowser';
import { ProjectSession } from './projectSession';
import type { BookletProject } from '../types';

export class App {
  private fileList!: FileList;
  private filePreview!: FilePreview;
  private spreadEditor!: SpreadEditor;
  private pdfPreview!: PDFPreview;
  private optionsPanel!: OptionsPanel;
  private browser!: ProjectBrowser;
  /** The open project's file: lifecycle, auto-save, file store, thumbnail. */
  private session = new ProjectSession(() => this.performReflow());

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
    this.browser = new ProjectBrowser();

    // Mount components
    this.fileList.mount();
    this.filePreview.mount();
    this.spreadEditor.mount();
    this.pdfPreview.mount();
    this.optionsPanel.mount();
    this.browser.mount({
      open: (path) => this.openProject(path),
      create: () => this.createProject(),
      openElsewhere: () => this.openProjectElsewhere(),
    });

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
    this.session.setup();
    this.setupResizers();
    this.setupEditorShortcuts();
    void this.setupFileAssociations();
    void this.setupNativeMenu();

    // Start in the project browser — a project must be created or opened
    // before the editor becomes interactive.
    void this.browser.show();

    console.log('PrintFold initialized', bridge.platform);
  }

  // -------------------------------------------------------------------
  // Welcome screen / project file lifecycle
  // -------------------------------------------------------------------

  private async createProject(): Promise<void> {
    await this.session.create();
    this.browser.hide();
  }

  private async openProject(path: string): Promise<void> {
    await this.session.openPath(path);
    this.browser.hide();
  }

  private async openProjectElsewhere(): Promise<void> {
    if (await this.session.openDialog()) this.browser.hide();
    else await this.browser.refresh();
  }

  /** Save and close the open project, then show all projects. */
  private async showProjects(): Promise<void> {
    try {
      await this.session.close();
    } catch (e) {
      console.error('Closing the project failed:', e);
    }
    await this.browser.show();
  }

  /** Report failures of user-initiated project actions. */
  private async guard(action: () => Promise<void>): Promise<void> {
    try {
      await action();
    } catch (e) {
      console.error('Project action failed:', e);
      await showAlert(errorMessage(e));
    }
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
      await this.openProject(path);
    } catch (e) {
      console.error('Open from path failed:', e);
      await showAlert(`Could not open project: ${errorMessage(e)}`);
      if (this.browser.isVisible) await this.browser.refresh();
    }
  }

  /** Route macOS menu bar items to the same actions as the UI buttons. */
  private async setupNativeMenu(): Promise<void> {
    const click = (selector: string) => (document.querySelector(selector) as HTMLElement | null)?.click();
    try {
      await bridge.onMenu((id) => {
        const inEditor = document.body.classList.contains('welcome-active') === false;
        switch (id) {
          case 'new-project': void this.guard(() => this.createProject()); break;
          case 'open-project': void this.guard(() => this.openProjectElsewhere()); break;
          case 'projects': if (inEditor) void this.showProjects(); break;
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

  /**
   * Keyboard shortcuts for the editor that the macOS menu bar provides
   * natively — needed on iPadOS (hardware keyboard), harmless on macOS
   * where the menu consumes them first.
   */
  private setupEditorShortcuts(): void {
    if (!env.isMobile) return;
    document.addEventListener('keydown', (e) => {
      if (this.browser.isVisible || !(e.metaKey || e.ctrlKey)) return;
      if (!document.getElementById('modal-overlay')?.classList.contains('hidden')) return;
      const key = e.key.toLowerCase();
      const click = (selector: string) => (document.querySelector(selector) as HTMLElement | null)?.click();
      if (key === 'o' && e.shiftKey) {
        e.preventDefault();
        void this.showProjects();
      } else if (key === 'n' && !e.shiftKey) {
        e.preventDefault();
        void this.guard(() => this.createProject());
      } else if (key === 'e' && !e.shiftKey) {
        e.preventDefault();
        click('#btn-export');
      } else if (key === 'a' && e.shiftKey) {
        e.preventDefault();
        click('#btn-add-files');
      } else if (key === '1' || key === '2') {
        e.preventDefault();
        click(`.column-header .tab[data-tab="${key === '1' ? 'editor' : 'preview'}"]`);
      }
    });
  }

  private setupHeaderButtons(): void {
    // Back to the project browser (saves and closes the project)
    document.getElementById('btn-welcome')?.addEventListener('click', () => {
      void this.showProjects();
    });

    // Click the project name to rename the file
    document.getElementById('project-name-display')?.addEventListener('click', () => {
      void this.guard(() => this.session.renameCurrent());
    });

    // Export PDF button
    document.getElementById('btn-export')?.addEventListener('click', async () => {
      const button = document.getElementById('btn-export') as HTMLButtonElement | null;
      if (button) button.disabled = true;
      try {
        await this.session.syncFilesNow();
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
          void this.session.syncFilesNow().then(() => this.pdfPreview.refresh());
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
    await this.session.syncFilesNow();
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

function errorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}
