// Document window entry: render the shell, then load the editor into the
// #editor element it provides, then wire Rust events.
import { invoke } from "@tauri-apps/api/core";
import { mount } from "svelte";

import App from "./App.svelte";
import { shellAction } from "./lib/actions";
import { editor, loadEditor } from "./lib/editor";
import { isChinese, t } from "./lib/i18n";
import { api, on, type DocInfo, type ExternalChange, type Settings } from "./lib/ipc";
import { replaceSourceText } from "./lib/source";
import { setDoc, setSettings, toast, ui } from "./lib/store.svelte";
import "./styles/app.css";

const target = document.getElementById("app");
if (target) mount(App, { target });

on<DocInfo>("doc:state", (info) => {
  setDoc(info);
  if (!ui.editorReady && editor()?.getMarkdown() !== null) ui.editorReady = true;
  if (ui.findOpen && !ui.sourceMode) ui.findResult = editor()?.searchState() ?? ui.findResult;
});
on<Settings>("settings:changed", setSettings);
on<string>("shell:action", (id) => void shellAction(id));
on<number>("cards:written", (count) => toast(isChinese ? `已导出 ${count} 张图片卡` : `Exported ${count} image cards`));
on<ExternalChange>("doc:external", (change) => {
  if (change.kind === "reloaded") {
    if (ui.sourceMode) replaceSourceText(change.fullText);
    ui.banner = null;
    toast(t.reloaded);
  } else {
    ui.banner = change;
  }
});

async function boot(): Promise<void> {
  setSettings(await api.settings());
  await loadEditor();
  // The editor module posted "ready"; Rust answers with setMarkdown. Settings
  // are applied again now that moltenAPI exists.
  if (ui.settings) setSettings(ui.settings);
  setDoc(await api.docInfo());
  const migrated = await invoke<string | null>("take_migration_notice");
  if (migrated) toast(migrated);
  if (window.__vellumiSelftest) {
    const { runSelftest } = await import("./lib/selftest");
    void runSelftest();
  }
}

declare global {
  interface Window {
    __vellumiSelftest?: boolean;
  }
}

void boot();
