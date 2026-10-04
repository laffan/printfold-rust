/**
 * PDFPreview Component
 * Displays a preview of the print-ready PDF output.
 *
 * The original embedded the PDF in an <iframe>; WKWebView can't display
 * PDFs that way reliably (iPadOS shows only the first page), so the port
 * renders pages with pdf.js. Pages render lazily as they scroll into view.
 */

import * as pdfjs from 'pdfjs-dist/legacy/build/pdf.mjs';
import workerUrl from 'pdfjs-dist/legacy/build/pdf.worker.min.mjs?url';
import { appState } from '../services/state';
import { generatePdf } from '../services/pdfExport';

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

type PdfDocument = Awaited<ReturnType<typeof pdfjs.getDocument>['promise']>;

export class PDFPreview {
  private container!: HTMLElement;
  private isGenerating = false;
  private pendingRefresh = false;
  private document: PdfDocument | null = null;
  private observer: IntersectionObserver | null = null;
  private refreshTimeout: number | null = null;
  private generation = 0;

  mount(): void {
    this.container = document.getElementById('pdf-preview-container')!;

    // Regenerate when the layout changes. Reflow runs asynchronously in
    // Rust, so watch for its result (new signatures) as well as requests.
    appState.onReflowRequest(() => {
      this.scheduleRefresh();
    });
    appState.onProjectChange((project, prev) => {
      if (project.signatures !== prev.signatures || project.outputOptions !== prev.outputOptions) {
        this.scheduleRefresh();
      }
    });

    // Re-render at the new width when the pane is resized.
    let resizeTimer: number | null = null;
    new ResizeObserver(() => {
      if (!this.document) return;
      if (resizeTimer) clearTimeout(resizeTimer);
      resizeTimer = window.setTimeout(() => this.layoutPages(), 200);
    }).observe(this.container);
  }

  private scheduleRefresh(): void {
    if (this.refreshTimeout) {
      clearTimeout(this.refreshTimeout);
    }
    this.refreshTimeout = window.setTimeout(() => {
      // Only refresh if preview tab is active
      if (appState.getEditor().activeTab === 'preview') {
        void this.refresh();
      }
    }, 500);
  }

  async refresh(): Promise<void> {
    if (this.isGenerating) {
      this.pendingRefresh = true;
      return;
    }

    const project = appState.getProject();
    if (project.signatures.length === 0) {
      this.showPlaceholder('No content to preview');
      return;
    }

    this.isGenerating = true;
    if (!this.document) this.showPlaceholder('Generating PDF preview...');

    try {
      const bytes = await generatePdf();
      const doc = await pdfjs.getDocument({ data: bytes.slice() }).promise;
      const previous = this.document;
      this.document = doc;
      this.layoutPages();
      void previous?.destroy();
    } catch (error) {
      console.error('PDF generation failed:', error);
      this.showPlaceholder('Failed to generate PDF preview');
    } finally {
      this.isGenerating = false;
      if (this.pendingRefresh) {
        this.pendingRefresh = false;
        void this.refresh();
      }
    }
  }

  /** Build page placeholders sized to the pane; render when visible. */
  private layoutPages(): void {
    const doc = this.document;
    if (!doc) return;
    const generation = ++this.generation;
    this.observer?.disconnect();

    const scrollTop = this.container.scrollTop;
    const list = document.createElement('div');
    list.className = 'pdf-pages';
    const width = Math.max(200, this.container.clientWidth - 32);

    this.observer = new IntersectionObserver(entries => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const holder = entry.target as HTMLElement;
        this.observer?.unobserve(holder);
        void this.renderPage(doc, Number(holder.dataset.page), holder, width, generation);
      }
    }, { root: this.container, rootMargin: '400px 0px' });

    void (async () => {
      for (let n = 1; n <= doc.numPages; n++) {
        const page = await doc.getPage(n);
        if (generation !== this.generation) return;
        const viewport = page.getViewport({ scale: 1 });
        const holder = document.createElement('div');
        holder.className = 'pdf-page';
        holder.dataset.page = String(n);
        holder.style.width = `${width}px`;
        holder.style.height = `${(width * viewport.height) / viewport.width}px`;
        list.appendChild(holder);
        this.observer?.observe(holder);
      }
    })();

    this.container.innerHTML = '';
    this.container.appendChild(list);
    this.container.scrollTop = scrollTop;
  }

  private async renderPage(doc: PdfDocument, n: number, holder: HTMLElement, width: number, generation: number): Promise<void> {
    const page = await doc.getPage(n);
    if (generation !== this.generation) return;
    const base = page.getViewport({ scale: 1 });
    const ratio = window.devicePixelRatio || 1;
    const viewport = page.getViewport({ scale: (width / base.width) * ratio });
    const canvas = document.createElement('canvas');
    canvas.width = Math.floor(viewport.width);
    canvas.height = Math.floor(viewport.height);
    canvas.style.width = '100%';
    canvas.style.height = '100%';
    const ctx = canvas.getContext('2d');
    if (!ctx) return;
    await page.render({ canvasContext: ctx, viewport }).promise;
    if (generation !== this.generation) return;
    holder.replaceChildren(canvas);
  }

  private showPlaceholder(message: string): void {
    this.observer?.disconnect();
    this.container.innerHTML = `
      <div class="preview-placeholder">
        <p>${message}</p>
      </div>
    `;
  }

  destroy(): void {
    this.observer?.disconnect();
    void this.document?.destroy();
    this.document = null;
  }
}
