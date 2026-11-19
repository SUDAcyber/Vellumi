// Export (P6): gathers the editor's cleaned HTML and the stylesheets the page
// needs, and hands both to Rust (src-tauri/src/export.rs), which assembles
// and writes the file.
import { invoke } from "@tauri-apps/api/core";

import { editor } from "./editor";
import { isChinese } from "./i18n";
import { errorMessage } from "./ipc";
import { exitSource } from "./source";
import { toast, ui } from "./store.svelte";

const L = (zh: string, en: string) => (isChinese ? zh : en);

function arrayBufferToBase64(buffer: ArrayBuffer): string {
  const bytes = new Uint8Array(buffer);
  let binary = "";
  for (let i = 0; i < bytes.length; i += 0x8000) {
    binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  }
  return btoa(binary);
}

const FONT_FACE = /@font-face\s*{[^}]*}/g;
const URL_REF = /url\((['"]?)([^'")]+)\1\)/g;

/** Inline `url(...)` fonts as data: URIs (woff2 only; the other formats are
 *  dropped from `src`, every engine that renders exports reads woff2). */
async function inlineFonts(css: string, base: string): Promise<string> {
  const cache = new Map<string, string>();
  const blocks = css.match(FONT_FACE) ?? [];
  let out = css;
  for (const block of blocks) {
    let rewritten = block;
    for (const match of block.matchAll(URL_REF)) {
      const ref = match[2];
      if (ref.startsWith("data:")) continue;
      if (!/\.woff2(\?|#|$)/.test(ref)) {
        // e.g. `url(x.woff) format("woff")` inside a src list: drop the item.
        rewritten = rewritten.replace(new RegExp(`,?\\s*url\\(${match[1]}${ref.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}${match[1]}\\)\\s*format\\([^)]*\\)`), "");
        continue;
      }
      const absolute = new URL(ref, base).href;
      let data = cache.get(absolute);
      if (!data) {
        const response = await fetch(absolute);
        data = `data:font/woff2;base64,${arrayBufferToBase64(await response.arrayBuffer())}`;
        cache.set(absolute, data);
      }
      rewritten = rewritten.replace(match[0], `url(${data})`);
    }
    out = out.replace(block, rewritten);
  }
  return out;
}

/** Every stylesheet the export page needs, self-contained. KaTeX fonts are
 *  embedded only when the document has math (audit B21: they were relative
 *  URLs that 404'd next to the exported file); otherwise dropped. */
export async function collectCss(withMath: boolean): Promise<string> {
  const theme = ui.settings?.theme ?? "frame";
  const sheets = [
    ...Array.from(document.querySelectorAll<HTMLLinkElement>('link[rel="stylesheet"]'))
      .map((link) => link.href)
      .filter((href) => href.includes("/assets/")),
    new URL(`themes/${theme}-light.css`, document.baseURI).href,
  ];
  const parts: string[] = [];
  for (const href of sheets) {
    const css = await (await fetch(href)).text();
    if (withMath) {
      parts.push(await inlineFonts(css, href));
    } else {
      parts.push(css.replace(FONT_FACE, (block) => (/KaTeX/i.test(block) ? "" : block)));
    }
  }
  return parts.join("\n");
}

/** Cleaned body HTML, leaving source mode first (the parked editor would
 *  otherwise export pre-⌘/ content — the reason 0.5.x disabled exports). */
async function body(): Promise<string | null> {
  if (ui.sourceMode) await exitSource();
  return (editor() as unknown as { getContentHTML(): string | null } | undefined)?.getContentHTML() ?? null;
}

async function withPage(command: string, success: (path: string) => string): Promise<void> {
  try {
    const html = await body();
    if (html === null) return;
    const css = await collectCss(html.includes('class="katex'));
    const path = await invoke<string | null>(command, { bodyHtml: html, css });
    if (path) toast(success(path));
  } catch (error) {
    toast(`${L("导出失败：", "Export failed: ")}${errorMessage(error)}`);
  }
}

const shortPath = (path: string) => path.replace(/^\/Users\/[^/]+/, "~");

export function exportHtml(): Promise<void> {
  return withPage("export_html", (path) => L(`已导出 HTML：${shortPath(path)}`, `Exported HTML: ${shortPath(path)}`));
}

export function exportPdf(): Promise<void> {
  toast(L("正在生成 PDF…", "Rendering PDF…"));
  return withPage("export_pdf", (path) => L(`已导出 PDF：${shortPath(path)}`, `Exported PDF: ${shortPath(path)}`));
}

export async function printDocument(): Promise<void> {
  try {
    const html = await body();
    if (html === null) return;
    const css = await collectCss(html.includes('class="katex'));
    await invoke("print_document", { bodyHtml: html, css });
  } catch (error) {
    toast(`${L("打印失败：", "Printing failed: ")}${errorMessage(error)}`);
  }
}

export async function exportCards(): Promise<string | null> {
  try {
    const html = await body();
    if (html === null) return null;
    return await invoke<string>("cards_open", { bodyHtml: html });
  } catch (error) {
    toast(`${L("无法打开图片卡导出：", "Cannot open image cards: ")}${errorMessage(error)}`);
    return null;
  }
}
