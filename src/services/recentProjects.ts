/**
 * RecentProjects Service
 *
 * Recently opened projects for the welcome screen, persisted natively in
 * the app data folder (`recents.json`). On iPadOS the list also includes
 * every project in PrintFold's Documents folder.
 */

import { bridge, type RecentEntry } from './bridge';

export type { RecentEntry };

export const recentProjects = {
  list(): Promise<RecentEntry[]> {
    return bridge.recentsList();
  },

  add(path: string, name: string): Promise<void> {
    return bridge.recentsAdd(path, name);
  },

  remove(entry: RecentEntry): Promise<void> {
    return bridge.recentsRemove(entry.path);
  },
};
