<script lang="ts">
  import { ui } from "../lib/store.svelte";
  import FileTree from "./FileTree.svelte";
  import Outline from "./Outline.svelte";
  import SearchPanel from "./SearchPanel.svelte";

  const panel = $derived(ui.settings?.sidebarPanel ?? "files");
</script>

<aside class="sidebar" aria-label={panel}>
  {#if panel === "files"}
    <FileTree />
  {:else if panel === "outline"}
    <Outline />
  {:else}
    <SearchPanel />
  {/if}
</aside>

<style>
  .sidebar {
    position: fixed;
    top: 0;
    left: var(--rail-w);
    bottom: var(--status-h);
    width: var(--sidebar-w);
    display: flex;
    flex-direction: column;
    background: var(--chrome);
    border-right: 1px solid var(--line);
    overflow: hidden;
    z-index: 9;
    font-size: 13px;
  }
  .sidebar :global(.panel-header) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 12px 10px 6px 14px;
  }
  .sidebar :global(.panel-title) {
    font-size: 13px;
    font-weight: 600;
    color: var(--ink);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sidebar :global(.panel-subtitle) {
    font-size: 11px;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sidebar :global(.panel-body) {
    flex: 1;
    min-height: 0;
    overflow: auto;
    padding: 4px 8px 12px;
  }
  .sidebar :global(.panel-empty) {
    padding: 8px 6px;
    color: var(--muted);
    font-size: 12px;
    line-height: 1.6;
  }
  .sidebar :global(.row) {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-height: 28px;
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--ink2);
    font-size: 13px;
    text-align: left;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sidebar :global(.row:hover) {
    background: var(--hover);
  }
  .sidebar :global(.row.current) {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 500;
  }
</style>
