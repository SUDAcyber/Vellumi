<script lang="ts">
  import { editor } from "../lib/editor";
  import { t } from "../lib/i18n";
  import { ui } from "../lib/store.svelte";

  let selected = $state<number | null>(null);
  const minLevel = $derived(ui.outline.length ? Math.min(...ui.outline.map((h) => h.level)) : 1);

  $effect(() => {
    if (ui.editorReady && ui.outline.length === 0) ui.outline = editor()?.getOutline() ?? [];
  });

  function go(pos: number): void {
    selected = pos;
    editor()?.scrollToHeading(pos);
  }

  function onKey(event: KeyboardEvent): void {
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    const index = ui.outline.findIndex((h) => h.pos === selected);
    const next = event.key === "ArrowDown" ? Math.min(index + 1, ui.outline.length - 1) : Math.max(index - 1, 0);
    const heading = ui.outline[next];
    if (heading) {
      go(heading.pos);
      (document.querySelector(`[data-heading="${heading.pos}"]`) as HTMLElement | null)?.focus();
    }
  }
</script>

<div class="panel-header"><div class="panel-title">{t.outline}</div></div>
<div class="panel-body" role="tree" tabindex="-1" onkeydown={onKey}>
  {#if ui.outline.length === 0}
    <p class="panel-empty">{t.noHeadings}</p>
  {:else}
    {#each ui.outline as heading (heading.pos)}
      <button
        class="row"
        class:current={heading.pos === selected}
        data-heading={heading.pos}
        role="treeitem"
        aria-selected={heading.pos === selected}
        style:padding-left="{8 + (heading.level - minLevel) * 14}px"
        style:font-weight={heading.level === minLevel ? 600 : 400}
        onclick={() => go(heading.pos)}
      >
        {heading.text || "—"}
      </button>
    {/each}
  {/if}
</div>
