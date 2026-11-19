// Typed access to the editing surface (packages/editor/src/main.js exposes
// window.moltenAPI). The module loads lazily, after the shell has rendered
// the #editor element it mounts into.
import type { Settings } from "./ipc";

export interface SearchQuery {
  text: string;
  caseSensitive: boolean;
  wholeWord: boolean;
  regex: boolean;
}

export interface SearchResult {
  total: number;
  current: number;
}

export interface Heading {
  level: number;
  text: string;
  pos: number;
}

interface MoltenAPI {
  setMarkdown(markdown: string): void;
  getMarkdown(): string | null;
  focus(): void;
  setTheme(name: string): void;
  setTypewriter(on: boolean): void;
  setFocusMode(on: boolean): void;
  setSpellcheck(on: boolean): void;
  setSmartPunctuation(on: boolean): void;
  setTypography(config: { scheme: string; lineHeight: number; paragraphSpacing: number; maxWidth: number }): void;
  getOutline(): Heading[];
  scrollToHeading(pos: number): boolean;
  search(query: SearchQuery): SearchResult;
  searchStep(backwards: boolean): SearchResult;
  searchState(): SearchResult;
  clearSearch(): void;
  replaceCurrent(replacement: string): SearchResult;
  replaceAll(replacement: string): number;
  getScrollFraction(): number;
  setScrollFraction(fraction: number): void;
}

declare global {
  interface Window {
    moltenAPI?: MoltenAPI;
  }
}

export async function loadEditor(): Promise<void> {
  await import("@vellumi/editor");
}

export function editor(): MoltenAPI | undefined {
  return window.moltenAPI;
}

/** Editor-level settings (Rust applies zoom and light/dark itself). */
export function applyEditorSettings(settings: Settings): void {
  const api = editor();
  if (!api) return;
  api.setTheme(settings.theme);
  api.setTypewriter(settings.typewriter);
  api.setFocusMode(settings.focusMode);
  api.setSpellcheck(settings.spellcheck);
  api.setSmartPunctuation(settings.smartPunctuation);
  api.setTypography({
    scheme: settings.fontScheme,
    lineHeight: settings.lineHeight,
    paragraphSpacing: settings.paragraphSpacing,
    maxWidth: settings.lineWidth,
  });
}
