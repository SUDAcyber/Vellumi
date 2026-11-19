// Floating format toolbar, arranged as in the design canvas (Main artboard):
// B  I  S  </>  |  link  “ — Crepe's own items and commands, regrouped, plus
// a quote toggle, Lucide-style stroke icons (ISC) and native tooltips with
// the matching menu shortcut.

import { commandsCtx, editorViewCtx } from "@milkdown/core";
import { lift } from "@milkdown/prose/commands";
import { wrapInBlockquoteCommand } from "@milkdown/preset-commonmark";

const svg = (body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;

export const toolbarIcons = {
  boldIcon: svg('<path d="M6 12h9a4 4 0 0 1 0 8H7a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h7a4 4 0 0 1 0 8"/>'),
  italicIcon: svg('<line x1="19" x2="10" y1="4" y2="4"/><line x1="14" x2="5" y1="20" y2="20"/><line x1="15" x2="9" y1="4" y2="20"/>'),
  strikethroughIcon: svg('<path d="M16 4H9a3 3 0 0 0-2.83 4"/><path d="M14 12a4 4 0 0 1 0 8H6"/><line x1="4" x2="20" y1="12" y2="12"/>'),
  codeIcon: svg('<path d="m18 16 4-4-4-4"/><path d="m6 8-4 4 4 4"/><path d="m14.5 4-5 16"/>'),
  linkIcon: svg('<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>'),
};

const quoteIcon = svg(
  '<path d="M16 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z"/><path d="M5 3a2 2 0 0 0-2 2v6a2 2 0 0 0 2 2 1 1 0 0 1 1 1v1a2 2 0 0 1-2 2 1 1 0 0 0-1 1v2a1 1 0 0 0 1 1 6 6 0 0 0 6-6V5a2 2 0 0 0-2-2z"/>',
);

function inBlockquote(ctx) {
  const { $from } = ctx.get(editorViewCtx).state.selection;
  for (let depth = $from.depth; depth > 0; depth -= 1) {
    if ($from.node(depth).type.name === "blockquote") return true;
  }
  return false;
}

/** Crepe `buildToolbar` hook: [B I S </>] | [link “]. */
export function buildToolbar(builder) {
  const groups = builder.build();
  const formatting = groups.find((group) => group.key === "formatting");
  const functions = groups.find((group) => group.key === "function");
  if (!formatting || !functions) return;
  const code = functions.items.find((item) => item.key === "code");
  // Inline math stays reachable by typing `$…$` and from the slash menu.
  functions.items = functions.items.filter((item) => item.key === "link");
  if (code) formatting.items.push(code);
  builder.getGroup("function").addItem("quote", {
    icon: quoteIcon,
    active: (ctx) => inBlockquote(ctx),
    onRun: (ctx) => {
      if (inBlockquote(ctx)) {
        const view = ctx.get(editorViewCtx);
        lift(view.state, view.dispatch);
      } else {
        ctx.get(commandsCtx).call(wrapInBlockquoteCommand.key);
      }
    },
  });
}

/** Tooltips in button order (the toolbar renders unlabeled buttons). */
export function labelToolbar(root, chinese) {
  const labels = chinese
    ? ["粗体 ⌘B", "斜体 ⌘I", "删除线 ⇧⌘X", "行内代码 ⌘E", "链接 ⌘K", "引用 ⇧⌘Q"]
    : ["Bold ⌘B", "Italic ⌘I", "Strikethrough ⇧⌘X", "Inline Code ⌘E", "Link ⌘K", "Quote ⇧⌘Q"];
  const apply = () => {
    root.querySelectorAll(".milkdown-toolbar").forEach((toolbar) => {
      toolbar.querySelectorAll("button.toolbar-item").forEach((button, index) => {
        const label = labels[index];
        if (label && button.getAttribute("aria-label") !== label) {
          button.setAttribute("aria-label", label);
          button.setAttribute("title", label);
        }
      });
    });
  };
  new MutationObserver(apply).observe(root, { childList: true, subtree: true });
  apply();
}
