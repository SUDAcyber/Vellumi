// Makes block "islands" visibly part of a selection. Code blocks (a nested
// CodeMirror), Mermaid diagrams, display math and image blocks are rendered by
// node views that ignore the browser's text selection, so a selection running
// over them looked like it stopped at their edges. Every such block that lies
// entirely inside a non-empty selection gets `vellumi-block-selected`.

import { $prose } from "@milkdown/utils";
import { Plugin, PluginKey } from "@milkdown/prose/state";
import { Decoration, DecorationSet } from "@milkdown/prose/view";

const ISLANDS = new Set(["code_block", "diagram", "image-block", "math_display", "table"]);

export const selectionKey = new PluginKey("vellumi-block-selection");

/** [from, to] ranges of island blocks fully covered by the selection. */
export function coveredIslands(doc, from, to) {
  const ranges = [];
  if (from === to) return ranges;
  doc.nodesBetween(from, to, (node, pos) => {
    if (ISLANDS.has(node.type.name)) {
      if (pos >= from && pos + node.nodeSize <= to) ranges.push([pos, pos + node.nodeSize]);
      return false;
    }
    return true;
  });
  return ranges;
}

export const blockSelectionPlugin = $prose(
  () =>
    new Plugin({
      key: selectionKey,
      props: {
        decorations(state) {
          const { from, to } = state.selection;
          const ranges = coveredIslands(state.doc, from, to);
          if (!ranges.length) return DecorationSet.empty;
          return DecorationSet.create(
            state.doc,
            ranges.map(([start, end]) => Decoration.node(start, end, { class: "vellumi-block-selected" })),
          );
        },
      },
    }),
);
