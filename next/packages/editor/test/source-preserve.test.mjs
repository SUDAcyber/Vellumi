// node --test packages/editor/test
import assert from "node:assert/strict";
import { test } from "node:test";

import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkMath from "remark-math";
import remarkStringify from "remark-stringify";

import { alignBlocks, createIncrementalBlocks, createSourcePreserver } from "../src/source-preserve.js";
import { readFileSync, readdirSync } from "node:fs";

const processor = unified().use(remarkParse).use(remarkGfm).use(remarkMath);
const parse = (text) => processor.runSync(processor.parse(text), text);
const stringify = (text) =>
  unified().use(remarkParse).use(remarkGfm).use(remarkMath).use(remarkStringify, { rule: "*" }).processSync(text).toString();

const ORIGINAL = [
  "# 标题",
  "",
  "第一段 *不是强调。*Pete 和 snake_case 与 [注]。  ",
  "",
  "---",
  "",
  "| 区域 | 说明 |",
  "|---|---|",
  "| A1 | 内场 |",
  "",
  "* 一",
  "* 二",
  "",
].join("\n");

test("an unchanged document is saved byte for byte", () => {
  const preserve = createSourcePreserver(parse);
  const output = stringify(ORIGINAL);
  assert.notEqual(output, ORIGINAL, "the serializer re-spells this fixture");
  assert.equal(preserve(ORIGINAL, output), ORIGINAL);
});

test("only the edited block is re-serialized", () => {
  const preserve = createSourcePreserver(parse);
  const edited = ORIGINAL.replace("# 标题", "# 新标题");
  const result = preserve(ORIGINAL, stringify(edited));
  assert.ok(result.startsWith("# 新标题\n"));
  assert.ok(result.includes("|---|---|"), "untouched table keeps its spelling");
  assert.ok(result.includes("\n---\n"), "untouched rule keeps its marker");
  assert.ok(result.includes("snake_case"), "no escape added");
});

test("inserted and deleted blocks keep their neighbours' original text", () => {
  const preserve = createSourcePreserver(parse);
  const inserted = ORIGINAL.replace("---\n", "---\n\n新的一段\n");
  const result = preserve(ORIGINAL, stringify(inserted));
  assert.ok(result.includes("\n---\n\n新的一段\n\n| 区域 | 说明 |"));
  const deleted = ORIGINAL.replace("---\n\n", "");
  const result2 = preserve(ORIGINAL, stringify(deleted));
  assert.ok(!result2.includes("---\n") && result2.includes("|---|---|"));
  assert.deepEqual(parse(result2).children.length, parse(deleted).children.length);
});

test("falls back to the serializer when reuse would change meaning", () => {
  // Two lists that only stay apart because the serializer alternated markers:
  // reusing the original `*` list next to a new `*` list would merge them.
  const preserve = createSourcePreserver(parse);
  const original = "* a\n* b\n";
  const output = "* a\n* b\n\n- c\n";
  const merged = preserve(original, output);
  const keys = (text) => parse(text).children.map((n) => n.type + (n.children?.length ?? ""));
  assert.deepEqual(keys(merged), keys(output));
});

test("alignment pairs equal keys in order", () => {
  const blocks = (keys) => keys.map((key) => ({ key }));
  assert.deepEqual(alignBlocks(blocks(["a", "b", "c", "d"]), blocks(["a", "x", "c", "d"])), [
    [0, 0],
    [2, 2],
    [3, 3],
  ]);
  assert.deepEqual(alignBlocks(blocks(["a", "b"]), blocks(["b", "a"])).length, 1);
});

test("a reference link keeps its spelling when the editor inlines it", () => {
  const preserve = createSourcePreserver(parse);
  const original = "见 [文档][ref]。\n\n[ref]: https://example.com \"标题\"\n";
  const output = "见 [文档](https://example.com \"标题\")。\n\n[ref]: https://example.com \"标题\"\n";
  assert.equal(preserve(original, output), original);
});

test("transform-built nodes without positions use the raw parse", () => {
  const diagram = () => (tree) => {
    tree.children = tree.children.map((n) => (n.type === "code" && n.lang === "mermaid" ? { type: "diagram", value: n.value } : n));
  };
  const p = unified().use(remarkParse).use(remarkGfm).use(remarkMath).use(diagram);
  const parseBoth = (text) => {
    const raw = p.parse(text);
    const positions = raw.children.map((node) => node.position);
    return { positions, tree: p.runSync(raw, text) };
  };
  const original = "![图](a.png)\n\n```mermaid\ngraph LR; A-->B;\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
  const output = "![图](a.png)\n\n```mermaid\ngraph LR; A-->B;\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n";
  assert.equal(createSourcePreserver(parseBoth)(original, output), original);
});

test("incremental parsing matches a full parse after random edits", () => {
  const dir = new URL("../../../fixtures/fidelity/input/", import.meta.url);
  const docs = readdirSync(dir).filter((n) => n.endsWith(".md")).map((n) => readFileSync(new URL(n, dir), "utf8"));
  const snippets = ["x", "\n\n", "\n", "- ", "* ", "1. ", "```", "\n```\n", "# ", "|", "[a][ref]", "[ref]: http://e.com\n", "> ", "    ", "---", "**", "$$", "<!--", "\n\n- item\n"];
  let seed = 7;
  const random = (n) => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed % n;
  };
  const full = (text) => parse(text).children.map((n) => [n.type, n.position.start.offset, n.position.end.offset]);
  let checked = 0;
  for (const base of docs) {
    const blocks = createIncrementalBlocks(parse);
    let text = base;
    blocks(text);
    for (let step = 0; step < 150; step += 1) {
      const at = random(text.length + 1);
      if (random(3) === 0 && text.length > 2) {
        text = text.slice(0, at) + text.slice(at + 1 + random(6));
      } else {
        text = text.slice(0, at) + snippets[random(snippets.length)] + text.slice(at);
      }
      const got = blocks(text);
      assert.deepEqual(got.map((b) => [b.type, b.start, b.end]), full(text), `doc edit ${step}`);
      assert.deepEqual(got.map((b) => b.key), createIncrementalBlocks(parse)(text).map((b) => b.key), `keys after edit ${step}`);
      checked += 1;
    }
  }
  assert.ok(checked > 100);
});
