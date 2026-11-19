<script lang="ts">
  import { onMount } from "svelte";

  import { COMMANDS, run } from "../lib/actions";
  import { editor } from "../lib/editor";
  import { t } from "../lib/i18n";
  import { api, type FileHit, type FileMatches } from "../lib/ipc";
  import { openFind, ui, type PaletteMode } from "../lib/store.svelte";

  interface Row {
    key: string;
    title: string;
    detail: string;
    trailing?: string;
    lines?: { line: number; text: string }[];
    activate: () => void;
  }

  let query = $state("");
  let selected = $state(0);
  let files = $state<FileHit[]>([]);
  let textHits = $state<FileMatches[]>([]);
  let input: HTMLInputElement;
  let generation = 0;

  const modes: { id: PaletteMode; label: string; hint: string }[] = [
    { id: "files", label: t.paletteFiles, hint: "⇧⌘P" },
    { id: "commands", label: t.paletteCommands, hint: ">" },
    { id: "text", label: t.paletteText, hint: "⇧⌘F" },
  ];

  const placeholder = $derived(
    ui.paletteMode === "files" ? t.palettePlaceholderFiles : ui.paletteMode === "commands" ? t.palettePlaceholderCommands : t.palettePlaceholderText,
  );

  /** Subsequence match, same spirit as the Rust fuzzy scorer, for commands. */
  function matches(needle: string, haystack: string): boolean {
    const lower = haystack.toLowerCase();
    let at = 0;
    for (const ch of needle.toLowerCase()) {
      at = lower.indexOf(ch, at);
      if (at < 0) return false;
      at += 1;
    }
    return true;
  }

  const rows = $derived.by<Row[]>(() => {
    if (ui.paletteMode === "commands") {
      const needle = query.replace(/^>\s*/, "");
      return COMMANDS.filter((c) => !needle || matches(needle, `${c.group} ${c.label}`)).map((c) => ({
        key: c.id,
        title: c.label,
        detail: c.group,
        trailing: c.shortcut,
        activate: () => void run(c.id),
      }));
    }
    if (ui.paletteMode === "text") {
      return textHits.map((file) => ({
        key: file.path,
        title: file.name,
        detail: `${file.matches.length}`,
        lines: file.matches.slice(0, 2),
        activate: () => openTextHit(file.path),
      }));
    }
    return files.map((file) => ({
      key: file.path,
      title: file.name,
      detail: file.recent ? `${t.recent} · ${file.folder}` : file.folder,
      activate: () => void api.openPath(file.path),
    }));
  });

  async function refresh(): Promise<void> {
    const mine = ++generation;
    if (ui.paletteMode === "files") {
      const found = await api.quickOpen(query);
      if (mine === generation) files = found;
    } else if (ui.paletteMode === "text") {
      const text = query.trim();
      const found = text ? await api.search(text, false) : [];
      if (mine === generation) textHits = found;
    }
    selected = 0;
  }

  // Typing ">" in file mode switches to commands, like editor palettes do.
  $effect(() => {
    if (ui.paletteMode === "files" && query.startsWith(">")) ui.paletteMode = "commands";
  });

  let debounce: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void query;
    void ui.paletteMode;
    clearTimeout(debounce);
    debounce = setTimeout(() => void refresh(), ui.paletteMode === "text" ? 250 : 60);
  });

  onMount(() => input?.focus());

  function close(): void {
    ui.paletteOpen = false;
  }

  function activate(index: number): void {
    const row = rows[index];
    if (!row) return;
    close();
    row.activate();
  }

  function openTextHit(path: string): void {
    if (path === ui.doc?.path) {
      openFind();
      ui.findResult = editor()?.search({ text: query.trim(), caseSensitive: false, wholeWord: false, regex: false }) ?? ui.findResult;
    } else {
      void api.openPath(path);
    }
  }

  function cycleMode(): void {
    const index = modes.findIndex((m) => m.id === ui.paletteMode);
    ui.paletteMode = modes[(index + 1) % modes.length].id;
    query = "";
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      selected = Math.min(selected + 1, rows.length - 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      selected = Math.max(selected - 1, 0);
    } else if (event.key === "Enter") {
      event.preventDefault();
      activate(selected);
    } else if (event.key === "Tab") {
      event.preventDefault();
      cycleMode();
    }
  }
</script>

<div class="scrim" role="presentation" onclick={close}></div>
<div class="palette" role="dialog" aria-modal="true" aria-label={t.palette} tabindex="-1" onkeydown={onKey}>
  <div class="input-row">
    <label class="visually-hidden" for="palette-input">{placeholder}</label>
    <input id="palette-input" bind:this={input} bind:value={query} {placeholder} autocomplete="off" spellcheck="false" />
    <span class="kbd">Esc</span>
  </div>
  <div class="modes" role="tablist">
    {#each modes as mode (mode.id)}
      <button
        role="tab"
        class="mode"
        aria-selected={ui.paletteMode === mode.id}
        onclick={() => {
          ui.paletteMode = mode.id;
          query = "";
          input?.focus();
        }}>{mode.label} <span class="hint">{mode.hint}</span></button
      >
    {/each}
  </div>
  <div class="results" role="listbox" aria-label={placeholder}>
    {#if rows.length === 0}
      <p class="empty">{t.paletteEmpty}</p>
    {:else}
      {#each rows as row, index (row.key)}
        <button
          role="option"
          class="result"
          aria-selected={index === selected}
          onmouseenter={() => (selected = index)}
          onclick={() => activate(index)}
        >
          <span class="title">{row.title}</span>
          <span class="detail">{row.detail}</span>
          {#if row.trailing}<span class="kbd">{row.trailing}</span>{/if}
          {#if row.lines}
            {#each row.lines as line (line.line)}
              <span class="line"><span class="line-no">L{line.line}</span> {line.text}</span>
            {/each}
          {/if}
        </button>
      {/each}
    {/if}
  </div>
  <div class="footer">
    <span><span class="kbd">↑↓</span> {t.select}</span>
    <span><span class="kbd">↵</span> {t.open}</span>
    <span><span class="kbd">Tab</span> {t.switchMode}</span>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(20, 22, 26, 0.18);
    z-index: 50;
  }
  .palette {
    position: fixed;
    top: 72px;
    left: 50%;
    transform: translateX(-50%);
    width: min(640px, calc(100vw - 32px));
    border-radius: 14px;
    background: var(--chrome);
    box-shadow: var(--shadow);
    overflow: hidden;
    z-index: 51;
  }
  .input-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--line);
  }
  .input-row input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font-size: 16px;
  }
  .modes {
    display: flex;
    gap: 2px;
    margin: 10px 12px 4px;
    padding: 3px;
    width: fit-content;
    border-radius: 9px;
    background: var(--hover);
  }
  .mode {
    height: 26px;
    padding: 0 12px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: var(--ink2);
    font-size: 12px;
    cursor: pointer;
  }
  .mode[aria-selected="true"] {
    background: var(--surface);
    color: var(--ink);
    font-weight: 500;
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.1);
  }
  .hint {
    color: var(--muted);
    font-weight: 400;
  }
  .results {
    max-height: 360px;
    overflow: auto;
    padding: 4px 8px 8px;
  }
  .empty {
    padding: 16px 12px;
    margin: 0;
    color: var(--muted);
    font-size: 13px;
  }
  .result {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    column-gap: 12px;
    width: 100%;
    min-height: 40px;
    padding: 6px 12px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--ink);
    text-align: left;
    cursor: pointer;
  }
  .result[aria-selected="true"] {
    background: var(--accent-soft);
  }
  .title {
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .detail {
    grid-column: 1;
    font-size: 12px;
    color: var(--muted);
  }
  .result .kbd {
    grid-column: 2;
    grid-row: 1 / span 2;
  }
  .line {
    grid-column: 1 / span 2;
    font-size: 12px;
    color: var(--ink2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .line-no {
    font-family: var(--mono);
    color: var(--muted);
  }
  .footer {
    display: flex;
    gap: 14px;
    padding: 9px 16px;
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
  }
</style>
