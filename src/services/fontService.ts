/**
 * Font Service
 *
 * For Styles (body, headings, etc.): the installed system fonts, listed by
 * the Rust engine (which measures and embeds the same font files), with
 * web-safe fonts as a fallback list.
 * For Static Page Items: Google Fonts + web-safe fonts (rendered to images).
 * Custom fonts: user uploads, registered with both WebKit (@font-face) and
 * the Rust engine so layout, canvas and PDF all use the same file.
 */

import { bridge, type FamilyVariants } from './bridge';

export interface FontDefinition {
  name: string;
  family: string; // CSS font-family value
  category: 'serif' | 'sans-serif' | 'monospace' | 'display';
  weights?: number[];
  loaded?: boolean; // For async font loading
}

// Web-safe fonts that work reliably in PDFs across all platforms
export const WEB_SAFE_FONTS: FontDefinition[] = [
  // Serif fonts
  { name: 'Georgia', family: 'Georgia', category: 'serif' },
  { name: 'Times New Roman', family: 'Times New Roman', category: 'serif' },
  { name: 'Palatino', family: 'Palatino Linotype, Palatino, Book Antiqua', category: 'serif' },
  { name: 'Garamond', family: 'Garamond, EB Garamond', category: 'serif' },
  { name: 'Baskerville', family: 'Baskerville, Baskerville Old Face', category: 'serif' },
  { name: 'Book Antiqua', family: 'Book Antiqua, Palatino', category: 'serif' },
  { name: 'Cambria', family: 'Cambria', category: 'serif' },

  // Sans-serif fonts
  { name: 'Arial', family: 'Arial', category: 'sans-serif' },
  { name: 'Helvetica', family: 'Helvetica Neue, Helvetica', category: 'sans-serif' },
  { name: 'Verdana', family: 'Verdana', category: 'sans-serif' },
  { name: 'Tahoma', family: 'Tahoma', category: 'sans-serif' },
  { name: 'Trebuchet MS', family: 'Trebuchet MS', category: 'sans-serif' },
  { name: 'Lucida Sans', family: 'Lucida Sans Unicode, Lucida Grande', category: 'sans-serif' },
  { name: 'Segoe UI', family: 'Segoe UI', category: 'sans-serif' },
  { name: 'Calibri', family: 'Calibri', category: 'sans-serif' },
  { name: 'Candara', family: 'Candara', category: 'sans-serif' },
  { name: 'Optima', family: 'Optima', category: 'sans-serif' },
  { name: 'Futura', family: 'Futura', category: 'sans-serif' },
  { name: 'Gill Sans', family: 'Gill Sans, Gill Sans MT', category: 'sans-serif' },
  { name: 'Century Gothic', family: 'Century Gothic', category: 'sans-serif' },

  // Monospace fonts
  { name: 'Courier New', family: 'Courier New', category: 'monospace' },
  { name: 'Courier', family: 'Courier', category: 'monospace' },
  { name: 'Lucida Console', family: 'Lucida Console', category: 'monospace' },
  { name: 'Monaco', family: 'Monaco', category: 'monospace' },
  { name: 'Consolas', family: 'Consolas', category: 'monospace' },
  { name: 'Menlo', family: 'Menlo', category: 'monospace' },
];

// Google Fonts for static page items (rendered to images, no CORS issues)
export const GOOGLE_FONTS: FontDefinition[] = [
  // Sans-serif fonts
  { name: 'DM Sans', family: 'DM Sans', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Inter', family: 'Inter', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Work Sans', family: 'Work Sans', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Space Grotesk', family: 'Space Grotesk', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Syne', family: 'Syne', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Libre Franklin', family: 'Libre Franklin', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Fira Sans', family: 'Fira Sans', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Alegreya Sans', family: 'Alegreya Sans', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Source Sans Pro', family: 'Source Sans 3', category: 'sans-serif', weights: [400, 600, 700] },
  { name: 'Roboto', family: 'Roboto', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Poppins', family: 'Poppins', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Archivo Narrow', family: 'Archivo Narrow', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Karla', family: 'Karla', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Proza Libre', family: 'Proza Libre', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'IBM Plex Sans', family: 'IBM Plex Sans', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Manrope', family: 'Manrope', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Montserrat', family: 'Montserrat', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Lato', family: 'Lato', category: 'sans-serif', weights: [400, 700] },
  { name: 'PT Sans', family: 'PT Sans', category: 'sans-serif', weights: [400, 700] },
  { name: 'Chivo', family: 'Chivo', category: 'sans-serif', weights: [400, 700] },
  { name: 'Rubik', family: 'Rubik', category: 'sans-serif', weights: [400, 500, 700] },
  { name: 'Open Sans', family: 'Open Sans', category: 'sans-serif', weights: [400, 600, 700] },
  { name: 'Raleway', family: 'Raleway', category: 'sans-serif', weights: [400, 500, 700] },

  // Serif fonts
  { name: 'Cormorant', family: 'Cormorant', category: 'serif', weights: [400, 500, 700] },
  { name: 'Eczar', family: 'Eczar', category: 'serif', weights: [400, 500, 700] },
  { name: 'Alegreya', family: 'Alegreya', category: 'serif', weights: [400, 500, 700] },
  { name: 'Source Serif Pro', family: 'Source Serif 4', category: 'serif', weights: [400, 600, 700] },
  { name: 'Fraunces', family: 'Fraunces', category: 'serif', weights: [400, 500, 700] },
  { name: 'Inknut Antiqua', family: 'Inknut Antiqua', category: 'serif', weights: [400, 500, 700] },
  { name: 'BioRhyme', family: 'BioRhyme', category: 'serif', weights: [400, 700] },
  { name: 'Libre Baskerville', family: 'Libre Baskerville', category: 'serif', weights: [400, 700] },
  { name: 'Playfair Display', family: 'Playfair Display', category: 'serif', weights: [400, 500, 700] },
  { name: 'Lora', family: 'Lora', category: 'serif', weights: [400, 500, 700] },
  { name: 'Spectral', family: 'Spectral', category: 'serif', weights: [400, 500, 700] },
  { name: 'PT Serif', family: 'PT Serif', category: 'serif', weights: [400, 700] },
  { name: 'Cardo', family: 'Cardo', category: 'serif', weights: [400, 700] },
  { name: 'Neuton', family: 'Neuton', category: 'serif', weights: [400, 700] },
  { name: 'Merriweather', family: 'Merriweather', category: 'serif', weights: [400, 700] },

  // Monospace fonts
  { name: 'Space Mono', family: 'Space Mono', category: 'monospace', weights: [400, 700] },
  { name: 'Inconsolata', family: 'Inconsolata', category: 'monospace', weights: [400, 700] },
];

function base64ToUint8Array(base64: string): Uint8Array {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

function uint8ArrayToBase64(bytes: Uint8Array): string {
  let binary = '';
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}

class FontService {
  private loadedGoogleFonts = new Set<string>();
  private loadingGoogleFonts = new Map<string, Promise<void>>();
  private fontLoadCallbacks = new Set<() => void>();

  // System fonts (listed by the Rust font registry)
  private systemFonts: FontDefinition[] = [];
  private systemFontsLoaded = false;
  private systemFontsLoading = false;
  private systemFontLoadCallbacks = new Set<() => void>();

  // Track which fonts have been checked for availability
  private fontPreviewLoaded = new Map<string, boolean>();

  // Real faces per family, from the native font registry.
  private variantCache = new Map<string, Promise<FamilyVariants>>();

  // User-uploaded custom fonts. Keyed by font family name (the filename
  // without extension). The bytes are kept to rebuild the @font-face rules;
  // the Rust engine gets its own copy for layout and PDF embedding.
  private customFonts = new Map<string, FontDefinition>();
  private customFontBytes = new Map<string, Uint8Array>();
  private customFontExt = new Map<string, 'ttf' | 'otf' | 'woff'>();
  private customFontStyleEl: HTMLStyleElement | null = null;
  private customFontsChangedCallbacks = new Set<() => void>();

  /**
   * Get fonts for markdown styles (body, headings, etc.)
   * Installed system fonts once listed; web-safe fonts until then
   */
  getStyleFonts(): FontDefinition[] {
    // Use system fonts once they loaded with actual fonts
    // Fall back to web-safe fonts if system fonts are empty (e.g., shell commands failed in packaged app)
    if (this.systemFontsLoaded && this.systemFonts.length > 0) {
      return this.systemFonts;
    }
    return WEB_SAFE_FONTS;
  }

  /**
   * Get fonts for static page items (text objects)
   * Uses Google Fonts + web-safe fonts (rendered to images, no CORS)
   */
  getItemFonts(): FontDefinition[] {
    return [...GOOGLE_FONTS, ...WEB_SAFE_FONTS];
  }

  /**
   * User-uploaded custom fonts (sorted alphabetically by display name).
   */
  getCustomFonts(): FontDefinition[] {
    return Array.from(this.customFonts.values()).sort((a, b) =>
      a.name.localeCompare(b.name)
    );
  }

  /**
   * Check whether a font name belongs to a user-uploaded custom font.
   */
  isCustomFont(fontName: string): boolean {
    return this.customFonts.has(fontName);
  }

  /**
   * Register a custom font from base64-encoded file content (TTF/OTF/WOFF).
   * The font is installed via @font-face for immediate use in the editor
   * and canvas, and registered with the Rust engine, which measures text
   * and embeds the font in exported PDFs from the same bytes.
   *
   * Returns the family name actually used (filename without extension).
   */
  registerCustomFont(fileName: string, base64Content: string): string {
    const family = this.familyNameFromFileName(fileName);
    const ext = this.fontExtension(fileName);

    const bytes = base64ToUint8Array(base64Content);
    this.customFontBytes.set(family, bytes);
    this.customFontExt.set(family, ext);
    this.customFonts.set(family, {
      name: family,
      family,
      category: this.guessFontCategory(family),
      loaded: true,
    });

    this.injectCustomFontFace(family, ext, base64Content);
    this.notifyCustomFontsChanged();

    // Defer the font-loaded notification until both the browser has parsed
    // the @font-face descriptor and the layout engine has the font. Without
    // this, downstream listeners (App.ts reflows, dropdown previews) can
    // re-measure/re-render before the font is available and end up using
    // the fallback typeface.
    const browserReady = 'fonts' in document
      ? document.fonts.load(`16px "${family}"`).catch(() => undefined)
      : Promise.resolve();
    const engineReady = bridge.fontsRegister(family, base64Content).catch((e) => {
      console.warn(`Layout engine could not load font "${family}":`, e);
    });
    void Promise.all([browserReady, engineReady]).then(() => this.notifyFontLoaded());
    return family;
  }

  /**
   * Remove a previously registered custom font.
   */
  unregisterCustomFont(family: string): void {
    if (!this.customFonts.has(family)) return;
    this.customFonts.delete(family);
    this.customFontBytes.delete(family);
    this.customFontExt.delete(family);
    this.rebuildCustomFontFaces();
    this.notifyCustomFontsChanged();
    void bridge.fontsUnregister(family).catch(() => undefined).then(() => this.notifyFontLoaded());
  }

  /**
   * Subscribe to changes to the custom-fonts list (additions/removals).
   */
  onCustomFontsChanged(callback: () => void): () => void {
    this.customFontsChangedCallbacks.add(callback);
    return () => this.customFontsChangedCallbacks.delete(callback);
  }

  private notifyCustomFontsChanged(): void {
    for (const callback of this.customFontsChangedCallbacks) callback();
  }

  private familyNameFromFileName(fileName: string): string {
    const dot = fileName.lastIndexOf('.');
    return (dot > 0 ? fileName.slice(0, dot) : fileName).trim();
  }

  private fontExtension(fileName: string): 'ttf' | 'otf' | 'woff' {
    const ext = fileName.split('.').pop()?.toLowerCase();
    return ext === 'otf' ? 'otf' : ext === 'woff' ? 'woff' : 'ttf';
  }

  private fontFormatFor(ext: 'ttf' | 'otf' | 'woff'): string {
    return ext === 'otf' ? 'opentype' : ext === 'woff' ? 'woff' : 'truetype';
  }

  private injectCustomFontFace(family: string, ext: 'ttf' | 'otf' | 'woff', base64Content: string): void {
    if (!this.customFontStyleEl) {
      this.customFontStyleEl = document.createElement('style');
      this.customFontStyleEl.dataset.printfoldCustomFonts = 'true';
      document.head.appendChild(this.customFontStyleEl);
    }
    const mime = ext === 'otf' ? 'font/otf' : ext === 'woff' ? 'font/woff' : 'font/ttf';
    const format = this.fontFormatFor(ext);
    const rule = `
@font-face {
  font-family: "${family}";
  src: url(data:${mime};base64,${base64Content}) format("${format}");
  font-display: swap;
}
`;
    this.customFontStyleEl.appendChild(document.createTextNode(rule));
  }

  /**
   * Rebuild the entire custom-font stylesheet from current state. Used after
   * a removal to drop the @font-face rule cleanly.
   */
  private rebuildCustomFontFaces(): void {
    if (!this.customFontStyleEl) return;
    this.customFontStyleEl.textContent = '';
    for (const family of this.customFonts.keys()) {
      const bytes = this.customFontBytes.get(family);
      const ext = this.customFontExt.get(family) ?? 'ttf';
      if (!bytes) continue;
      this.injectCustomFontFace(family, ext, uint8ArrayToBase64(bytes));
    }
  }

  /**
   * Which real faces (regular/bold/italic/bold italic) a family provides.
   * Custom fonts are single-face uploads.
   */
  getFontVariants(fontFamily: string): Promise<FamilyVariants> {
    if (this.customFonts.has(fontFamily)) {
      return Promise.resolve({ regular: true, bold: false, italic: false, boldItalic: false });
    }
    let cached = this.variantCache.get(fontFamily);
    if (!cached) {
      cached = bridge.fontsVariants(fontFamily).catch(() => ({ regular: true, bold: true, italic: true, boldItalic: true }));
      this.variantCache.set(fontFamily, cached);
    }
    return cached;
  }

  /**
   * Check if the system font list has loaded
   * Returns true only if fonts were actually loaded (not empty)
   */
  hasSystemFonts(): boolean {
    return this.systemFontsLoaded && this.systemFonts.length > 0;
  }

  /**
   * Load the system font list from the native font registry (async)
   */
  async loadSystemFonts(): Promise<void> {
    if (this.systemFontsLoaded || this.systemFontsLoading) {
      return;
    }

    this.systemFontsLoading = true;

    try {
      const fonts = await bridge.fontsList();
      if (fonts && Array.isArray(fonts)) {
        this.systemFonts = fonts.map((fontName: string) => ({
          name: fontName,
          family: fontName,
          category: this.guessFontCategory(fontName),
          loaded: false,
        }));
        this.systemFontsLoaded = true;
        this.notifySystemFontsLoaded();
      }
    } catch (error) {
      console.error('Failed to load system fonts:', error);
      // Fall back to web-safe fonts
    } finally {
      this.systemFontsLoading = false;
    }
  }

  /**
   * Guess font category based on name
   */
  private guessFontCategory(fontName: string): 'serif' | 'sans-serif' | 'monospace' | 'display' {
    const lower = fontName.toLowerCase();

    if (lower.includes('mono') || lower.includes('code') || lower.includes('console') ||
        lower.includes('courier') || lower.includes('menlo') || lower.includes('consolas')) {
      return 'monospace';
    }

    if (lower.includes('serif') || lower.includes('times') || lower.includes('georgia') ||
        lower.includes('garamond') || lower.includes('baskerville') || lower.includes('bodoni') ||
        lower.includes('palatino') || lower.includes('cambria')) {
      return 'serif';
    }

    if (lower.includes('display') || lower.includes('decorative') || lower.includes('script') ||
        lower.includes('hand') || lower.includes('brush') || lower.includes('comic')) {
      return 'display';
    }

    return 'sans-serif';
  }

  /**
   * Subscribe to system fonts load event
   */
  onSystemFontsLoaded(callback: () => void): () => void {
    this.systemFontLoadCallbacks.add(callback);
    return () => this.systemFontLoadCallbacks.delete(callback);
  }

  private notifySystemFontsLoaded(): void {
    for (const callback of this.systemFontLoadCallbacks) {
      callback();
    }
  }

  /**
   * Check if a font is a Google Font
   */
  isGoogleFont(fontName: string): boolean {
    return GOOGLE_FONTS.some(f => f.name === fontName || f.family === fontName);
  }

  /**
   * Check if a Google font is loaded
   */
  isGoogleFontLoaded(fontName: string): boolean {
    return this.loadedGoogleFonts.has(fontName) || !this.isGoogleFont(fontName);
  }

  /**
   * Load a single Google Font (for static page items)
   */
  async loadGoogleFont(fontName: string): Promise<void> {
    const font = GOOGLE_FONTS.find(f => f.name === fontName || f.family === fontName);
    if (!font) return; // Not a Google font

    if (this.loadedGoogleFonts.has(font.name)) return;

    // Check if already loading
    if (this.loadingGoogleFonts.has(font.name)) {
      return this.loadingGoogleFonts.get(font.name);
    }

    const loadPromise = this.doLoadGoogleFont(font);
    this.loadingGoogleFonts.set(font.name, loadPromise);

    try {
      await loadPromise;
      this.loadedGoogleFonts.add(font.name);
      this.notifyFontLoaded();
    } finally {
      this.loadingGoogleFonts.delete(font.name);
    }
  }

  /**
   * Load multiple Google fonts at once
   */
  async loadGoogleFonts(fontNames: string[]): Promise<void> {
    const googleFonts = fontNames
      .map(name => GOOGLE_FONTS.find(f => f.name === name || f.family === name))
      .filter((f): f is FontDefinition => f !== undefined)
      .filter(f => !this.loadedGoogleFonts.has(f.name));

    if (googleFonts.length === 0) return;

    // Build Google Fonts URL for batch loading
    const families = googleFonts.map(f => {
      const weights = f.weights?.join(';') || '400;700';
      return `family=${encodeURIComponent(f.family)}:wght@${weights}`;
    }).join('&');

    const url = `https://fonts.googleapis.com/css2?${families}&display=swap`;

    await this.injectStylesheet(url);

    // Wait for fonts to actually be available
    await Promise.all(googleFonts.map(f => this.waitForFont(f.family)));

    for (const font of googleFonts) {
      this.loadedGoogleFonts.add(font.name);
    }
    this.notifyFontLoaded();
  }

  /**
   * Preload all Google Fonts (for font preview in item dropdowns)
   */
  async preloadAllGoogleFonts(): Promise<void> {
    const fontNames = GOOGLE_FONTS.map(f => f.name);
    await this.loadGoogleFonts(fontNames);
  }

  /**
   * Subscribe to font load events
   */
  onFontLoaded(callback: () => void): () => void {
    this.fontLoadCallbacks.add(callback);
    return () => this.fontLoadCallbacks.delete(callback);
  }

  /**
   * Get CSS font-family value with fallbacks
   */
  getFontFamily(fontName: string): string {
    const custom = this.customFonts.get(fontName);
    if (custom) {
      const fallback = custom.category === 'serif' ? 'serif' :
                       custom.category === 'monospace' ? 'monospace' : 'sans-serif';
      return `"${custom.family}", ${fallback}`;
    }
    const allFonts = [...GOOGLE_FONTS, ...WEB_SAFE_FONTS, ...this.systemFonts];
    const font = allFonts.find(f => f.name === fontName || f.family === fontName);
    if (!font) return fontName;

    const fallback = font.category === 'serif' ? 'serif' :
                     font.category === 'monospace' ? 'monospace' : 'sans-serif';

    return `"${font.family}", ${fallback}`;
  }

  /**
   * Check if a font is available in the browser (for preview)
   */
  async checkFontAvailable(fontFamily: string): Promise<boolean> {
    if (this.fontPreviewLoaded.has(fontFamily)) {
      return this.fontPreviewLoaded.get(fontFamily) || false;
    }

    // Use Font Loading API if available
    if ('fonts' in document) {
      try {
        const result = await document.fonts.check(`16px "${fontFamily}"`);
        this.fontPreviewLoaded.set(fontFamily, result);
        return result;
      } catch {
        return false;
      }
    }

    return true; // Assume available if we can't check
  }

  private async doLoadGoogleFont(font: FontDefinition): Promise<void> {
    const weights = font.weights?.join(';') || '400;700';
    const url = `https://fonts.googleapis.com/css2?family=${encodeURIComponent(font.family)}:wght@${weights}&display=swap`;

    await this.injectStylesheet(url);
    await this.waitForFont(font.family);
  }

  private injectStylesheet(url: string): Promise<void> {
    return new Promise((resolve, reject) => {
      // Check if already injected
      const existing = document.querySelector(`link[href="${url}"]`);
      if (existing) {
        resolve();
        return;
      }

      const link = document.createElement('link');
      link.rel = 'stylesheet';
      link.href = url;
      link.onload = () => resolve();
      link.onerror = () => reject(new Error(`Failed to load font: ${url}`));
      document.head.appendChild(link);
    });
  }

  private waitForFont(fontFamily: string): Promise<void> {
    return new Promise((resolve) => {
      // Use the Font Loading API if available
      if ('fonts' in document) {
        document.fonts.load(`16px "${fontFamily}"`).then(() => resolve());
      } else {
        // Fallback: wait a bit and hope it's loaded
        setTimeout(resolve, 100);
      }
    });
  }

  private notifyFontLoaded(): void {
    for (const callback of this.fontLoadCallbacks) {
      callback();
    }
  }
}

export const fontService = new FontService();

// Legacy compatibility exports for gradual migration
export const googleFonts = {
  getAllFonts: () => fontService.getItemFonts(),
  getFontsByCategory: (category: FontDefinition['category']) =>
    fontService.getItemFonts().filter(f => f.category === category),
  isGoogleFont: (fontName: string) => fontService.isGoogleFont(fontName),
  isFontLoaded: (fontName: string) => fontService.isGoogleFontLoaded(fontName),
  loadFont: (fontName: string) => fontService.loadGoogleFont(fontName),
  loadFonts: (fontNames: string[]) => fontService.loadGoogleFonts(fontNames),
  preloadAllFonts: () => fontService.preloadAllGoogleFonts(),
  onFontLoaded: (callback: () => void) => fontService.onFontLoaded(callback),
  getFontFamily: (fontName: string) => fontService.getFontFamily(fontName),
};
