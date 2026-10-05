/**
 * Project browser: the start screen. Shows the project library as a grid
 * of cover thumbnails and works as a small file manager — create, open,
 * select, rename, duplicate, delete, share and import projects.
 *
 * Mouse/trackpad: click selects (⌘/Shift extend), double-click opens,
 * right-click for actions. Touch: tap opens, press and hold for actions,
 * "Select" for multi-selection. Keyboard: arrows, Return opens,
 * ⌘⌫ deletes, ⌘A selects all, ⌘N new, ⌘O open/import.
 */

import { bridge, type LibraryEntry, type LibraryInfo } from '../../services/bridge';
import { env } from '../../services/environment';
import { showAlert, showConfirm } from '../../services/dialogs';
import { showContextMenu, hideContextMenu, type ContextMenuItem } from '../SpreadEditor/selection';
import { createCard, pruneThumbnails } from './card';

export interface BrowserHandlers {
  /** Open a project in the editor. */
  open(path: string): Promise<void>;
  /** Create a new project and open it. */
  create(): Promise<void>;
  /** macOS: open a project from anywhere on disk. */
  openElsewhere(): Promise<void>;
}

const LONG_PRESS_MS = 500;
const LONG_PRESS_SLOP = 10;

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export class ProjectBrowser {
  private root!: HTMLElement;
  private grid!: HTMLElement;
  private handlers!: BrowserHandlers;
  private info: LibraryInfo | null = null;
  private entries: LibraryEntry[] = [];
  private selected = new Set<string>();
  private anchor: string | null = null;
  private filter = '';
  private selectMode = false;
  private lastPointerType = 'mouse';
  private busy = false;

  mount(handlers: BrowserHandlers): void {
    this.handlers = handlers;
    this.root = document.getElementById('welcome-screen')!;
    this.grid = this.root.querySelector('#browser-grid')!;

    const importBtn = this.root.querySelector<HTMLButtonElement>('#browser-import')!;
    importBtn.textContent = env.isMobile ? 'Import…' : 'Open…';
    importBtn.title = env.isMobile ? 'Copy projects from Files into PrintFold' : 'Open a project from any folder';
    importBtn.addEventListener('click', () => void this.importOrOpen());
    this.root.querySelector('#browser-new')!.addEventListener('click', () => void this.run(() => this.handlers.create()));
    this.root.querySelector('#browser-select')!.addEventListener('click', () => this.toggleSelectMode());
    this.root.querySelector('#browser-reveal')!.addEventListener('click', () => void bridge.libraryReveal());
    const search = this.root.querySelector<HTMLInputElement>('#browser-search')!;
    search.addEventListener('input', () => {
      this.filter = search.value.trim().toLowerCase();
      this.render();
    });

    this.root.querySelectorAll<HTMLButtonElement>('[data-browser-action]').forEach(btn => {
      btn.addEventListener('click', (e) => this.runAction(btn.dataset.browserAction!, e));
    });

    this.setupGridEvents();
    this.setupKeyboard();
    this.setupDrop();
  }

  get isVisible(): boolean {
    return !this.root.classList.contains('hidden');
  }

  async show(): Promise<void> {
    this.root.classList.remove('hidden');
    document.body.classList.add('welcome-active');
    await this.refresh();
    this.grid.focus({ preventScroll: true });
  }

  hide(): void {
    hideContextMenu();
    this.root.classList.add('hidden');
    document.body.classList.remove('welcome-active');
  }

  async refresh(select?: string[]): Promise<void> {
    try {
      this.info ??= await bridge.libraryInfo();
      this.entries = await bridge.libraryList();
    } catch (e) {
      console.error('Could not list projects:', e);
      this.entries = [];
    }
    pruneThumbnails(this.entries);
    const paths = new Set(this.entries.map(e => e.path));
    if (select) this.selected = new Set(select.filter(p => paths.has(p)));
    else for (const p of Array.from(this.selected)) if (!paths.has(p)) this.selected.delete(p);
    this.render();
  }

  // ------------------------------------------------------------ rendering

  private visibleEntries(): LibraryEntry[] {
    if (!this.filter) return this.entries;
    return this.entries.filter(e => e.name.toLowerCase().includes(this.filter));
  }

  private render(): void {
    const visible = this.visibleEntries();
    this.grid.replaceChildren(...visible.map(e => createCard(e, this.selected.has(e.path))));
    const empty = this.root.querySelector<HTMLElement>('#browser-empty')!;
    empty.hidden = visible.length > 0;
    empty.textContent = this.entries.length === 0
      ? 'No projects yet. Create one with New Project, or drop .printfold files here.'
      : 'No projects match your search.';
    const location = this.root.querySelector('#browser-location');
    if (location && this.info) location.textContent = this.info.display;
    this.root.querySelector<HTMLElement>('#browser-reveal')!.hidden = !this.info?.canReveal;
    this.root.classList.toggle('select-mode', this.selectMode);
    this.root.querySelector('#browser-select')!.textContent = this.selectMode ? 'Done' : 'Select';
    this.updateToolbar();
  }

  private updateToolbar(): void {
    const count = this.selected.size;
    const toolbar = this.root.querySelector<HTMLElement>('#browser-toolbar')!;
    toolbar.classList.toggle('active', count > 0);
    this.root.querySelector('#browser-count')!.textContent = count === 1 ? '1 selected' : `${count} selected`;
    const single = count === 1;
    const setEnabled = (action: string, enabled: boolean, visible = true) => {
      const btn = toolbar.querySelector<HTMLButtonElement>(`[data-browser-action="${action}"]`);
      if (!btn) return;
      btn.disabled = !enabled;
      btn.hidden = !visible;
    };
    setEnabled('open', single);
    setEnabled('rename', single);
    setEnabled('duplicate', count > 0);
    setEnabled('share', single);
    setEnabled('reveal', single, !!this.info?.canReveal);
    setEnabled('delete', count > 0);
    const del = toolbar.querySelector<HTMLButtonElement>('[data-browser-action="delete"]');
    if (del) del.textContent = this.info?.usesTrash ? 'Move to Trash' : 'Delete';
  }

  private entry(path: string): LibraryEntry | undefined {
    return this.entries.find(e => e.path === path);
  }

  private cardFor(path: string): HTMLElement | null {
    return this.grid.querySelector<HTMLElement>(`.project-card[data-path="${CSS.escape(path)}"]`);
  }

  // ------------------------------------------------------------ selection

  private setSelection(paths: string[], anchor?: string): void {
    this.selected = new Set(paths);
    if (anchor !== undefined) this.anchor = anchor;
    this.grid.querySelectorAll<HTMLElement>('.project-card').forEach(card => {
      const on = this.selected.has(card.dataset.path!);
      card.classList.toggle('selected', on);
      card.setAttribute('aria-selected', on ? 'true' : 'false');
    });
    this.updateToolbar();
  }

  private clickSelect(path: string, e: MouseEvent): void {
    const toggle = e.metaKey || e.ctrlKey || this.selectMode;
    if (e.shiftKey && this.anchor) {
      const order = this.visibleEntries().map(x => x.path);
      const [a, b] = [order.indexOf(this.anchor), order.indexOf(path)].sort((x, y) => x - y);
      if (a >= 0 && b >= 0) {
        this.setSelection(order.slice(a, b + 1));
        return;
      }
    }
    if (toggle) {
      const next = new Set(this.selected);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      this.setSelection(Array.from(next), path);
    } else {
      this.setSelection([path], path);
    }
  }

  private toggleSelectMode(): void {
    this.selectMode = !this.selectMode;
    if (!this.selectMode) this.setSelection([]);
    this.render();
  }

  // --------------------------------------------------------------- events

  private setupGridEvents(): void {
    let pressTimer: number | null = null;
    let pressStart = { x: 0, y: 0 };
    let pressFired = false;
    const cancelPress = () => {
      if (pressTimer !== null) clearTimeout(pressTimer);
      pressTimer = null;
    };

    this.grid.addEventListener('pointerdown', (e) => {
      this.lastPointerType = e.pointerType;
      pressFired = false;
      const card = (e.target as HTMLElement).closest<HTMLElement>('.project-card');
      if (!card || e.pointerType !== 'touch') return;
      pressStart = { x: e.clientX, y: e.clientY };
      cancelPress();
      pressTimer = window.setTimeout(() => {
        pressTimer = null;
        pressFired = true;
        this.openMenu(card.dataset.path!, pressStart.x, pressStart.y);
      }, LONG_PRESS_MS);
    });
    this.grid.addEventListener('pointermove', (e) => {
      if (Math.hypot(e.clientX - pressStart.x, e.clientY - pressStart.y) > LONG_PRESS_SLOP) cancelPress();
    });
    this.grid.addEventListener('pointerup', cancelPress);
    this.grid.addEventListener('pointercancel', cancelPress);

    this.grid.addEventListener('click', (e) => {
      if (pressFired) {
        pressFired = false;
        return;
      }
      const target = e.target as HTMLElement;
      const card = target.closest<HTMLElement>('.project-card');
      if (!card) {
        if (!this.selectMode && !e.metaKey && !e.shiftKey) this.setSelection([]);
        return;
      }
      const path = card.dataset.path!;
      if (target.closest('.card-more')) {
        const r = target.getBoundingClientRect();
        this.openMenu(path, r.left + r.width / 2, r.bottom);
        return;
      }
      if (this.lastPointerType === 'touch' && !this.selectMode) {
        void this.openProject(path);
        return;
      }
      this.clickSelect(path, e);
    });

    this.grid.addEventListener('dblclick', (e) => {
      const target = e.target as HTMLElement;
      const card = target.closest<HTMLElement>('.project-card');
      if (!card || this.selectMode || target.closest('.card-more')) return;
      if (target.closest('.card-name')) {
        void this.renameInline(card.dataset.path!);
      } else {
        void this.openProject(card.dataset.path!);
      }
    });

    this.grid.addEventListener('contextmenu', (e) => {
      const card = (e.target as HTMLElement).closest<HTMLElement>('.project-card');
      if (!card) return;
      e.preventDefault();
      this.openMenu(card.dataset.path!, e.clientX, e.clientY);
    });
  }

  private setupKeyboard(): void {
    document.addEventListener('keydown', (e) => {
      if (!this.isVisible || this.busy) return;
      if (!document.getElementById('modal-overlay')?.classList.contains('hidden')) return;
      const typing = e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement;
      const cmd = e.metaKey || e.ctrlKey;
      if (cmd && e.key.toLowerCase() === 'n') {
        e.preventDefault();
        void this.run(() => this.handlers.create());
        return;
      }
      if (cmd && e.key.toLowerCase() === 'o') {
        e.preventDefault();
        void this.importOrOpen();
        return;
      }
      if (cmd && e.key.toLowerCase() === 'f') {
        e.preventDefault();
        this.root.querySelector<HTMLInputElement>('#browser-search')?.focus();
        return;
      }
      if (typing) return;
      const selected = Array.from(this.selected);
      if (cmd && e.key.toLowerCase() === 'a') {
        e.preventDefault();
        this.setSelection(this.visibleEntries().map(x => x.path));
      } else if (cmd && e.key.toLowerCase() === 'd' && selected.length > 0) {
        e.preventDefault();
        void this.duplicate(selected);
      } else if (e.key === 'Enter' && selected.length === 1) {
        e.preventDefault();
        void this.openProject(selected[0]);
      } else if ((e.key === 'Backspace' || e.key === 'Delete') && selected.length > 0) {
        e.preventDefault();
        void this.delete(selected);
      } else if (e.key === 'Escape') {
        hideContextMenu();
        this.setSelection([]);
      } else if (e.key.startsWith('Arrow')) {
        e.preventDefault();
        this.moveSelection(e.key, e.shiftKey);
      }
    });
  }

  /** Arrow-key navigation through the grid (rows from card positions). */
  private moveSelection(key: string, extend: boolean): void {
    const cards = Array.from(this.grid.querySelectorAll<HTMLElement>('.project-card'));
    if (cards.length === 0) return;
    const current = this.anchor ? cards.findIndex(c => c.dataset.path === this.anchor) : -1;
    let next = current;
    if (current < 0) {
      next = 0;
    } else if (key === 'ArrowLeft') {
      next = Math.max(0, current - 1);
    } else if (key === 'ArrowRight') {
      next = Math.min(cards.length - 1, current + 1);
    } else {
      const top = cards[0].offsetTop;
      const perRow = Math.max(1, cards.filter(c => c.offsetTop === top).length);
      next = key === 'ArrowUp' ? current - perRow : current + perRow;
      if (next < 0 || next >= cards.length) return;
    }
    const path = cards[next].dataset.path!;
    if (extend) {
      const next2 = new Set(this.selected);
      next2.add(path);
      this.setSelection(Array.from(next2), path);
    } else {
      this.setSelection([path], path);
    }
    cards[next].scrollIntoView({ block: 'nearest' });
  }

  private setupDrop(): void {
    let depth = 0;
    const hasFiles = (e: DragEvent) => Array.from(e.dataTransfer?.types ?? []).includes('Files');
    this.root.addEventListener('dragenter', (e) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth++;
      this.root.classList.add('drop-target');
    });
    this.root.addEventListener('dragover', (e) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'copy';
    });
    this.root.addEventListener('dragleave', () => {
      depth = Math.max(0, depth - 1);
      if (depth === 0) this.root.classList.remove('drop-target');
    });
    this.root.addEventListener('drop', (e) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      depth = 0;
      this.root.classList.remove('drop-target');
      void this.importDropped(Array.from(e.dataTransfer?.files ?? []));
    });
  }

  // -------------------------------------------------------------- actions

  /** Run an action, reporting failures; ignores re-entry while busy. */
  private async run(action: () => Promise<void>): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    try {
      await action();
    } catch (e) {
      console.error('Project browser action failed:', e);
      await showAlert(errorText(e));
    } finally {
      this.busy = false;
    }
  }

  private runAction(action: string, e?: MouseEvent): void {
    const selected = Array.from(this.selected);
    const first = selected[0];
    switch (action) {
      case 'open': if (first) void this.openProject(first); break;
      case 'rename': if (first) void this.renameInline(first); break;
      case 'duplicate': void this.duplicate(selected); break;
      case 'share': if (first) void this.share(first, e); break;
      case 'reveal': if (first) void this.run(() => bridge.libraryReveal(first)); break;
      case 'delete': void this.delete(selected); break;
    }
  }

  private openMenu(path: string, x: number, y: number): void {
    if (!this.selected.has(path)) this.setSelection([path], path);
    const entry = this.entry(path);
    if (!entry) return;
    const multiple = this.selected.size > 1;
    const paths = Array.from(this.selected);
    const items: ContextMenuItem[] = [
      { label: 'Open', action: () => void this.openProject(path), disabled: multiple },
      { label: 'Rename', action: () => void this.renameInline(path), disabled: multiple },
      { label: 'Duplicate', action: () => void this.duplicate(paths) },
      { label: 'Share…', action: () => void this.share(path, undefined, { x, y }), disabled: multiple },
    ];
    if (this.info?.canReveal) {
      items.push({ label: 'Show in Finder', action: () => void this.run(() => bridge.libraryReveal(path)), disabled: multiple });
    }
    if (!entry.inLibrary) {
      items.push({ label: 'Remove from Recents', action: () => void this.forget(paths), separator: true });
    }
    items.push({
      label: this.info?.usesTrash ? 'Move to Trash' : 'Delete',
      action: () => void this.delete(paths),
      separator: entry.inLibrary,
    });
    showContextMenu(x, y, items);
  }

  private openProject(path: string): Promise<void> {
    return this.run(() => this.handlers.open(path));
  }

  private importOrOpen(): Promise<void> {
    if (!env.isMobile) return this.run(() => this.handlers.openElsewhere());
    return this.run(async () => {
      const added = await bridge.libraryImport();
      await this.refresh(added.map(a => a.path));
    });
  }

  private async importDropped(files: File[]): Promise<void> {
    await this.run(async () => {
      const projects = files.filter(f => f.name.toLowerCase().endsWith('.printfold'));
      if (projects.length === 0) {
        await showAlert('Drop .printfold project files here. Markdown, images and fonts go into an open project\'s Files panel.');
        return;
      }
      const added: string[] = [];
      for (const file of projects) {
        const loc = await bridge.libraryImportBytes(new Uint8Array(await file.arrayBuffer()), file.name);
        added.push(loc.path);
      }
      await this.refresh(added);
    });
  }

  /** Edit the card's name in place; Return commits, Escape cancels. */
  private async renameInline(path: string): Promise<void> {
    const entry = this.entry(path);
    const nameEl = this.cardFor(path)?.querySelector<HTMLElement>('.card-name');
    if (!entry || !nameEl) return;
    hideContextMenu();
    this.setSelection([path], path);
    const input = document.createElement('input');
    input.type = 'text';
    input.className = 'card-rename';
    input.value = entry.name;
    input.setAttribute('autocapitalize', 'words');
    input.setAttribute('autocomplete', 'off');
    nameEl.replaceChildren(input);
    input.focus();
    input.select();
    await new Promise<void>(resolve => {
      let done = false;
      const finish = async (commit: boolean) => {
        if (done) return;
        done = true;
        const value = input.value.trim();
        if (commit && value && value !== entry.name) {
          await this.run(async () => {
            const loc = await bridge.libraryRename(path, value);
            await this.refresh([loc.path]);
          });
          if (this.cardFor(path)) nameEl.textContent = entry.name;
        } else {
          nameEl.textContent = entry.name;
        }
        resolve();
      };
      input.addEventListener('keydown', (e) => {
        e.stopPropagation();
        if (e.key === 'Enter') void finish(true);
        if (e.key === 'Escape') void finish(false);
      });
      input.addEventListener('blur', () => void finish(true));
      input.addEventListener('click', e => e.stopPropagation());
      input.addEventListener('dblclick', e => e.stopPropagation());
    });
  }

  private duplicate(paths: string[]): Promise<void> {
    return this.run(async () => {
      const copies: string[] = [];
      for (const p of paths) copies.push((await bridge.libraryDuplicate(p)).path);
      await this.refresh(copies);
    });
  }

  private share(path: string, e?: MouseEvent, point?: { x: number; y: number }): Promise<void> {
    const anchor = point
      ?? (e?.currentTarget instanceof HTMLElement ? centerBottom(e.currentTarget) : null)
      ?? (this.cardFor(path) ? centerBottom(this.cardFor(path)!.querySelector('.card-thumb') as HTMLElement) : { x: 0, y: 0 });
    return this.run(() => bridge.libraryShare(path, anchor.x, anchor.y));
  }

  private delete(paths: string[]): Promise<void> {
    if (paths.length === 0) return Promise.resolve();
    const names = paths.map(p => this.entry(p)?.name ?? p);
    const trash = !!this.info?.usesTrash;
    const subject = names.length === 1 ? `“${names[0]}”` : `${names.length} projects`;
    const heading = trash ? 'Move to Trash' : 'Delete';
    const message = trash
      ? `Move ${subject} to the Trash?`
      : `Delete ${subject}? This can't be undone.`;
    return this.run(async () => {
      if (!(await showConfirm(heading, message, heading, true))) return;
      await bridge.libraryDelete(paths);
      this.setSelection([]);
      await this.refresh();
    });
  }

  private forget(paths: string[]): Promise<void> {
    return this.run(async () => {
      for (const p of paths) await bridge.libraryForget(p);
      this.setSelection([]);
      await this.refresh();
    });
  }
}

function centerBottom(el: HTMLElement): { x: number; y: number } {
  const r = el.getBoundingClientRect();
  return { x: r.left + r.width / 2, y: r.bottom };
}
