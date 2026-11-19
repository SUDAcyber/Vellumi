<script lang="ts">
  import { onMount } from "svelte";

  import { editor } from "../lib/editor";
  import { t } from "../lib/i18n";
  import { api } from "../lib/ipc";
  import { setDoc, ui } from "../lib/store.svelte";

  const TEMPLATE = "---\ntitle: \n---\n";
  let text = $state(ui.doc?.frontMatter || TEMPLATE);
  let invalid = $state(false);
  let area: HTMLTextAreaElement;

  onMount(() => area?.focus());

  async function save(): Promise<void> {
    try {
      setDoc(await api.setFrontMatter(text));
      close();
    } catch {
      invalid = true;
    }
  }

  function close(): void {
    ui.frontMatterOpen = false;
    editor()?.focus();
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key === "Escape") close();
    if (event.key === "Enter" && event.metaKey) void save();
  }
</script>

<div class="scrim" role="presentation" onclick={close}></div>
<div class="dialog" role="dialog" aria-modal="true" aria-labelledby="fm-title" tabindex="-1" onkeydown={onKey}>
  <h2 id="fm-title">{t.editFrontMatter}</h2>
  <label class="visually-hidden" for="fm-text">{t.editFrontMatter}</label>
  <textarea id="fm-text" bind:this={area} bind:value={text} spellcheck="false" oninput={() => (invalid = false)}></textarea>
  {#if invalid}<p class="error" role="alert">{t.frontMatterInvalid}</p>{/if}
  <div class="footer">
    <span class="hint">{t.frontMatterHint}</span>
    <button class="button" onclick={close}>{t.cancel}</button>
    <button class="button primary" onclick={save}>{t.save} ⌘↩</button>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgba(20, 22, 26, 0.25);
    z-index: 50;
  }
  .dialog {
    position: fixed;
    top: 72px;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, calc(100vw - 40px));
    padding: 16px;
    border-radius: 14px;
    background: var(--chrome);
    box-shadow: var(--shadow);
    z-index: 51;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 15px;
  }
  textarea {
    width: 100%;
    min-height: 200px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--surface);
    color: var(--ink);
    font-family: var(--mono);
    font-size: 13px;
    line-height: 1.6;
    resize: vertical;
  }
  .error {
    margin: 8px 0 0;
    color: #d9534f;
    font-size: 12px;
  }
  .footer {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 12px;
  }
  .hint {
    flex: 1;
    font-size: 11px;
    color: var(--muted);
  }
</style>
