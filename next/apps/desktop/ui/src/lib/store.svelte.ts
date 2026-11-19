// Window-wide UI state (Svelte 5 runes) and the actions that change it.
// Rust is the source of truth for the document and settings; this mirrors
// them and adds purely visual state (open panels, find bar, palette).
import { api, type DocInfo, type ExternalChange, type Settings, type SidebarPanel } from "./ipc";
import { applyEditorSettings, editor, type Heading } from "./editor";

export type PaletteMode = "files" | "commands" | "text";

export const ui = $state({
  doc: null as DocInfo | null,
  settings: null as Settings | null,
  editorReady: false,
  outline: [] as Heading[],
  findOpen: false,
  findFocusToken: 0,
  findResult: { total: 0, current: 0 },
  paletteOpen: false,
  paletteMode: "files" as PaletteMode,
  frontMatterOpen: false,
  sourceMode: false,
  banner: null as ExternalChange | null,
  toast: null as string | null,
  normalizationNoticeShown: false,
  showNormalizationNotice: false,
  searchFocusToken: 0,
});

let toastTimer: ReturnType<typeof setTimeout> | undefined;

export function toast(message: string): void {
  ui.toast = message;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (ui.toast = null), 3000);
}

export function setDoc(info: DocInfo): void {
  ui.doc = info;
  ui.sourceMode = info.sourceMode;
  if (info.normalizedOnLoad && !ui.normalizationNoticeShown) {
    ui.normalizationNoticeShown = true;
    ui.showNormalizationNotice = true;
    setTimeout(() => (ui.showNormalizationNotice = false), 10_000);
  }
  scheduleOutline();
}

export function setSettings(settings: Settings): void {
  ui.settings = settings;
  applyEditorSettings(settings);
}

export async function patchSettings(patch: Partial<Settings>): Promise<void> {
  setSettings(await api.patchSettings(patch));
}

let outlineTimer: ReturnType<typeof setTimeout> | undefined;

/** Outline refresh is cheap but runs only while the panel is visible. */
export function scheduleOutline(): void {
  clearTimeout(outlineTimer);
  outlineTimer = setTimeout(() => {
    const visible = ui.settings?.sidebarVisible && ui.settings.sidebarPanel === "outline";
    if (visible || ui.paletteOpen) ui.outline = editor()?.getOutline() ?? [];
  }, 200);
}

/** ⌥⌘1 / ⌥⌘2 / ⇧⌘F: show that panel, or hide the sidebar if it is showing. */
export async function togglePanel(panel: SidebarPanel): Promise<void> {
  const settings = ui.settings;
  if (!settings) return;
  const showing = settings.sidebarVisible && settings.sidebarPanel === panel;
  await patchSettings({ sidebarVisible: !showing, sidebarPanel: panel });
  if (panel === "outline") ui.outline = editor()?.getOutline() ?? [];
  if (panel === "search" && !showing) ui.searchFocusToken += 1;
}

export async function showPanel(panel: SidebarPanel): Promise<void> {
  await patchSettings({ sidebarVisible: true, sidebarPanel: panel });
  if (panel === "outline") ui.outline = editor()?.getOutline() ?? [];
  if (panel === "search") ui.searchFocusToken += 1;
}

export function openFind(): void {
  ui.findOpen = true;
  ui.findFocusToken += 1;
}

export function openPalette(mode: PaletteMode = "files"): void {
  ui.paletteMode = mode;
  ui.paletteOpen = true;
  scheduleOutline();
}
