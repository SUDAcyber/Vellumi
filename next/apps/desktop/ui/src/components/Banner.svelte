<script lang="ts">
  import { run } from "../lib/actions";
  import { t } from "../lib/i18n";
  import { api } from "../lib/ipc";
  import { ui } from "../lib/store.svelte";

  async function reload(): Promise<void> {
    await api.reloadFromDisk();
    ui.banner = null;
  }

  async function keepMine(): Promise<void> {
    await api.keepMine();
    ui.banner = null;
  }
</script>

{#if ui.banner?.kind === "conflict"}
  <div class="banner" role="alert">
    <span>{t.conflict}</span>
    <span class="spacer"></span>
    <button class="button" onclick={reload}>{t.reloadDisk}</button>
    <button class="button primary" onclick={keepMine}>{t.keepMine}</button>
  </div>
{:else if ui.banner?.kind === "deleted"}
  <div class="banner" role="alert">
    <span>{t.deleted}</span>
    <span class="spacer"></span>
    <button class="button primary" onclick={() => run("file.saveAs")}>{t.saveAs}</button>
    <button class="button" onclick={() => (ui.banner = null)}>{t.dismiss}</button>
  </div>
{/if}

<style>
  .banner {
    position: sticky;
    top: 0;
    z-index: 8;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 16px;
    background: var(--warn-bg);
    border-bottom: 1px solid var(--warn-line);
    color: var(--warn-ink);
    font-size: 12px;
  }
  .spacer {
    flex: 1;
  }
</style>
