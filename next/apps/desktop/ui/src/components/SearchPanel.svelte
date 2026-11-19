<script lang="ts">
  import { editor } from "../lib/editor";
  import { t } from "../lib/i18n";
  import { api, errorMessage, type FileMatches } from "../lib/ipc";
  import { openFind, ui } from "../lib/store.svelte";

  let query = $state("");
  let caseSensitive = $state(false);
  let results = $state<FileMatches[] | null>(null);
  let searching = $state(false);
  let failure = $state<string | null>(null);
  let input: HTMLInputElement;
  let generation = 0;

  const total = $derived(results?.reduce((sum, file) => sum + file.matches.length, 0) ?? 0);

  $effect(() => {
    // ⇧⌘F focuses the field (a new token each time).
    if (ui.searchFocusToken) input?.focus();
  });

  async function runSearch(): Promise<void> {
    const text = query.trim();
    if (!text) {
      results = null;
      return;
    }
    const mine = ++generation;
    searching = true;
    failure = null;
    try {
      const found = await api.search(text, caseSensitive);
      if (mine === generation) results = found;
    } catch (error) {
      if (mine === generation) failure = errorMessage(error);
    } finally {
      if (mine === generation) searching = false;
    }
  }

  function openHit(path: string): void {
    if (path === ui.doc?.path) {
      // Same document: drive the in-document find to the query.
      openFind();
      ui.findResult = editor()?.search({ text: query.trim(), caseSensitive, wholeWord: false, regex: false }) ?? ui.findResult;
      ui.findResult = editor()?.searchStep(false) ?? ui.findResult;
    } else {
      void api.openPath(path);
    }
  }
</script>

<div class="panel-header"><div class="panel-title">{t.search}</div></div>
<div class="search-box">
  <label class="visually-hidden" for="folder-search">{t.searchPlaceholder}</label>
  <input
    id="folder-search"
    bind:this={input}
    bind:value={query}
    placeholder={t.searchPlaceholder}
    onkeydown={(event) => event.key === "Enter" && runSearch()}
  />
  <label class="case">
    <input type="checkbox" bind:checked={caseSensitive} onchange={runSearch} />
    {t.caseSensitive}
  </label>
  <p class="hint">
    {#if searching}…{:else if results}{t.matchesIn(results.length, total)}{:else}{t.searchHint}{/if}
  </p>
</div>
<div class="panel-body">
  {#if failure}
    <p class="panel-empty">{failure}</p>
  {:else if results && results.length === 0}
    <p class="panel-empty">{t.searchNone}</p>
  {:else if results}
    {#each results as file (file.path)}
      <button class="hit" class:current={file.path === ui.doc?.path} onclick={() => openHit(file.path)} title={file.path}>
        <span class="hit-name">{file.name} <span class="count">· {file.matches.length}</span></span>
        {#each file.matches.slice(0, 3) as match (match.line)}
          <span class="hit-line"><span class="line-no">L{match.line}</span> {match.text}</span>
        {/each}
      </button>
    {/each}
  {/if}
</div>

<style>
  .search-box {
    padding: 4px 12px 8px;
  }
  .search-box input:not([type]) {
    width: 100%;
    height: 30px;
    border: 1px solid var(--line);
    border-radius: 7px;
    padding: 0 10px;
    background: var(--surface);
    color: var(--ink);
    font-size: 13px;
  }
  .case {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 6px;
    font-size: 12px;
    color: var(--ink2);
  }
  .hint {
    margin: 6px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .hit {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    text-align: left;
    cursor: pointer;
    color: var(--ink);
  }
  .hit:hover {
    background: var(--hover);
  }
  .hit.current {
    background: var(--accent-soft);
  }
  .hit-name {
    font-size: 13px;
    font-weight: 500;
  }
  .count,
  .line-no {
    color: var(--muted);
    font-weight: 400;
  }
  .line-no {
    font-family: var(--mono);
    font-size: 11px;
  }
  .hit-line {
    font-size: 12px;
    color: var(--ink2);
    line-height: 1.5;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
