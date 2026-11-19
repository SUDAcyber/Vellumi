<script lang="ts">
  import { api, type TreeNode } from "../lib/ipc";
  import Icon from "./Icon.svelte";
  import TreeRows from "./TreeRows.svelte";

  let { nodes, depth, current }: { nodes: TreeNode[]; depth: number; current: string | null } = $props();

  // Folders on the path to the current file start expanded, others closed.
  const open = $state<Record<string, boolean>>({});
  const isOpen = (node: TreeNode) => open[node.path] ?? (current?.startsWith(node.path + "/") ?? false);
</script>

<ul role={depth === 0 ? "tree" : "group"}>
  {#each nodes as node (node.path)}
    <li role="treeitem" aria-expanded={node.isDir ? isOpen(node) : undefined} aria-selected={node.path === current}>
      {#if node.isDir}
        <button class="row" style:padding-left="{8 + depth * 14}px" onclick={() => (open[node.path] = !isOpen(node))}>
          <Icon name={isOpen(node) ? "chevron-down" : "chevron-right"} size={14} />
          <span class="name">{node.name}</span>
        </button>
        {#if isOpen(node)}
          <TreeRows nodes={node.children} depth={depth + 1} {current} />
        {/if}
      {:else}
        <button
          class="row"
          class:current={node.path === current}
          style:padding-left="{22 + depth * 14}px"
          title={node.path}
          onclick={() => node.path !== current && api.openPath(node.path)}
        >
          <Icon name="file" size={14} />
          <span class="name">{node.name}</span>
        </button>
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
