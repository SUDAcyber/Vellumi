// Fidelity patches for Milkdown Crepe 7.21.2. Crepe "normalizes" Markdown on
// every load. Spelling changes are acceptable, but these rewrites changed a
// document's CONTENT or rendering, and Vellumi 0.5.x shipped all of them; the
// rewrite fixes them so saving never loses data (REWRITE_PLAN hard req. 1):
//
// 1. Image alt text: the image block stores its resize ratio in the alt, so
//    `![描述](a.png)` was saved as `![1.00](a.png)`.
// 2. `$$x$$` on one line is inline math to remark-math, and the serializer
//    picks the shortest fence, saving it as `$x$`.
// 3. Tight lists: list `spread` is parsed into the STRING "false", which the
//    serializer's join rule ignores, so every tight list was saved loose.
// 4. List markers: always `*` and `.`; the document's own markers are kept.
//
// Edge newlines (blank line after front matter, final newline) and CRLF are
// restored on the Rust side (vellumi-core DocumentSession).
//
// Patches 1–2 re-register a node under the same name: Milkdown builds its
// schema with Object.fromEntries, so the later registration wins.

import { $nodeSchema, $remark } from "@milkdown/utils";
import { visit } from "unist-util-visit";
import katex from "katex";

import {
  detectListMarkers,
  imageAltForMarkdown,
  mathFenceAt,
  ratioFromAlt,
  rawMathMarkdown,
  stringSpreadJoin,
} from "./fidelity-rules.js";

// ---- 1. Image block alt text ---------------------------------------------

const IMAGE_DATA_TYPE = "image-block";

const imageBlockFidelity = $nodeSchema(IMAGE_DATA_TYPE, () => ({
  inline: false,
  group: "block",
  selectable: true,
  draggable: true,
  isolating: true,
  marks: "",
  atom: true,
  priority: 100,
  attrs: {
    src: { default: "", validate: "string" },
    caption: { default: "", validate: "string" },
    ratio: { default: 1, validate: "number" },
    alt: { default: "", validate: "string" },
  },
  parseDOM: [
    {
      tag: `img[data-type="${IMAGE_DATA_TYPE}"]`,
      getAttrs: (dom) => ({
        src: dom.getAttribute("src") || "",
        caption: dom.getAttribute("caption") || "",
        ratio: Number(dom.getAttribute("ratio") ?? 1),
        alt: dom.getAttribute("alt") || "",
      }),
    },
  ],
  toDOM: (node) => ["img", { "data-type": IMAGE_DATA_TYPE, ...node.attrs }],
  parseMarkdown: {
    match: ({ type }) => type === IMAGE_DATA_TYPE,
    runner: (state, node, type) => {
      const alt = typeof node.alt === "string" ? node.alt : "";
      state.addNode(type, {
        src: node.url,
        caption: node.title ?? "",
        ratio: ratioFromAlt(alt),
        alt,
      });
    },
  },
  toMarkdown: {
    match: (node) => node.type.name === IMAGE_DATA_TYPE,
    runner: (state, node) => {
      state.openNode("paragraph");
      state.addNode("image", undefined, undefined, {
        title: node.attrs.caption,
        url: node.attrs.src,
        alt: imageAltForMarkdown(node.attrs.alt, node.attrs.ratio),
      });
      state.closeNode();
    },
  },
}));

// ---- 2. Inline math fence ------------------------------------------------

const MATH_INLINE = "math_inline";

// Records how many `$` opened each inline math span in the source.
const mathFenceRemark = $remark("vellumiMathFence", () => () => (tree, file) => {
  const source = String(file?.value ?? "");
  visit(tree, "inlineMath", (node) => {
    const start = node.position?.start?.offset;
    if (typeof start !== "number") return;
    node.data = { ...(node.data ?? {}), vellumiFence: mathFenceAt(source, start) };
  });
});

const mathInlineFidelity = $nodeSchema(MATH_INLINE, () => ({
  group: "inline",
  inline: true,
  draggable: true,
  atom: true,
  attrs: {
    value: { default: "" },
    fence: { default: 1 },
  },
  parseDOM: [
    {
      tag: `span[data-type="${MATH_INLINE}"]`,
      getAttrs: (dom) => ({
        value: dom.dataset.value ?? "",
        fence: Number(dom.dataset.fence ?? 1) || 1,
      }),
    },
  ],
  toDOM: (node) => {
    const dom = document.createElement("span");
    dom.dataset.type = MATH_INLINE;
    dom.dataset.value = node.attrs.value;
    dom.dataset.fence = String(node.attrs.fence);
    katex.render(node.attrs.value, dom, { throwOnError: false });
    return dom;
  },
  parseMarkdown: {
    match: (node) => node.type === "inlineMath",
    runner: (state, node, type) => {
      state.addNode(type, { value: node.value, fence: node.data?.vellumiFence ?? 1 });
    },
  },
  toMarkdown: {
    match: (node) => node.type.name === MATH_INLINE,
    runner: (state, node) => {
      const raw = rawMathMarkdown(node.attrs.value, Number(node.attrs.fence) || 1);
      if (raw === null) {
        state.addNode("inlineMath", undefined, node.attrs.value);
      } else {
        // An html node is written verbatim, so the `$$` fence survives.
        state.addNode("html", undefined, raw);
      }
    },
  },
}));

// ---- 3 + 4. Serializer extension -------------------------------------------

// Registered as a to-markdown extension on the remark processor itself. (An
// editor.config() update of remarkStringifyOptionsCtx raced Milkdown's init:
// the stringify options were already baked into the processor.) Keys other
// than join/handlers/unsafe become serializer options (bullet, …).
function serializerExtension(markdown) {
  const extension = { join: [stringSpreadJoin], ...detectListMarkers(markdown) };
  return $remark("vellumiSerializer", () => function vellumiSerializer() {
    const data = this.data();
    (data.toMarkdownExtensions ??= []).push(extension);
  });
}

// ---- Install -----------------------------------------------------------

/** Registers every patch on a Crepe editor before `create()`. `markdown` is
 *  the document being loaded (its list markers are kept). */
export function installFidelity(editor, markdown) {
  editor
    .use(serializerExtension(markdown))
    .use(imageBlockFidelity)
    .use(mathFenceRemark)
    .use(mathInlineFidelity);
}
