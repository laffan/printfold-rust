/**
 * Project import/export.
 *
 * Archive encoding lives in Rust (`project_file.rs`); this module applies a
 * decoded project to the app state. Import is two-phase, as in the
 * original ZipHandler: lay out the markdown first, then layer the saved
 * per-page state (static/available pages, items, backgrounds) onto the
 * flowed pages and re-flow if any page left the text flow.
 */

import { appState } from './state';
import { bridge, type ProjectImport, type StaticPageData } from './bridge';
import { defaultFontOptions, defaultLayoutOptions } from './state/defaults';

/** Save the current project to its bound file. */
export function saveProject(): Promise<void> {
  return bridge.projectSave(appState.getProject());
}

function countPages(): number {
  return appState.getProject().signatures.reduce(
    (n, sig) => n + sig.spreads.reduce((m, sp) => m + (sp.verso ? 1 : 0) + (sp.recto ? 1 : 0), 0),
    0,
  );
}

/** Resolve once the project has at least one signature (first reflow done). */
function waitForSignatures(): Promise<void> {
  if (appState.getProject().signatures.length > 0) return Promise.resolve();
  return new Promise<void>(resolve => {
    const unsubscribe = appState.onProjectChange(project => {
      if (project.signatures.length > 0) {
        unsubscribe();
        resolve();
      }
    });
  });
}

/** Apply saved page data; returns true if any page left the text state. */
function applyStaticPageData(pages: StaticPageData[]): boolean {
  let stateChanged = false;
  for (const page of pages) {
    if (page.pageState !== 'text') {
      appState.setPageState(page.pageNumber, page.pageState);
      stateChanged = true;
    }
    if (page.backgroundFill) {
      appState.setPageBackgroundFill(page.pageNumber, page.backgroundFill);
    }
    if (page.customBackgroundImageId) {
      appState.setCustomBackground(page.pageNumber, page.customBackgroundImageId);
    }
    for (const item of page.items) {
      appState.addItemToPage(page.pageNumber, item);
    }
  }
  return stateChanged;
}

/** Load a decoded project into the app state. */
export async function importProject(data: ProjectImport): Promise<void> {
  const { manifest, files, staticPages } = data;
  appState.reset();

  if (manifest) {
    appState.updateProject({
      id: manifest.projectId || crypto.randomUUID(),
      name: manifest.name,
      measurementUnit: manifest.measurementUnit || 'in',
      outputOptions: manifest.outputOptions,
      // Backfill fields added after older projects were written.
      layoutOptions: { ...defaultLayoutOptions, ...manifest.layoutOptions },
      fontOptions: {
        ...defaultFontOptions,
        ...manifest.fontOptions,
        footnote: manifest.fontOptions?.footnote ?? defaultFontOptions.footnote,
      },
      headerFooter: manifest.headerFooter,
      blankPages: manifest.blankPages || [],
    });
  }

  // Drop the placeholder signature from reset() so waiting for signatures
  // really waits for the reflow of the imported markdown.
  if (staticPages.length > 0) {
    appState.updateProject({ signatures: [] });
  }

  appState.addFiles(files);

  if (manifest?.mainDocument && files.some(f => f.id === manifest.mainDocument)) {
    appState.setMainDocument(manifest.mainDocument);
  }

  if (staticPages.length > 0) {
    await waitForSignatures();
    // Static pages can sit past the end of the flowed markdown; add
    // available signatures until every saved page has a slot.
    const maxPage = Math.max(...staticPages.map(p => p.pageNumber));
    let guard = 0;
    while (countPages() < maxPage && guard++ < 1000) {
      appState.addAvailableSignature();
    }
    if (applyStaticPageData(staticPages)) {
      appState.requestReflow();
    }
  }
}
