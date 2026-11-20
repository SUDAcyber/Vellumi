// Source preservation: a save rewrites only the blocks the user changed.
//
// The editor serializes the WHOLE document from its tree, so every save used
// to re-spell untouched text: `---` became `***`, a literal `*` or `_` gained a
// backslash, table padding was redone, trailing spaces vanished. Over a real
// notes folder that touched 6 of every 10 files on the first save.
//
// preserveSource() compares the serializer's output with the text that was
// loaded, block by block (top-level Markdown blocks, parsed by the editor's
// own remark processor). A block whose parsed tree is identical to an
// original block is written back as the ORIGINAL text; changed blocks keep the
// serializer's text. The result is then parsed once more and must produce the
// exact same tree as the serializer's output — otherwise the plain output is
// returned. So this can only change spelling, never what the document says.

// Structural identity of a node: everything but source positions. A
// reference link/image counts as the inline link/image it resolves to (the
// editor writes `[a][ref]` as `[a](url)`), so the original spelling is kept.
function nodeKey(node, definitions) {
  return JSON.stringify(node, (name, value) => {
    if (name === "position") return undefined;
    let result = value;
    if (value && (value.type === "linkReference" || value.type === "imageReference")) {
      const definition = definitions.get(normalizeLabel(value.identifier ?? value.label ?? ""));
      if (definition) {
        result =
          value.type === "linkReference"
            ? { type: "link", title: definition.title ?? null, url: definition.url, children: value.children }
            : { type: "image", title: definition.title ?? null, url: definition.url, alt: value.alt ?? null };
      }
    }
    // Key order is not meaning (remark and Milkdown's transforms differ).
    if (result && typeof result === "object" && !Array.isArray(result)) {
      return Object.fromEntries(Object.keys(result).sort().map((key) => [key, result[key]]));
    }
    return result;
  });
}

function normalizeLabel(label) {
  return String(label).replace(/\s+/g, " ").trim().toLowerCase();
}

function definitionsOf(tree) {
  const definitions = new Map();
  const walk = (node) => {
    if (node?.type === "definition") {
      const label = normalizeLabel(node.identifier ?? node.label ?? "");
      if (!definitions.has(label)) definitions.set(label, node);
    }
    if (Array.isArray(node?.children)) node.children.forEach(walk);
  };
  walk(tree);
  return definitions;
}

// `parsed` is the editor's tree, or { tree, positions } where `positions` are
// the top-level positions from before remark transforms ran. Transforms build
// some nodes from scratch (image blocks, Mermaid diagrams) without source
// positions; when they map 1:1 onto the raw blocks, the raw one is used.
function blocksOf(parsed, text) {
  const tree = parsed?.tree ?? parsed;
  const children = Array.isArray(tree?.children) ? tree.children : [];
  const raw = Array.isArray(parsed?.positions) && parsed.positions.length === children.length ? parsed.positions : null;
  const definitions = definitionsOf(tree);
  const blocks = [];
  for (let i = 0; i < children.length; i += 1) {
    const node = children[i];
    const position = node.position ?? raw?.[i];
    const start = position?.start?.offset;
    const end = position?.end?.offset;
    if (typeof start !== "number" || typeof end !== "number") return null;
    blocks.push({ key: nodeKey(node, definitions), type: node.type, start, end, text: text.slice(start, end) });
  }
  return blocks;
}

const DEFINITION_TYPES = new Set(["definition", "footnoteDefinition"]);

function shifted(block, by, text) {
  const start = block.start + by;
  const end = block.end + by;
  return { key: block.key, type: block.type, start, end, text: text.slice(start, end) };
}

// Parses a text that is usually a small edit of the previous one: only the
// changed region plus one unchanged block on each side is parsed again. The
// two margin blocks must come back identical, which shows the edit did not
// reach past them (lists merging, a fence left open…). Edits touching link or
// footnote definitions, or any doubt, take a full parse. Reference links in
// the region still resolve: the document's definitions are parsed with it.
export function createIncrementalBlocks(parse) {
  let previous = null; // { text, blocks, definitions }

  const full = (text) => {
    const blocks = blocksOf(parse(text), text);
    previous = blocks
      ? {
          text,
          blocks,
          definitions: blocks.filter((b) => DEFINITION_TYPES.has(b.type)).map((b) => b.text).join("\n\n"),
        }
      : null;
    return blocks;
  };

  function blocksFor(text) {
    if (!previous) return full(text);
    if (text === previous.text) return previous.blocks;
    const old = previous.text;
    const blocks = previous.blocks;
    if (!blocks.length) return full(text);

    const limit = Math.min(old.length, text.length);
    let head = 0;
    while (head < limit && old.charCodeAt(head) === text.charCodeAt(head)) head += 1;
    let tail = 0;
    while (tail < limit - head && old.charCodeAt(old.length - 1 - tail) === text.charCodeAt(text.length - 1 - tail)) tail += 1;
    const delta = text.length - old.length;
    const changeEnd = old.length - tail; // in the old text

    let first = blocks.findIndex((b) => b.end >= head);
    if (first < 0) first = blocks.length;
    let last = -1;
    for (let k = blocks.length - 1; k >= 0; k -= 1) {
      if (blocks[k].start <= changeEnd) {
        last = k;
        break;
      }
    }
    const lo = Math.max(0, Math.min(first, last + 1) - 1);
    const hi = Math.min(blocks.length - 1, Math.max(first - 1, last) + 1);
    const leadingMargin = lo < first && lo <= last;
    const trailingMargin = hi > last && hi >= first;
    for (let k = lo; k <= hi; k += 1) {
      if (DEFINITION_TYPES.has(blocks[k].type)) return full(text);
    }

    const sliceStart = leadingMargin ? blocks[lo].start : lo === 0 ? 0 : blocks[lo].start;
    const sliceEnd = (trailingMargin ? blocks[hi].end : hi === blocks.length - 1 ? old.length : blocks[hi].end) + delta;
    if (sliceStart > head || sliceEnd < text.length - tail || sliceEnd < sliceStart) return full(text);

    const region = text.slice(sliceStart, sliceEnd);
    const combined = previous.definitions ? `${region}\n\n${previous.definitions}\n` : region;
    const parsed = blocksOf(parse(combined), combined);
    if (!parsed) return full(text);
    const inner = parsed.filter((b) => b.start < region.length);
    if (inner.some((b) => b.end > region.length || DEFINITION_TYPES.has(b.type))) return full(text);
    if (leadingMargin && (!inner.length || inner[0].key !== blocks[lo].key || inner[0].start !== 0)) return full(text);
    if (trailingMargin) {
      const end = inner[inner.length - 1];
      if (!end || end.key !== blocks[hi].key || end.end !== region.length) return full(text);
    }

    const next = [
      ...blocks.slice(0, lo),
      ...inner.map((b) => shifted(b, sliceStart, text)),
      ...blocks.slice(hi + 1).map((b) => shifted(b, delta, text)),
    ];
    previous = { text, blocks: next, definitions: previous.definitions };
    return next;
  }
  /** Adopts blocks already known for `text` (no parse). */
  blocksFor.prime = (text, blocks) => {
    if (!blocks) return;
    previous = {
      text,
      blocks,
      definitions: blocks.filter((b) => DEFINITION_TYPES.has(b.type)).map((b) => b.text).join("\n\n"),
    };
  };
  return blocksFor;
}

// Pairs of [outputIndex, originalIndex] for blocks with equal keys, in order.
// Edits are local, so common head and tail are matched directly and only the
// middle is aligned (LCS when small enough, otherwise a greedy forward scan).
export function alignBlocks(original, output) {
  const pairs = [];
  let head = 0;
  while (head < original.length && head < output.length && original[head].key === output[head].key) {
    pairs.push([head, head]);
    head += 1;
  }
  let tailOriginal = original.length;
  let tailOutput = output.length;
  const tail = [];
  while (tailOriginal > head && tailOutput > head && original[tailOriginal - 1].key === output[tailOutput - 1].key) {
    tailOriginal -= 1;
    tailOutput -= 1;
    tail.push([tailOutput, tailOriginal]);
  }

  const rows = tailOutput - head;
  const cols = tailOriginal - head;
  if (rows > 0 && cols > 0) {
    if (rows * cols <= 4_000_000) {
      // LCS table over the middle.
      const width = cols + 1;
      const table = new Uint32Array((rows + 1) * width);
      for (let i = rows - 1; i >= 0; i -= 1) {
        for (let j = cols - 1; j >= 0; j -= 1) {
          table[i * width + j] =
            output[head + i].key === original[head + j].key
              ? table[(i + 1) * width + j + 1] + 1
              : Math.max(table[(i + 1) * width + j], table[i * width + j + 1]);
        }
      }
      let i = 0;
      let j = 0;
      while (i < rows && j < cols) {
        if (output[head + i].key === original[head + j].key) {
          pairs.push([head + i, head + j]);
          i += 1;
          j += 1;
        } else if (table[(i + 1) * width + j] >= table[i * width + j + 1]) {
          i += 1;
        } else {
          j += 1;
        }
      }
    } else {
      const positions = new Map();
      for (let j = head; j < tailOriginal; j += 1) {
        const list = positions.get(original[j].key);
        if (list) list.push(j);
        else positions.set(original[j].key, [j]);
      }
      let floor = head;
      for (let i = head; i < tailOutput; i += 1) {
        const list = positions.get(output[i].key);
        const next = list?.find((j) => j >= floor);
        if (next !== undefined) {
          pairs.push([i, next]);
          floor = next + 1;
        }
      }
    }
  }
  return pairs.concat(tail.reverse());
}

/** `onFallback(reason)` is told when a merge was rejected (diagnostics). */
export function createSourcePreserver(parse, onFallback = () => {}) {
  let cachedText = null;
  let cachedBlocks = null;
  const originalBlocks = (text) => {
    if (text !== cachedText) {
      cachedText = text;
      primed = false;
      cachedBlocks = blocksOf(parse(text), text);
    }
    return cachedBlocks;
  };
  // Output and merged texts change a little per edit: parsed incrementally.
  const outputBlocks = createIncrementalBlocks(parse);
  const mergedBlocks = createIncrementalBlocks(parse);
  let primed = false;

  return function preserveSource(originalText, outputText) {
    if (typeof originalText !== "string" || typeof outputText !== "string") return outputText;
    if (originalText === outputText) return outputText;
    try {
      const original = originalBlocks(originalText);
      if (original && !primed) {
        // The first merged text of a document is usually the original itself.
        mergedBlocks.prime(originalText, original);
        primed = true;
      }
      const output = outputBlocks(outputText);
      if (!original || !output || !original.length || !output.length) return outputText;

      const matchOf = new Array(output.length).fill(-1);
      for (const [i, j] of alignBlocks(original, output)) matchOf[i] = j;
      if (!matchOf.some((j) => j >= 0)) return outputText;

      let merged = "";
      const firstMatch = matchOf[0];
      merged += firstMatch === 0 ? originalText.slice(0, original[0].start) : outputText.slice(0, output[0].start);
      for (let i = 0; i < output.length; i += 1) {
        const j = matchOf[i];
        if (i > 0) {
          const previous = matchOf[i - 1];
          // Original spacing between two blocks that were neighbours there.
          merged +=
            j >= 0 && previous >= 0 && j === previous + 1
              ? originalText.slice(original[previous].end, original[j].start)
              : outputText.slice(output[i - 1].end, output[i].start);
        }
        merged += j >= 0 ? original[j].text : output[i].text;
      }
      const lastOutput = output.length - 1;
      merged +=
        matchOf[lastOutput] === original.length - 1
          ? originalText.slice(original[original.length - 1].end)
          : outputText.slice(output[lastOutput].end);

      if (merged === outputText) return outputText;
      // Every block reused in order, nothing new: this IS the original text,
      // whose blocks equal the output's by construction.
      if (merged === originalText && output.length === original.length && matchOf.every((j, i) => j === i)) {
        return merged;
      }
      // Safety net: the merged text must mean exactly what the output means.
      const check = mergedBlocks(merged);
      if (!check || check.length !== output.length) {
        onFallback(`block count ${check?.length} != ${output.length}`);
        return outputText;
      }
      for (let i = 0; i < output.length; i += 1) {
        if (check[i].key !== output[i].key) {
          onFallback(`block ${i} differs: ${check[i].key.slice(0, 300)} vs ${output[i].key.slice(0, 300)}`);
          return outputText;
        }
      }
      return merged;
    } catch (error) {
      onFallback(String(error?.stack ?? error));
      return outputText;
    }
  };
}
