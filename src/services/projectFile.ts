/**
 * ProjectFile Service
 *
 * Tracks the on-disk `.printfold` file backing the open project. The path
 * itself is bound on the Rust side (it performs the atomic writes); the UI
 * keeps the display name and whether a file is bound.
 */

class ProjectFileService {
  private path: string | null = null;
  private name = 'Untitled';

  hasFile(): boolean {
    return this.path !== null;
  }

  getName(): string {
    return this.name;
  }

  getPath(): string | null {
    return this.path;
  }

  bind(path: string, name: string): void {
    this.path = path;
    this.name = name;
  }

  clear(): void {
    this.path = null;
    this.name = 'Untitled';
  }
}

export const projectFile = new ProjectFileService();
