/**
 * Project cards for the project browser: thumbnail, name and details.
 * Thumbnails are read from the `.printfold` archives on demand and cached
 * as object URLs per file version.
 */

import { bridge, type LibraryEntry } from '../../services/bridge';

const thumbnailUrls = new Map<string, Promise<string | null>>();

function thumbnailKey(entry: LibraryEntry): string {
  return `${entry.path}\u0000${entry.modified}`;
}

function loadThumbnail(entry: LibraryEntry): Promise<string | null> {
  const key = thumbnailKey(entry);
  let url = thumbnailUrls.get(key);
  if (!url) {
    url = bridge
      .libraryThumbnail(entry.path)
      .then(bytes => (bytes.length > 0 ? URL.createObjectURL(new Blob([bytes as BlobPart], { type: 'image/png' })) : null))
      .catch(() => null);
    thumbnailUrls.set(key, url);
  }
  return url;
}

/** Drop cached thumbnails for files that changed or disappeared. */
export function pruneThumbnails(entries: LibraryEntry[]): void {
  const live = new Set(entries.map(thumbnailKey));
  for (const [key, url] of Array.from(thumbnailUrls.entries())) {
    if (live.has(key)) continue;
    thumbnailUrls.delete(key);
    void url.then(u => u && URL.revokeObjectURL(u));
  }
}

export function formatRelativeTime(ts: number): string {
  const seconds = Math.floor((Date.now() - ts) / 1000);
  if (seconds < 60) return 'Just now';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days} d ago`;
  return new Date(ts).toLocaleDateString();
}

export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function parentFolder(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.length >= 2 ? parts[parts.length - 2] : '';
}

export function createCard(entry: LibraryEntry, selected: boolean): HTMLElement {
  const card = document.createElement('div');
  card.className = 'project-card';
  card.classList.toggle('selected', selected);
  card.dataset.path = entry.path;
  card.tabIndex = -1;
  card.setAttribute('role', 'option');
  card.setAttribute('aria-selected', selected ? 'true' : 'false');

  const thumb = document.createElement('div');
  thumb.className = 'card-thumb';
  const page = document.createElement('div');
  page.className = 'card-page placeholder';
  thumb.appendChild(page);
  void loadThumbnail(entry).then(url => {
    if (!url) return;
    const img = document.createElement('img');
    img.alt = '';
    img.draggable = false;
    img.src = url;
    page.classList.remove('placeholder');
    page.appendChild(img);
  });

  const more = document.createElement('button');
  more.type = 'button';
  more.className = 'card-more';
  more.title = 'Actions';
  more.setAttribute('aria-label', `Actions for ${entry.name}`);
  more.textContent = '⋯';
  thumb.appendChild(more);

  const name = document.createElement('div');
  name.className = 'card-name';
  name.textContent = entry.name;
  name.title = entry.fileName;

  const meta = document.createElement('div');
  meta.className = 'card-meta';
  meta.textContent = `${formatRelativeTime(entry.modified)} · ${formatSize(entry.size)}`;
  if (!entry.inLibrary) {
    const where = document.createElement('span');
    where.className = 'card-location';
    where.textContent = parentFolder(entry.path);
    where.title = entry.path;
    meta.append(' · ', where);
  }

  card.append(thumb, name, meta);
  return card;
}
