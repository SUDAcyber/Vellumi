<script lang="ts">
  // The protected YAML block, visible as chips above the title (decision
  // D13); clicking opens the editor. Only simple `key: value` lines become
  // chips — the block itself is never parsed or rewritten here.
  import { t } from "../lib/i18n";
  import { ui } from "../lib/store.svelte";

  const chips = $derived.by(() => {
    const lines = (ui.doc?.frontMatter ?? "").split(/\r?\n/).slice(1);
    const out: string[] = [];
    for (const line of lines) {
      if (/^(---|\.\.\.)\s*$/.test(line)) break;
      const match = /^([A-Za-z0-9_-]+):\s*(.*)$/.exec(line);
      if (match) out.push(match[2] ? `${match[1]}: ${match[2]}` : match[1]);
      if (out.length >= 6) break;
    }
    return out;
  });
</script>

<div class="front-matter-bar">
  <button class="chip action" onclick={() => (ui.frontMatterOpen = true)} aria-label={t.editFrontMatter}>
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><path d="M4 7h16M4 12h10M4 17h7" /></svg>
    {t.frontMatter}
  </button>
  {#each chips as chip, index (index)}
    <button class="chip" onclick={() => (ui.frontMatterOpen = true)}>{chip}</button>
  {/each}
</div>

<style>
  .front-matter-bar {
    max-width: var(--vellumi-max-width, clamp(760px, 75%, 1200px));
    margin: 0 auto;
    padding: 28px 56px 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    max-width: 280px;
    padding: 0 9px;
    border: 0;
    border-radius: 999px;
    background: var(--chip);
    color: var(--ink2);
    font-size: 12px;
    cursor: pointer;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .chip.action {
    color: var(--accent);
    background: var(--accent-soft);
  }
</style>
