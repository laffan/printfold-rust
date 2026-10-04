/**
 * Page geometry helpers for the UI (the Rust engine has the same logic in
 * `model/project.rs`; these keep synchronous editor maths local).
 */

import type { OutputOptions, LayoutOptions, HeaderFooterOptions, Margins } from '../types';
import { getOrientedSheetSize } from '../types';

export interface PageDimensions {
  width: number;
  height: number;
  contentWidth: number;
  contentHeight: number;
}

/** Page size and content box. Header/footer sit inside the margins. */
export function calculatePageDimensions(
  outputOptions: OutputOptions,
  layoutOptions: LayoutOptions,
  _headerFooter?: HeaderFooterOptions
): PageDimensions {
  const sheet = getOrientedSheetSize(outputOptions.sheetSize, outputOptions.orientation);
  let pageWidth = sheet.width / 2;
  let pageHeight: number;
  switch (outputOptions.bookletSize) {
    case 'custom':
      pageWidth = outputOptions.customWidth || sheet.width / 2;
      pageHeight = outputOptions.customHeight || sheet.height;
      break;
    case 'quarter':
      pageHeight = sheet.height / 2;
      break;
    case 'eighth':
      pageHeight = sheet.height / 4;
      break;
    case 'sixteenth':
      pageHeight = sheet.height / 8;
      break;
    default:
      pageHeight = sheet.height;
  }
  const m = layoutOptions.margins;
  return {
    width: pageWidth,
    height: pageHeight,
    contentWidth: pageWidth - m.inner - m.outer,
    contentHeight: pageHeight - m.top - m.bottom,
  };
}

/** Margins for a page with any per-page override applied. */
export function getMarginsForPage(pageNumber: number, layoutOptions: LayoutOptions): Margins {
  const override = layoutOptions.marginOverrides.find(o => o.pageNumber === pageNumber);
  return override ? { ...layoutOptions.margins, ...override.margins } : layoutOptions.margins;
}
