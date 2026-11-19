<script lang="ts">
  import { t } from "../lib/i18n";
  import { patchSettings, ui } from "../lib/store.svelte";

  const doc = $derived(ui.doc);
  const settings = $derived(ui.settings);
  const mode = $derived(settings?.statsMode ?? 0);
  const goal = $derived(settings?.wordGoal ?? 0);
  const percent = $derived(goal > 0 && doc ? Math.round((doc.stats.words / goal) * 100) : 0);
  const ring = $derived(Math.min(100, percent));

  const saveState = $derived.by(() => {
    if (!doc) return { label: "", tone: "idle" };
    if (!doc.dirty) return { label: t.saved, tone: "saved" };
    return { label: doc.path ? t.autosaveSoon : t.unsavedUntitled, tone: "dirty" };
  });

  const statText = $derived.by(() => {
    if (!doc) return "";
    const stats = doc.stats;
    if (mode === 1) return t.characters(stats.characters);
    if (mode === 2) return t.paragraphs(stats.paragraphs);
    return stats.words > 0 ? `${t.words(stats.words)} · ${t.reading(stats.readingMinutes)}` : t.words(0);
  });
</script>

<footer class="status-bar">
  <span class="item" title={doc?.path ?? ""}>
    <span class="dot {saveState.tone}" aria-hidden="true"></span>{saveState.label}
  </span>
  <span class="spacer"></span>
  {#if ui.showNormalizationNotice}
    <button class="item link" onclick={() => (ui.showNormalizationNotice = false)}>{t.normalizedNotice}</button>
    <span class="divider" aria-hidden="true"></span>
  {/if}
  <button class="item link" title={t.cycleStats} onclick={() => patchSettings({ statsMode: (mode + 1) % 3 })}>{statText}</button>
  {#if goal > 0}
    <span class="item" title={String(goal)}>
      <svg width="13" height="13" viewBox="0 0 36 36" aria-hidden="true">
        <circle cx="18" cy="18" r="15" fill="none" stroke="var(--line)" stroke-width="5" />
        <circle cx="18" cy="18" r="15" fill="none" stroke="var(--accent)" stroke-width="5" stroke-dasharray="{(ring / 100) * 94.2} 100" transform="rotate(-90 18 18)" />
      </svg>
      {t.goal(percent)}
    </span>
  {/if}
  <span class="divider" aria-hidden="true"></span>
  <span class="item">{doc?.encoding ?? ""}</span>
  {#if settings && Math.abs(settings.zoom - 1) > 0.001}
    <span class="item">{Math.round(settings.zoom * 100)}%</span>
  {/if}
</footer>

<style>
  .status-bar {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    height: var(--status-h);
    display: flex;
    align-items: center;
    gap: 2px;
    padding: 0 10px;
    background: var(--chrome);
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--muted);
    z-index: 20;
  }
  .spacer {
    flex: 1;
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 22px;
    padding: 0 8px;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted);
    font-size: 12px;
    white-space: nowrap;
  }
  .link {
    cursor: pointer;
  }
  .link:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .divider {
    width: 1px;
    height: 13px;
    background: var(--line);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--line);
  }
  .dot.saved {
    background: #3fb97f;
  }
  .dot.dirty {
    background: #e0a100;
  }
</style>
