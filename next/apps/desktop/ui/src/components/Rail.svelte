<script lang="ts">
  import { run } from "../lib/actions";
  import { t } from "../lib/i18n";
  import type { SidebarPanel } from "../lib/ipc";
  import { openPalette, togglePanel, ui } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";

  const active = (panel: SidebarPanel) => ui.settings?.sidebarVisible === true && ui.settings.sidebarPanel === panel;
</script>

<nav class="rail" aria-label={t.palette}>
  <button class="rail-button" aria-pressed={active("files")} aria-label="{t.files} ⌥⌘2" title="{t.files} ⌥⌘2" onclick={() => togglePanel("files")}>
    <Icon name="folder" size={20} />
  </button>
  <button class="rail-button" aria-pressed={active("outline")} aria-label="{t.outline} ⌥⌘1" title="{t.outline} ⌥⌘1" onclick={() => togglePanel("outline")}>
    <Icon name="outline" size={20} />
  </button>
  <button class="rail-button" aria-pressed={active("search")} aria-label="{t.search} ⇧⌘F" title="{t.search} ⇧⌘F" onclick={() => togglePanel("search")}>
    <Icon name="search" size={20} />
  </button>
  <button class="rail-button" aria-label="{t.projects} ⇧⌘0" title="{t.projects} ⇧⌘0" onclick={() => run("view.projects")}>
    <Icon name="grid" size={20} />
  </button>
  <button class="rail-button" aria-label="{t.palette} ⇧⌘P" title="{t.palette} ⇧⌘P" onclick={() => openPalette("files")}>
    <Icon name="command" size={20} />
  </button>
  <div class="spacer"></div>
  <button class="rail-button" aria-label="{t.settings} ⌘," title="{t.settings} ⌘," onclick={() => run("app.settings")}>
    <Icon name="settings" size={20} />
  </button>
</nav>

<style>
  .rail {
    position: fixed;
    top: 0;
    left: 0;
    bottom: var(--status-h);
    width: var(--rail-w);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 10px 0;
    background: var(--chrome);
    border-right: 1px solid var(--line);
    z-index: 10;
  }
  .spacer {
    flex: 1;
  }
  .rail-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border: 0;
    border-radius: 10px;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    padding: 0;
  }
  .rail-button:hover {
    background: var(--hover);
    color: var(--ink);
  }
  .rail-button[aria-pressed="true"] {
    background: var(--accent-soft);
    color: var(--accent);
  }
</style>
