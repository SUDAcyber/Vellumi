// Self-test (VELLUMI_SELFTEST): drives the real shell + editor in a hidden
// window and reports each check to Rust, which writes the report and the
// saved file for scripts/selftest.sh to assert on. Exercises P2/P3 features
// end to end, including the IPC round trips.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { tick } from "svelte";

import { shellAction } from "./actions";
import { editor } from "./editor";
import { exportCards, exportHtml, exportPdf } from "./export";
import { api } from "./ipc";
import { sourceView } from "./source";
import { openPalette, patchSettings, ui } from "./store.svelte";

interface Check {
  name: string;
  ok: boolean;
  detail?: string;
}

const checks: Check[] = [];

function check(name: string, ok: boolean, detail?: unknown): void {
  const entry = { name, ok, detail: detail === undefined ? undefined : JSON.stringify(detail) };
  checks.push(entry);
  window.__vellumiLog?.(ok ? "info" : "error", `${ok ? "ok" : "FAIL"} ${name} ${entry.detail ?? ""}`);
}

async function waitFor(predicate: () => boolean, timeoutMs = 8000): Promise<boolean> {
  const started = performance.now();
  while (!predicate()) {
    if (performance.now() - started > timeoutMs) return false;
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  return true;
}

const markdown = () => editor()?.getMarkdown() ?? "";

async function step(name: string, body: () => Promise<void>): Promise<void> {
  try {
    await body();
  } catch (error) {
    check(`${name} (threw)`, false, String(error));
  }
}

export async function runSelftest(): Promise<void> {
  await step("editor boots with the document", async () => {
    const ready = await waitFor(() => markdown().includes("节奏"));
    check("editor boots with the document", ready, markdown().slice(0, 80));
  });

  await step("doc info", async () => {
    const info = await api.docInfo();
    check("front matter kept out of the editor", info.frontMatter.startsWith("---") && !markdown().includes("title:"), info.frontMatter);
    check("stats computed in Rust", info.stats.words > 10, info.stats);
    check("clean after open", !info.dirty);
  });

  await step("external changes", async () => {
    const external = (action: string, text?: string) =>
      invoke<void>("selftest_external", { action, text: text ?? null });
    await external("snapshot");

    // 1. Clean document + another app saves → silent reload.
    await external("write", "---\ntitle: 原始标题\n---\n\n# 外部修改\n\n别的编辑器写入的内容。\n");
    const reloaded = await waitFor(() => markdown().includes("外部修改"), 6000);
    check("clean document reloads after an external save", reloaded && ui.doc?.dirty === false, markdown().slice(0, 40));
    check("reload shows a notice", ui.toast !== null);

    // 2. Unsaved edits + external save → conflict banner, nothing overwritten.
    await waitFor(() => editor()?.getMarkdown() !== null);
    editor()?.search({ text: "别的编辑器", caseSensitive: false, wholeWord: false, regex: false });
    editor()?.replaceAll("我自己");
    editor()?.clearSearch();
    await waitFor(() => ui.doc?.dirty === true, 6000);
    await external("write", "---\ntitle: 原始标题\n---\n\n# 外部再次修改\n");
    const conflict = await waitFor(() => ui.banner?.kind === "conflict", 6000);
    await tick();
    check("dirty document shows a conflict banner instead of reloading", conflict && markdown().includes("我自己") && document.querySelector(".banner") !== null, ui.banner);

    // 3. Keep mine dismisses; Revert takes the disk version.
    await api.keepMine();
    ui.banner = null;
    const reverted = await api.revert();
    const back = await waitFor(() => markdown().includes("外部再次修改"), 6000);
    check("revert loads the saved version", reverted && back && ui.doc?.dirty === false, markdown().slice(0, 40));

    // Put the fixture back for the remaining checks.
    await external("restore");
    const restored = await waitFor(() => markdown().includes("节奏") && ui.doc?.dirty === false, 6000);
    check("external restore reloads the fixture", restored);
  });

  await step("images", async () => {
    const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";
    const bytes = Uint8Array.from(atob(png), (c) => c.charCodeAt(0));
    const upload = (window.moltenAPI as unknown as { uploadImageForTest(file: File): Promise<string> }).uploadImageForTest;
    const first = await upload(new File([bytes], "截图 测试.png", { type: "image/png" }));
    const second = await upload(new File([bytes], "截图 测试.png", { type: "image/png" }));
    check("pasted image is saved next to the document", first === "assets/%E6%88%AA%E5%9B%BE-%E6%B5%8B%E8%AF%95.png", first);
    check("a second paste gets a unique name", second === "assets/%E6%88%AA%E5%9B%BE-%E6%B5%8B%E8%AF%95-1.png", second);
    const loads = (src: string) =>
      new Promise<boolean>((resolve) => {
        const img = new Image();
        img.onload = () => resolve(img.naturalWidth === 1);
        img.onerror = () => resolve(false);
        img.src = src;
      });
    check("saved image displays through the asset protocol", await loads(`vellumi-asset://localhost/${first}`));
    check("asset protocol refuses paths outside the folder", !(await loads("vellumi-asset://localhost/..%2F..%2Fetc%2Fhosts")));
  });

  await step("selection and toolbar", async () => {
    editor()?.focus();
    document.execCommand("selectAll");
    const marked = await waitFor(() => document.querySelector(".milkdown-code-block.vellumi-block-selected, .vellumi-block-selected .milkdown-code-block, .vellumi-block-selected") !== null, 3000);
    check("select all marks the code block as selected", marked && document.querySelector(".vellumi-block-selected") !== null);
    const buttons = [...document.querySelectorAll<HTMLButtonElement>(".milkdown-toolbar button.toolbar-item")];
    const titles = buttons.map((button) => button.title);
    check("toolbar has the design's six buttons with tooltips", buttons.length === 6 && titles.every(Boolean) && titles[5].includes("⇧⌘Q"), titles);
    check("toolbar has one divider (B I S </> | link quote)", document.querySelectorAll(".milkdown-toolbar .divider").length === 1);
    // Rendered styles, not just structure: Crepe's own stylesheet loads after
    // ours and once silently won.
    const bar = document.querySelector<HTMLElement>(".milkdown-toolbar");
    const icon = document.querySelector<SVGElement>(".milkdown-toolbar .toolbar-item:not(.active) svg");
    const barStyle = bar ? getComputedStyle(bar) : null;
    const iconStyle = icon ? getComputedStyle(icon) : null;
    check("toolbar renders as the dark pill", barStyle?.backgroundColor === "rgb(31, 32, 35)" && barStyle.borderRadius === "10px", barStyle?.backgroundColor);
    check("toolbar icons are 16px outline icons", iconStyle?.width === "16px" && iconStyle.fill === "none" && iconStyle.color === "rgb(241, 242, 244)", {
      width: iconStyle?.width,
      fill: iconStyle?.fill,
      color: iconStyle?.color,
    });
    const surface = document.querySelector<HTMLElement>("#editor .milkdown");
    check("editor column has no background band of its own", surface ? getComputedStyle(surface).backgroundColor === "rgba(0, 0, 0, 0)" : false, surface && getComputedStyle(surface).backgroundColor);
    const selected = document.querySelector<HTMLElement>(".vellumi-block-selected");
    check("selected block has a visible outline", selected ? getComputedStyle(selected).outlineWidth === "2px" : false);
    window.getSelection()?.collapseToStart();
  });

  await step("outline panel", async () => {
    await shellAction("view.outline");
    const ok = await waitFor(() => ui.outline.length >= 2);
    await tick();
    check("outline lists headings", ok && document.querySelectorAll("[data-heading]").length >= 2, ui.outline);
  });

  await step("files panel", async () => {
    await shellAction("view.files");
    const nodes = await api.tree();
    check("file tree includes the document", JSON.stringify(nodes).includes(ui.doc?.name ?? "?"), nodes.length);
    // The panel loads its tree asynchronously: wait for rows, not one frame.
    const rendered = await waitFor(() => document.querySelectorAll(".sidebar .row").length > 0, 5000);
    check("file tree renders", rendered);
  });

  await step("find", async () => {
    await shellAction("edit.find");
    await tick();
    check("find bar opens", document.querySelector(".find-bar") !== null);
    const api2 = editor();
    const all = api2?.search({ text: "节奏", caseSensitive: false, wholeWord: false, regex: false });
    check("find counts every match, across formatting", all?.total === 3, all);
    const next = api2?.searchStep(false);
    check("find steps to the next match", (next?.current ?? 0) >= 1 && next?.total === 3, next);
    const cased = api2?.search({ text: "Rhythm", caseSensitive: true, wholeWord: false, regex: false });
    check("case-sensitive find", cased?.total === 1, cased);
    const insensitive = api2?.search({ text: "Rhythm", caseSensitive: false, wholeWord: false, regex: false });
    check("case-insensitive find", insensitive?.total === 2, insensitive);
    const regex = api2?.search({ text: "L\\d+", caseSensitive: true, wholeWord: false, regex: true });
    check("regex find", regex?.total === 2, regex);
  });

  await step("replace all + undo", async () => {
    const api2 = editor();
    api2?.search({ text: "节奏", caseSensitive: false, wholeWord: false, regex: false });
    const count = api2?.replaceAll("韵律");
    await waitFor(() => !markdown().includes("节奏"));
    check("replace all replaces every match", count === 3 && markdown().includes("韵律") && !markdown().includes("节奏"), { count });
    // While the find field has focus, ⌘Z undoes typing in the field (as in
    // any Mac app); move focus back to the text like a user would.
    const field = document.querySelector<HTMLInputElement>("#find-text");
    window.__vellumiHistory?.("undo");
    check("⌘Z in the find field stays in the field", document.activeElement !== field || markdown().includes("韵律"));
    editor()?.focus();
    window.__vellumiHistory?.("undo");
    const undone = await waitFor(() => markdown().includes("节奏") && !markdown().includes("韵律"));
    check("replace all undoes in one step", undone, markdown().slice(0, 120));
    await shellAction("edit.find"); // keep open; close via Escape path below
    ui.findOpen = false;
    api2?.clearSearch();
  });

  await step("source mode", async () => {
    await shellAction("view.source");
    const entered = await waitFor(() => ui.sourceMode && sourceView() !== null);
    const view = sourceView();
    const text = view?.state.doc.toString() ?? "";
    check("source mode shows the full file", entered && text.startsWith("---") && text.includes("节奏"), text.slice(0, 60));
    view?.dispatch({ changes: { from: view.state.doc.length, insert: "\n源码模式追加的一行\n" } });
    window.__vellumiHistory?.("undo");
    check("undo in source mode goes to CodeMirror", !(view?.state.doc.toString() ?? "").includes("源码模式追加"));
    view?.dispatch({ changes: { from: view.state.doc.length, insert: "\n源码模式追加的一行\n" } });
    await shellAction("view.source");
    const exited = await waitFor(() => !ui.sourceMode && markdown().includes("源码模式追加的一行"));
    check("leaving source mode carries the edit into the editor", exited, markdown().slice(-60));
  });

  await step("front matter", async () => {
    const info = await api.setFrontMatter("---\ntitle: 自测\ntags: [a, b]\n---\n");
    check("front matter edit applies", info.frontMatter === "---\ntitle: 自测\ntags: [a, b]\n---\n" && info.dirty, info.frontMatter);
    let refused = false;
    try {
      await api.setFrontMatter("title: no fences");
    } catch {
      refused = true;
    }
    check("invalid front matter is refused", refused);
    await tick();
    check("front matter chips render", document.querySelectorAll(".front-matter-bar .chip").length >= 3);
  });

  await step("palette", async () => {
    openPalette("commands");
    const shown = await waitFor(() => document.querySelectorAll(".palette .result").length > 10);
    check("command palette lists commands", shown, document.querySelectorAll(".palette .result").length);
    ui.paletteMode = "files";
    const files = await waitFor(() => document.querySelectorAll(".palette .result").length > 0);
    check("palette finds files", files);
    ui.paletteOpen = false;
  });

  await step("settings", async () => {
    await patchSettings({ typewriter: true, statsMode: 1 });
    check("settings patch round-trips", ui.settings?.typewriter === true && ui.settings.statsMode === 1, ui.settings);
    await tick();
    check("status bar follows the stats mode", (document.querySelector(".status-bar")?.textContent ?? "").match(/characters|字符/) !== null);
    await patchSettings({ typewriter: false, statsMode: 0 });
  });

  await step("save", async () => {
    await window.__vellumiFlush?.("save");
    const saved = await waitFor(() => ui.doc?.dirty === false);
    check("save clears the dirty flag", saved, ui.doc);
    const hits = await api.search("源码模式追加的一行", false);
    check("find in folder sees the saved text", hits.length === 1, hits);
  });

  await step("export", async () => {
    const read = (name: string) =>
      invoke<{ exists: boolean; size?: number; text?: string; magic?: string }>("selftest_read", { name });
    // Export while editing state is visible: find highlights + a selection.
    editor()?.search({ text: "节奏", caseSensitive: false, wholeWord: false, regex: false });
    editor()?.focus();
    document.execCommand("selectAll");
    const clean = (editor() as unknown as { getContentHTML(): string }).getContentHTML();
    check("exported HTML carries no editing state", !/virtual-cursor|vellumi-match|vellumi-block-selected|ProseMirror-selectednode/.test(clean) && clean.includes("节奏"), clean.match(/class="[^"]*(cursor|match|selected)[^"]*"/)?.[0]);
    check("export starts with the document's first block", clean.trimStart().startsWith("<h1"), clean.slice(0, 60));
    editor()?.clearSearch();
    window.getSelection()?.collapseToStart();
    await exportHtml();
    const html = await read("export-test.html");
    const text = html.text ?? "";
    check("HTML export writes a standalone page", html.exists && text.startsWith("<!doctype html>") && text.includes("<title>doc</title>"), text.slice(0, 80));
    check("[toc] expands into linked headings", text.includes('<nav class="vellumi-toc">') && text.includes('id="慢写作笔记"') && text.includes('href="#为什么要慢"'));
    check("code blocks export as <pre><code>", /<pre><code[^>]*>print\("hello"\)/.test(text));
    check("math fonts are embedded, not linked (B21)", text.includes("data:font/woff2;base64,") && !/url\((?!data:)[^)]*KaTeX/i.test(text));
    await exportPdf();
    const pdf = await read("export-test.pdf");
    check("PDF export renders a real PDF offscreen", pdf.magic === "25504446" && (pdf.size ?? 0) > 2000, { magic: pdf.magic, size: pdf.size });
    const written = new Promise<number>((resolve) => {
      void getCurrentWebviewWindow().listen<number>("cards:written", (event) => resolve(event.payload));
      setTimeout(() => resolve(-1), 30000);
    });
    await exportCards();
    const count = await written;
    const png = await read(count === 1 ? "doc.png" : "doc-01.png");
    const last = await read(`doc-${String(count).padStart(2, "0")}.png`);
    check("image cards render and save", count >= 1 && png.magic === "89504e47" && (png.size ?? 0) > 10000, { count, magic: png.magic, size: png.size });
    check("multi-card exports are numbered", count === 1 || last.magic === "89504e47", { count, last: last.exists });
  });

  await step("projects and index", async () => {
    const folder = ui.doc?.folder;
    if (!folder) {
      check("projects need a saved document", false);
      return;
    }
    const added = await api.addProject(folder);
    check("a folder becomes a project", added?.path !== undefined, added);
    let status = await api.indexStatus();
    const started = performance.now();
    while (status.state !== "ready" && performance.now() - started < 15000) {
      await new Promise((resolve) => setTimeout(resolve, 100));
      status = await api.indexStatus();
    }
    check("the project gets indexed", status.state === "ready" && status.files >= 1, status);
    const list = await api.projects();
    check("project card lists its files", list.length === 1 && list[0].fileCount >= 1 && list[0].recent.length >= 1, list[0]);
    const hits = await api.search("源码模式追加的一行", false);
    check("indexed search finds saved text (Chinese substring)", hits.length === 1 && hits[0].matches[0].text.includes("源码模式追加的一行"), hits);
    const latin = await api.search("hyth", false);
    check("indexed search matches inside words", latin.length === 1, latin.length);
    const files = await api.quickOpen("doc");
    check("quick open finds project files", files.some((f) => f.name === "doc.md"), files.map((f) => f.name));
    await api.removeProject(added?.id ?? "");
    check("project removal", (await api.projects()).length === 0);
  });

  const report = { passed: checks.filter((c) => c.ok).length, failed: checks.filter((c) => !c.ok).length, checks };
  await api.selftestDone(report);
}

declare global {
  interface Window {
    __vellumiLog?: (level: string, message: string) => void;
    __vellumiFlush?: (reason: string) => Promise<void>;
    __vellumiHistory?: (direction: string) => boolean | undefined;
  }
}
