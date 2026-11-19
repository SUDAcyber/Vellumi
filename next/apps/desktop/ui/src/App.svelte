<script lang="ts">
  import { onMount } from "svelte";

  import Banner from "./components/Banner.svelte";
  import FindBar from "./components/FindBar.svelte";
  import FrontMatterBar from "./components/FrontMatterBar.svelte";
  import FrontMatterDialog from "./components/FrontMatterDialog.svelte";
  import Palette from "./components/Palette.svelte";
  import Rail from "./components/Rail.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import StatusBar from "./components/StatusBar.svelte";
  import { t } from "./lib/i18n";
  import { setSourceHost } from "./lib/source";
  import { ui } from "./lib/store.svelte";

  let sourceHost: HTMLDivElement;
  const sidebarOpen = $derived(ui.settings?.sidebarVisible ?? true);

  onMount(() => setSourceHost(sourceHost));
</script>

<Rail />
{#if sidebarOpen}
  <Sidebar />
{/if}

<main class="content" style:--chrome-left={sidebarOpen ? "calc(var(--rail-w) + var(--sidebar-w))" : "var(--rail-w)"}>
  <Banner />
  <div class="editor-area" class:parked={ui.sourceMode}>
    {#if ui.doc?.frontMatter && !ui.sourceMode}
      <FrontMatterBar />
    {/if}
    <!-- The editor mounts here once; Svelte never re-renders this element. -->
    <div id="editor"></div>
  </div>
</main>

<div class="source" class:active={ui.sourceMode} style:left={sidebarOpen ? "calc(var(--rail-w) + var(--sidebar-w))" : "var(--rail-w)"}>
  <div class="source-badge">{t.sourceMode}</div>
  <div class="source-host" bind:this={sourceHost}></div>
</div>

{#if ui.findOpen && !ui.sourceMode}
  <FindBar />
{/if}
<StatusBar />
{#if ui.paletteOpen}
  <Palette />
{/if}
{#if ui.frontMatterOpen}
  <FrontMatterDialog />
{/if}
{#if ui.toast}
  <div class="toast" role="status">{ui.toast}</div>
{/if}

<style>
  .editor-area.parked {
    display: none;
  }
  .source {
    position: fixed;
    top: 0;
    right: 0;
    bottom: var(--status-h);
    display: none;
    flex-direction: column;
    background: var(--surface);
    z-index: 5;
  }
  .source.active {
    display: flex;
  }
  .source-badge {
    align-self: flex-end;
    margin: 8px 12px 0;
    padding: 3px 10px;
    border-radius: 999px;
    background: var(--hover);
    color: var(--ink2);
    font-size: 12px;
  }
  .source-host {
    flex: 1;
    min-height: 0;
  }
  .source-host :global(.cm-editor) {
    height: 100%;
    font-family: var(--mono);
    font-size: 14px;
    background: var(--surface);
    color: var(--ink);
  }
  .source-host :global(.cm-scroller) {
    line-height: 1.75;
  }
  .source-host :global(.cm-content) {
    max-width: 900px;
    margin: 0 auto;
    padding: 16px 0 40vh;
  }
  .source-host :global(.cm-gutters) {
    background: transparent;
    border: 0;
    color: var(--muted);
  }
  .source-host :global(.cm-activeLine) {
    background: var(--hover);
  }
  .source-host :global(.cm-activeLineGutter) {
    background: transparent;
    color: var(--ink);
  }
  .source-host :global(.cm-panels) {
    background: var(--chrome);
    color: var(--ink);
    border-color: var(--line);
  }
  .toast {
    position: fixed;
    left: 50%;
    bottom: calc(var(--status-h) + 16px);
    transform: translateX(-50%);
    padding: 8px 14px;
    border-radius: 9px;
    background: var(--ink);
    color: var(--surface);
    font-size: 12px;
    z-index: 40;
    box-shadow: var(--shadow);
  }
</style>
