<script lang="ts">
  import { editor } from "../lib/editor";
  import { t } from "../lib/i18n";
  import { ui } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  let text = $state("");
  let replacement = $state("");
  let caseSensitive = $state(false);
  let wholeWord = $state(false);
  let regex = $state(false);
  let field: HTMLInputElement;

  const query = $derived({ text, caseSensitive, wholeWord, regex });

  $effect(() => {
    // ⌘F while open re-focuses and selects (a new token each time).
    if (ui.findFocusToken) {
      field?.focus();
      field?.select();
    }
  });

  $effect(() => {
    ui.findResult = editor()?.search(query) ?? { total: 0, current: 0 };
  });

  function step(backwards: boolean): void {
    ui.findResult = editor()?.searchStep(backwards) ?? ui.findResult;
  }

  function replaceOne(): void {
    ui.findResult = editor()?.replaceCurrent(replacement) ?? ui.findResult;
  }

  function replaceEvery(): void {
    editor()?.replaceAll(replacement);
    ui.findResult = editor()?.searchState() ?? ui.findResult;
  }

  function close(): void {
    ui.findOpen = false;
    editor()?.clearSearch();
    editor()?.focus();
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") {
      event.preventDefault();
      close();
    } else if (event.key === "Enter" && event.target === field) {
      event.preventDefault();
      step(event.shiftKey);
    }
  }

  const counter = $derived(text ? (ui.findResult.total ? `${ui.findResult.current} / ${ui.findResult.total}` : t.noMatches) : "");
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section class="find-bar" role="search" aria-label={t.find} onkeydown={onKey}>
  <div class="line">
    <div class="field" class:empty={text !== "" && ui.findResult.total === 0}>
      <Icon name="search" size={14} />
      <label class="visually-hidden" for="find-text">{t.find}</label>
      <input id="find-text" bind:this={field} bind:value={text} placeholder={t.find} autocomplete="off" spellcheck="false" />
      <span class="counter" aria-live="polite">{counter}</span>
      <button class="toggle" aria-pressed={caseSensitive} title={t.caseSensitive} aria-label={t.caseSensitive} onclick={() => (caseSensitive = !caseSensitive)}>Aa</button>
      <button class="toggle" aria-pressed={wholeWord} title={t.wholeWord} aria-label={t.wholeWord} onclick={() => (wholeWord = !wholeWord)}>ab</button>
      <button class="toggle" aria-pressed={regex} title={t.regex} aria-label={t.regex} onclick={() => (regex = !regex)}>.*</button>
    </div>
    <button class="icon-button" title="{t.previous} ⇧⌘G" aria-label={t.previous} onclick={() => step(true)}><Icon name="chevron-up" size={15} /></button>
    <button class="icon-button" title="{t.next} ⌘G" aria-label={t.next} onclick={() => step(false)}><Icon name="chevron-down" size={15} /></button>
    <button class="icon-button" title="{t.close} Esc" aria-label={t.close} onclick={close}><Icon name="close" size={14} /></button>
  </div>
  <div class="line">
    <div class="field">
      <label class="visually-hidden" for="find-replace">{t.replaceWith}</label>
      <input id="find-replace" bind:value={replacement} placeholder={t.replaceWith} autocomplete="off" spellcheck="false" />
    </div>
    <button class="button" disabled={!ui.findResult.total} onclick={replaceOne}>{t.replace}</button>
    <button class="button" disabled={!ui.findResult.total} onclick={replaceEvery}>{t.replaceAll}</button>
  </div>
  <p class="hint">{t.findHint}</p>
</section>

<style>
  .find-bar {
    position: fixed;
    top: 12px;
    right: 20px;
    width: min(480px, calc(100vw - 120px));
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    border-radius: 12px;
    background: var(--chrome);
    box-shadow: var(--shadow);
    z-index: 30;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .field {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 6px 0 10px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--muted);
  }
  .field.empty {
    border-color: #d9534f;
  }
  .field input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--ink);
    font-size: 13px;
  }
  .counter {
    font-size: 12px;
    color: var(--ink2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .toggle {
    height: 24px;
    min-width: 26px;
    padding: 0 5px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 12px;
    cursor: pointer;
  }
  .toggle[aria-pressed="true"] {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .hint {
    margin: 0;
    padding: 0 2px;
    font-size: 11px;
    color: var(--muted);
  }
</style>
