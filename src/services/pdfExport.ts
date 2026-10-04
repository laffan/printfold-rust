/**
 * PDF export orchestration.
 *
 * The PDF itself is built in Rust (`crates/printfold-core/src/pdf`). Pages
 * that carry items, backgrounds or items crossing in from a neighbour are
 * first rasterised here with Konva at 300 DPI — the same renderer as the
 * editor, so gradients, patterns, shadows and web fonts match exactly —
 * and handed to Rust, which places them over (or instead of) the vector
 * text.
 */

import { appState } from './state';
import { bridge, fileMetadata, snapshot } from './bridge';
import { renderPageToImage } from './pageRenderer';
import { calculatePageDimensions } from './pageGeometry';
import type { PageContent } from '../types';

function dataUrlToBytes(dataUrl: string): Uint8Array {
  const base64 = dataUrl.slice(dataUrl.indexOf(',') + 1);
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/** Only one generation at a time; later requests wait for the current one. */
let queue: Promise<unknown> = Promise.resolve();

function serialized<T>(task: () => Promise<T>): Promise<T> {
  const run = queue.then(task, task);
  queue = run.catch(() => undefined);
  return run;
}

/** Generate the print-ready PDF for the current project. */
export function generatePdf(): Promise<Uint8Array> {
  return serialized(async () => {
    const project = appState.getProject();
    const snap = snapshot(project);
    const dims = calculatePageDimensions(project.outputOptions, project.layoutOptions, project.headerFooter);
    const renderAll = project.outputOptions.renderTextAsImages === true;

    const pages = new Map<number, PageContent>();
    for (const sig of project.signatures) {
      for (const spread of sig.spreads) {
        if (spread.verso) pages.set(spread.verso.pageNumber, spread.verso);
        if (spread.recto) pages.set(spread.recto.pageNumber, spread.recto);
      }
    }

    await bridge.pdfClearPrerendered();
    const plan = await bridge.pdfPrerenderPlan(snap);
    for (const number of plan.overlay) {
      const page = pages.get(number);
      if (!page) continue;
      const adjacent = pages.get(page.isRecto ? number - 1 : number + 1) ?? null;
      try {
        const dataUrl = await renderPageToImage(page, dims.width, dims.height, adjacent, {
          includeTextContent: renderAll,
          includeBackground: renderAll || page.pageState !== 'text',
        });
        if (dataUrl) {
          await bridge.pdfPutPrerendered(number, dataUrlToBytes(dataUrl), 'overlay');
        }
      } catch (error) {
        console.warn(`Failed to pre-render page ${number}:`, error);
      }
    }
    // Gradient/pattern backgrounds under the vector text of text pages.
    for (const number of plan.background) {
      const page = pages.get(number);
      if (!page) continue;
      try {
        const dataUrl = await renderPageToImage(page, dims.width, dims.height, null, { backgroundOnly: true });
        if (dataUrl) {
          await bridge.pdfPutPrerendered(number, dataUrlToBytes(dataUrl), 'background');
        }
      } catch (error) {
        console.warn(`Failed to pre-render background of page ${number}:`, error);
      }
    }

    return bridge.pdfGenerate(snap, fileMetadata(project));
  });
}

/** Generate the two-page duplex calibration PDF. */
export function generateTestPage(): Promise<Uint8Array> {
  return serialized(() => bridge.pdfTestPage(snapshot(appState.getProject())));
}
