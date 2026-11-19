// Typed wrappers over the Rust commands (src-tauri/src/commands.rs) and the
// events the Rust side emits. Keep in sync with the Rust structs.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

export interface Stats {
  words: number;
  characters: number;
  paragraphs: number;
  readingMinutes: number;
}

export interface DocInfo {
  name: string;
  path: string | null;
  folder: string | null;
  encoding: string;
  frontMatter: string;
  dirty: boolean;
  sourceMode: boolean;
  normalizedOnLoad: boolean;
  stats: Stats;
}

export type Appearance = "system" | "light" | "dark";
export type SidebarPanel = "files" | "outline" | "search";

export interface Settings {
  appearance: Appearance;
  theme: "frame" | "nord" | "classic";
  zoom: number;
  typewriter: boolean;
  focusMode: boolean;
  spellcheck: boolean;
  smartPunctuation: boolean;
  fontScheme: "default" | "serif" | "sans";
  lineHeight: number;
  paragraphSpacing: number;
  lineWidth: number;
  wordGoal: number;
  imageFolder: string;
  sidebarVisible: boolean;
  sidebarPanel: SidebarPanel;
  statsMode: number;
  hasShownWelcome: boolean;
  recents: string[];
  projects: Project[];
  draftsFolder: string | null;
}

export interface Project {
  id: string;
  name: string;
  path: string;
  addedMs: number;
}

export interface ProjectInfo extends Project {
  available: boolean;
  fileCount: number;
  recent: { name: string; path: string; modifiedMs: number }[];
}

export interface IndexStatus {
  state: "starting" | "indexing" | "ready" | "unavailable";
  files: number;
}

export interface TreeNode {
  name: string;
  path: string;
  isDir: boolean;
  children: TreeNode[];
}

export interface FileMatches {
  path: string;
  name: string;
  matches: { line: number; text: string }[];
}

export interface FileHit {
  name: string;
  path: string;
  folder: string;
  recent: boolean;
}

export type ExternalChange =
  | { kind: "reloaded"; body: string; fullText: string }
  | { kind: "conflict" }
  | { kind: "deleted" };

export interface IpcError {
  kind: string;
  message: string;
}

export const api = {
  docInfo: () => invoke<DocInfo>("doc_info"),
  setFrontMatter: (text: string) => invoke<DocInfo>("doc_set_front_matter", { text }),
  sourceEnter: (markdown: string | null) => invoke<string>("doc_source_enter", { markdown }),
  sourceChanged: (text: string) => invoke<void>("doc_source_changed", { text }),
  sourceExit: (text: string) => invoke<string>("doc_source_exit", { text }),
  revert: () => invoke<boolean>("doc_revert"),
  reloadFromDisk: () => invoke<boolean>("doc_reload_from_disk"),
  keepMine: () => invoke<void>("doc_keep_mine"),
  tree: (folder?: string) => invoke<TreeNode[]>("tree", { folder: folder ?? null }),
  search: (query: string, caseSensitive: boolean) => invoke<FileMatches[]>("search", { query, caseSensitive }),
  quickOpen: (query: string) => invoke<FileHit[]>("quick_open", { query }),
  openPath: (path: string) => invoke<void>("open_path", { path }),
  reveal: (path: string) => invoke<void>("reveal_in_finder", { path }),
  settings: () => invoke<Settings>("settings_get"),
  patchSettings: (patch: Partial<Settings>) => invoke<Settings>("settings_patch", { patch }),
  runCommand: (id: string) => invoke<void>("run_command", { id }),
  indexStatus: () => invoke<IndexStatus>("index_status"),
  projects: () => invoke<ProjectInfo[]>("projects_list"),
  addProject: (path?: string) => invoke<Project | null>("projects_add", { path: path ?? null }),
  removeProject: (id: string) => invoke<void>("projects_remove", { id }),
  renameProject: (id: string, name: string) => invoke<void>("projects_rename", { id, name }),
  relocateProject: (id: string) => invoke<boolean>("projects_relocate", { id }),
  newNote: (id: string) => invoke<void>("project_new_note", { id }),
  pickDrafts: (clear: boolean) => invoke<Settings>("settings_pick_drafts", { clear }),
  openProjects: () => invoke<void>("open_projects_window"),
  selftestDone: (report: unknown) => invoke<void>("selftest_done", { report }),
};

/** Listens to an event targeted at this window (or broadcast). */
export function on<T>(event: string, handler: (payload: T) => void): void {
  void getCurrentWebviewWindow().listen<T>(event, (e) => handler(e.payload));
}

export function errorMessage(error: unknown): string {
  if (error && typeof error === "object" && "message" in error) return String((error as IpcError).message);
  return String(error);
}
