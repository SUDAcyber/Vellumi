// Pure rules behind fidelity.js — no imports, so `node --test` can run them.

function ratioFromAlt(alt) {
  const ratio = Number(alt || 1);
  return Number.isNaN(ratio) || ratio === 0 ? 1 : ratio;
}

/** Image-block alt text to write: the original unless the user resized it. */
export function imageAltForMarkdown(alt, ratio) {
  const original = typeof alt === "string" ? alt : "";
  const current = Number.parseFloat(ratio);
  if (Number.isNaN(current) || Math.abs(current - ratioFromAlt(original)) < 1e-9) {
    return original;
  }
  // A resized image: keep Crepe's ratio-in-alt convention only where the alt
  // was empty or already a ratio; a real description is content and wins.
  if (original === "" || !Number.isNaN(Number(original))) {
    return current.toFixed(2);
  }
  return original;
}

export { ratioFromAlt };

/** Raw Markdown for `$$x$$` inline math, or null for the stock `$x$` path. */
export function rawMathMarkdown(value, fence) {
  if (fence >= 2 && !value.includes("$$") && !/[\r\n]/.test(value)) {
    return `$$${value}$$`;
  }
  return null;
}

/** Number of `$` opening the inline math that starts at `offset`. */
export function mathFenceAt(source, offset) {
  let fence = 0;
  while (source[offset + fence] === "$") fence += 1;
  return Math.max(1, fence);
}

/**
 * mdast-util-to-markdown join rule. Milkdown gives lists and items a STRING
 * spread ("true"/"false"); the stock rule only honors booleans, so tight lists
 * were written loose.
 */
export function stringSpreadJoin(left, right, parent) {
  if (!parent || typeof parent.spread !== "string") return undefined;
  if (left.type === "paragraph" && (right.type === "paragraph" || right.type === "definition")) {
    return undefined;
  }
  return parent.spread === "true" ? 1 : 0;
}

/** The bullet / ordered-list markers a document already uses. */
export function detectListMarkers(markdown) {
  const markers = {};
  let inFence = false;
  for (const line of String(markdown).split("\n")) {
    if (/^ {0,3}(```|~~~)/.test(line)) {
      inFence = !inFence;
      continue;
    }
    if (inFence) continue;
    if (!markers.bullet) {
      // A bullet item, but not a thematic break such as `* * *` or `- - -`.
      const bullet = /^\s*([-*+])[ \t]+(?!([-*_])[ \t]*(\2[ \t]*)+$)\S/.exec(line);
      if (bullet) markers.bullet = bullet[1];
    }
    if (!markers.bulletOrdered) {
      const ordered = /^\s*\d{1,9}([.)])[ \t]+\S/.exec(line);
      if (ordered) markers.bulletOrdered = ordered[1];
    }
    if (markers.bullet && markers.bulletOrdered) break;
  }
  return markers;
}

/** mdast: list items holding only the editor's empty-line `<br />`. */
export function dropEmptyItemBreaks(tree) {
  const walk = (node) => {
    if (!Array.isArray(node?.children)) return;
    if (node.type === "listItem" && node.children.length === 1) {
      const [only] = node.children;
      if (
        only.type === "paragraph" &&
        only.children?.length === 1 &&
        only.children[0].type === "html" &&
        /^<br\s*\/?>$/.test(String(only.children[0].value).trim())
      ) {
        node.children = [];
        return;
      }
    }
    node.children.forEach(walk);
  };
  walk(tree);
  return tree;
}
