<script lang="ts">
  import { run } from "../lib/actions";
  import { t } from "../lib/i18n";
  import { api, type TreeNode } from "../lib/ipc";
  import { ui } from "../lib/store.svelte";
  import Icon from "./Icon.svelte";
  import TreeRows from "./TreeRows.svelte";

  let nodes = $state<TreeNode[]>([]);
  let loadedFor = $state<string | null>(null);
  const folder = $derived(ui.doc?.folder ?? null);
  const folderName = $derived(folder ? folder.split("/").filter(Boolean).at(-1) ?? folder : "");

  async function refresh(): Promise<void> {
    if (!folder) {
      nodes = [];
      return;
    }
    nodes = await api.tree(folder);
    loadedFor = folder;
  }

  // Reload when the document moves to another folder (Save As, first save).
  $effect(() => {
    if (folder !== loadedFor) void refresh();
  });
</script>

<div class="panel-header">
  <div style="min-width: 0">
    <div class="panel-title">{folderName || t.files}</div>
    {#if folder}<div class="panel-subtitle" title={folder}>{folder.replace(/^\/Users\/[^/]+/, "~")}</div>{/if}
  </div>
  <div style="display: flex; gap: 2px">
    <button class="icon-button" aria-label={t.newNote} title="{t.newNote} ⌘N" onclick={() => run("file.new")}><Icon name="plus" size={16} /></button>
    {#if folder}
      <button class="icon-button" aria-label={t.reveal} title={t.reveal} onclick={() => folder && api.reveal(folder)}><Icon name="reveal" size={15} /></button>
    {/if}
  </div>
</div>
<div class="panel-body">
  {#if !folder}
    <p class="panel-empty">{t.noFolder}</p>
  {:else if nodes.length === 0}
    <p class="panel-empty">{t.emptyFolder}</p>
  {:else}
    <TreeRows {nodes} depth={0} current={ui.doc?.path ?? null} />
  {/if}
</div>
