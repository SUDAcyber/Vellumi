<script lang="ts">
  // Export as image cards (design canvas: Cards artboard). The renderer is the
  // 0.5.x card.html (public/imagecard), running in an iframe; it reports back
  // through window.__vellumiCardBridge. Rust writes the files (cards_write).
  import { invoke } from "@tauri-apps/api/core";
  import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
  import { onMount } from "svelte";

  import { isChinese } from "./lib/i18n";
  import { errorMessage } from "./lib/ipc";

  const L = (zh: string, en: string) => (isChinese ? zh : en);

  type Theme = "aurora" | "cream" | "dark" | "forest" | "minimal" | "sunset";
  type Size = "story" | "portrait" | "iphonePlus";
  interface Options {
    theme: Theme;
    size: Size;
    format: "png" | "jpeg";
    indent: "0em" | "2em";
    watermark: string;
    output: "folder" | "zip";
  }

  const THEMES: { id: Theme; label: string; swatch: string }[] = [
    { id: "aurora", label: L("极光", "Aurora"), swatch: "#1f3247" },
    { id: "cream", label: L("奶油", "Cream"), swatch: "#f3e6cc" },
    { id: "dark", label: L("深色", "Dark"), swatch: "#16181d" },
    { id: "forest", label: L("森林", "Forest"), swatch: "#1e3a2f" },
    { id: "minimal", label: L("极简", "Minimal"), swatch: "#f7f3ec" },
    { id: "sunset", label: L("日落", "Sunset"), swatch: "#6b3326" },
  ];
  const SIZES: { id: Size; label: string; width: number; height: number }[] = [
    { id: "story", label: L("故事 9:16", "Story 9:16"), width: 1080, height: 1920 },
    { id: "portrait", label: L("竖版 4:5", "Portrait 4:5"), width: 1080, height: 1350 },
    { id: "iphonePlus", label: "1242×2208", width: 1242, height: 2208 },
  ];
  const DEFAULTS: Options = { theme: "aurora", size: "story", format: "png", indent: "2em", watermark: "", output: "folder" };
  const STORAGE_KEY = "vellumi.cards.options";

  function loadOptions(): Options {
    // The self-test must not depend on (or overwrite) what a user chose.
    if (window.__vellumiSelftest) return { ...DEFAULTS };
    try {
      const stored = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "null") as Partial<Options> | null;
      return { ...DEFAULTS, ...(stored ?? {}) };
    } catch {
      return { ...DEFAULTS };
    }
  }

  const trace = (message: string) => (window as unknown as { __vellumiLog?: (l: string, m: string) => void }).__vellumiLog?.("info", `cards: ${message}`);
  const id = new URLSearchParams(location.search).get("id") ?? "";
  let options = $state<Options>(loadOptions());
  let frame: HTMLIFrameElement;
  let ready = $state(false);
  let rendering = $state(true);
  let exporting = $state(false);
  let pageCount = $state(1);
  let currentPage = $state(0);
  let status = $state(L("正在载入…", "Loading…"));
  let failure = $state<string | null>(null);
  let baseName = $state("");
  let pendingCapture: ((pages: string[]) => void) | null = null;

  interface CardApi {
    setContent(html: string): void;
    setOptions(options: unknown): void;
    showPage(index: number): void;
    captureAll(requestID: string): void;
  }
  const card = () => (frame?.contentWindow as unknown as { markmacCard?: CardApi } | null)?.markmacCard;

  function rendererOptions(preservePage: boolean) {
    const size = SIZES.find((s) => s.id === options.size) ?? SIZES[0];
    return {
      theme: options.theme,
      size: { width: size.width, height: size.height },
      watermark: options.watermark,
      format: options.format,
      paragraphIndent: options.indent,
      preservePage,
    };
  }

  type Message =
    | { type: "ready" }
    | { type: "rendered"; pageCount: number; currentPage: number }
    | { type: "pageShown"; pageCount: number; currentPage: number }
    | { type: "captureAll"; pages: { index: number; dataURL: string }[] }
    | { type: "error"; message: string };

  (window as unknown as { __vellumiCardBridge: (m: Message) => void }).__vellumiCardBridge = (message) => {
    trace(`message ${message.type}`);
    switch (message.type) {
      case "rendered":
        pageCount = Math.max(1, message.pageCount);
        currentPage = Math.min(message.currentPage, pageCount - 1);
        rendering = false;
        failure = null;
        status = L(`共 ${pageCount} 张`, `${pageCount} cards`);
        if (window.__vellumiSelftest && !exporting) void runExport();
        break;
      case "pageShown":
        currentPage = message.currentPage;
        break;
      case "captureAll":
        pendingCapture?.(message.pages.sort((a, b) => a.index - b.index).map((p) => p.dataURL));
        pendingCapture = null;
        break;
      case "error":
        failure = message.message;
        rendering = false;
        exporting = false;
        break;
    }
  };

  onMount(async () => {
    try {
      const source = await invoke<{ bodyHtml: string; baseName: string }>("cards_source", { id });
      baseName = source.baseName;
      trace(`source ${source.bodyHtml.length} chars`);
      frame.addEventListener("load", () => {
        ready = true;
        trace(`iframe loaded, api ${card() ? "present" : "MISSING"}`);
        card()?.setContent(source.bodyHtml);
        card()?.setOptions(rendererOptions(false));
      });
      frame.src = "imagecard/card.html";
    } catch (error) {
      failure = errorMessage(error);
    }
    window.addEventListener("beforeunload", () => void invoke("export_release", { id }));
  });

  // Options: re-render (debounced: the watermark field is typed into) and remember.
  let debounce: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const snapshot = JSON.stringify(options);
    try {
      if (!window.__vellumiSelftest) localStorage.setItem(STORAGE_KEY, snapshot);
    } catch {
      // Storage unavailable: options just aren't remembered.
    }
    if (!ready) return;
    clearTimeout(debounce);
    debounce = setTimeout(() => {
      rendering = true;
      status = L("正在排版…", "Laying out…");
      card()?.setOptions(rendererOptions(true));
    }, 200);
  });

  function show(index: number): void {
    currentPage = Math.max(0, Math.min(index, pageCount - 1));
    card()?.showPage(currentPage);
  }

  async function runExport(): Promise<void> {
    if (exporting || rendering) return;
    exporting = true;
    failure = null;
    status = L("正在生成图片…", "Rendering images…");
    try {
      const pages = await new Promise<string[]>((resolve) => {
        pendingCapture = resolve;
        card()?.captureAll(`r${Date.now()}`);
      });
      const output = pages.length === 1 ? "single" : options.output;
      const written = await invoke<number>("cards_write", {
        id,
        pages,
        extension: options.format === "jpeg" ? "jpg" : "png",
        output,
      });
      if (written > 0) {
        status = L(`已导出 ${written} 张`, `Exported ${written} cards`);
        await getCurrentWebviewWindow().close();
      } else {
        status = L(`共 ${pageCount} 张`, `${pageCount} cards`);
      }
    } catch (error) {
      failure = errorMessage(error);
    } finally {
      exporting = false;
    }
  }

</script>

<div class="window">
  <aside>
    <div>
      <h1>{L("导出为图片卡", "Export Image Cards")}</h1>
      <p class="muted">{L("长文自动分页，适合发到社交平台。", "Long documents are split into pages for sharing.")}</p>
    </div>

    <section>
      <p class="label">{L("主题", "Theme")}</p>
      <div class="themes" role="radiogroup" aria-label={L("主题", "Theme")}>
        {#each THEMES as theme (theme.id)}
          <button role="radio" aria-checked={options.theme === theme.id} class="swatch" onclick={() => (options.theme = theme.id)}>
            <span class="thumb" style:background={theme.swatch}></span>{theme.label}
          </button>
        {/each}
      </div>
    </section>

    <section>
      <p class="label">{L("尺寸", "Size")}</p>
      <div class="segmented" role="radiogroup" aria-label={L("尺寸", "Size")}>
        {#each SIZES as size (size.id)}
          <button role="radio" aria-checked={options.size === size.id} onclick={() => (options.size = size.id)}>{size.label}</button>
        {/each}
      </div>
    </section>

    <div class="pair">
      <section>
        <p class="label">{L("格式", "Format")}</p>
        <div class="segmented" role="radiogroup" aria-label={L("格式", "Format")}>
          <button role="radio" aria-checked={options.format === "png"} onclick={() => (options.format = "png")}>PNG</button>
          <button role="radio" aria-checked={options.format === "jpeg"} onclick={() => (options.format = "jpeg")}>JPEG</button>
        </div>
      </section>
      <section>
        <p class="label">{L("段首缩进", "Indent")}</p>
        <div class="segmented" role="radiogroup" aria-label={L("段首缩进", "Indent")}>
          <button role="radio" aria-checked={options.indent === "0em"} onclick={() => (options.indent = "0em")}>{L("无", "None")}</button>
          <button role="radio" aria-checked={options.indent === "2em"} onclick={() => (options.indent = "2em")}>{L("两字", "2 chars")}</button>
        </div>
      </section>
    </div>

    <section>
      <label class="label" for="watermark">{L("水印", "Watermark")}</label>
      <input id="watermark" class="text" bind:value={options.watermark} placeholder={L("可选，例如 @你的名字", "Optional, e.g. @yourname")} />
    </section>

    <section>
      <p class="label">{L("输出", "Output")}</p>
      <label class="radio"><input type="radio" bind:group={options.output} value="folder" disabled={pageCount <= 1} />{L("每页一张，存入文件夹", "One file per card, into a folder")}</label>
      <label class="radio"><input type="radio" bind:group={options.output} value="zip" disabled={pageCount <= 1} />{L("打包为 ZIP", "As a ZIP archive")}</label>
    </section>

    <div class="spacer"></div>
    {#if failure}
      <p class="error" role="alert">{failure}</p>
    {:else}
      <p class="muted" aria-live="polite">{status}</p>
    {/if}
    <div class="actions">
      <button class="button" onclick={() => getCurrentWebviewWindow().close()}>{L("取消", "Cancel")}</button>
      <button class="button primary" disabled={!ready || rendering || exporting} onclick={runExport}>
        {pageCount > 1 ? L(`导出 ${pageCount} 张`, `Export ${pageCount}`) : L("导出", "Export")}
      </button>
    </div>
  </aside>

  <main>
    <iframe bind:this={frame} title={L("卡片预览", "Card preview")}></iframe>
    <div class="pager">
      <button class="icon-button" aria-label={L("上一页", "Previous")} disabled={currentPage === 0 || rendering} onclick={() => show(currentPage - 1)}>‹</button>
      <span class="muted">{L(`第 ${currentPage + 1} 页，共 ${pageCount} 页`, `Card ${currentPage + 1} of ${pageCount}`)}</span>
      <button class="icon-button" aria-label={L("下一页", "Next")} disabled={currentPage >= pageCount - 1 || rendering} onclick={() => show(currentPage + 1)}>›</button>
    </div>
    {#if baseName}<span class="file muted">{baseName}</span>{/if}
  </main>
</div>

<style>
  :global(body) {
    background: var(--chrome);
  }
  .window {
    display: flex;
    height: 100vh;
    font-size: 13px;
  }
  aside {
    width: 320px;
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding: 22px;
    border-right: 1px solid var(--line);
    overflow: auto;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 18px;
  }
  .muted {
    color: var(--muted);
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
  }
  .label {
    display: block;
    margin: 0 0 8px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ink2);
  }
  .themes {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 4px;
  }
  .swatch {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 6px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--ink2);
    font-size: 11px;
    cursor: pointer;
  }
  .swatch:hover {
    background: var(--hover);
  }
  .swatch[aria-checked="true"] {
    box-shadow: 0 0 0 2px var(--accent);
  }
  .thumb {
    width: 44px;
    height: 72px;
    border-radius: 6px;
    border: 1px solid var(--line);
  }
  .segmented {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 9px;
    background: var(--hover);
  }
  .segmented button {
    flex: 1;
    height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--ink2);
    font-size: 12px;
    cursor: pointer;
  }
  .segmented button[aria-checked="true"] {
    background: var(--surface);
    color: var(--ink);
    font-weight: 500;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  .pair {
    display: flex;
    gap: 12px;
  }
  .pair section {
    flex: 1;
  }
  .text {
    width: 100%;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--ink);
    font-size: 13px;
  }
  .radio {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 26px;
    color: var(--ink);
  }
  .radio input {
    accent-color: var(--accent);
  }
  .spacer {
    flex: 1;
  }
  .error {
    margin: 0;
    color: #d9534f;
    font-size: 12px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  .actions .button {
    height: 34px;
  }
  .button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 16px;
  }
  iframe {
    flex: 1;
    width: 100%;
    border: 0;
    border-radius: 10px;
    background: transparent;
  }
  .pager {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .pager .icon-button {
    font-size: 18px;
  }
</style>
