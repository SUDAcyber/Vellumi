// Keeps a formatted run whole when it contains inline code or a link.
//
// Milkdown serializes mark by mark and closes every mark after each text
// node, then re-merges neighbours. Before that merge it moves edge spaces OUT
// of a mark (`**a **` is not valid Markdown), so a space next to inline code
// splits the run:
//
//   **调用 `POST` 批量提交**   →   **调用** **`POST`** **批量提交**
//   [`X` documentation](u)     →   [`X`](u) [documentation](u)
//
// That adds characters and changes rendering. Before serializing, spaces that
// sit INSIDE a run (the neighbouring node carries the same mark) are swapped
// for a placeholder Milkdown does not move; the merge then succeeds and the
// placeholder becomes a space again in the text.

import { Fragment } from "@milkdown/prose/model";

export const SPACE_PLACEHOLDER = "";

function sharesRunMark(child, other) {
  if (!other) return false;
  return child.marks.some((mark) => !mark.type.spec.code && mark.isInSet(other.marks));
}

function protectTextblock(node) {
  const children = [];
  node.forEach((child) => children.push(child));
  let changed = false;
  const next = children.map((child, index) => {
    if (!child.isText || !child.marks.length) return child;
    let text = child.text;
    if (sharesRunMark(child, children[index + 1])) {
      text = text.replace(/ +$/, (spaces) => SPACE_PLACEHOLDER.repeat(spaces.length));
    }
    if (sharesRunMark(child, children[index - 1])) {
      text = text.replace(/^ +/, (spaces) => SPACE_PLACEHOLDER.repeat(spaces.length));
    }
    if (text === child.text) return child;
    changed = true;
    return child.type.schema.text(text, child.marks);
  });
  return changed ? node.copy(Fragment.from(next)) : node;
}

function protect(node) {
  if (node.isTextblock) return protectTextblock(node);
  if (node.isLeaf) return node;
  const children = [];
  let changed = false;
  node.forEach((child) => {
    const mapped = protect(child);
    if (mapped !== child) changed = true;
    children.push(mapped);
  });
  return changed ? node.copy(Fragment.from(children)) : node;
}

/** Serializes `doc` with `serialize(doc) → markdown`, keeping runs whole. */
export function serializeKeepingRuns(doc, serialize) {
  if (doc.textContent.includes(SPACE_PLACEHOLDER)) return serialize(doc);
  const protectedDoc = protect(doc);
  if (protectedDoc === doc) return serialize(doc);
  return serialize(protectedDoc).split(SPACE_PLACEHOLDER).join(" ");
}
