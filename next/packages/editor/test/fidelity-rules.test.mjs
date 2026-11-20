// node --test packages/editor/test
import assert from "node:assert/strict";
import { test } from "node:test";

import {
  detectListMarkers,
  imageAltForMarkdown,
  mathFenceAt,
  rawMathMarkdown,
  stringSpreadJoin,
} from "../src/fidelity-rules.js";

test("image alt: unchanged ratio writes the original alt back", () => {
  assert.equal(imageAltForMarkdown("书桌上的稿纸", 1), "书桌上的稿纸");
  assert.equal(imageAltForMarkdown("", 1), "");
  assert.equal(imageAltForMarkdown("1.50", 1.5), "1.50");
  assert.equal(imageAltForMarkdown("1.00", 1), "1.00");
});

test("image alt: a resize uses the ratio only where the alt was empty or numeric", () => {
  assert.equal(imageAltForMarkdown("", 0.5), "0.50");
  assert.equal(imageAltForMarkdown("1.50", 0.75), "0.75");
  assert.equal(imageAltForMarkdown("描述", 0.5), "描述");
});

test("inline math keeps a $$ fence", () => {
  assert.equal(rawMathMarkdown("E=mc^2", 2), "$$E=mc^2$$");
  assert.equal(rawMathMarkdown("E=mc^2", 1), null);
  assert.equal(rawMathMarkdown("a$$b", 2), null);
  assert.equal(rawMathMarkdown("a\nb", 2), null);
  assert.equal(mathFenceAt("x $$a$$ y", 2), 2);
  assert.equal(mathFenceAt("x $a$ y", 2), 1);
  assert.equal(mathFenceAt("abc", 10), 1);
});

test("string spread is honored by the join rule", () => {
  const p = { type: "paragraph" };
  const item = { type: "listItem" };
  assert.equal(stringSpreadJoin(item, item, { spread: "false" }), 0);
  assert.equal(stringSpreadJoin(item, item, { spread: "true" }), 1);
  assert.equal(stringSpreadJoin(item, item, { spread: false }), undefined);
  assert.equal(stringSpreadJoin(p, p, { spread: "false" }), undefined);
});

test("list markers are detected outside code and thematic breaks", () => {
  assert.deepEqual(detectListMarkers("- a\n1) b\n"), { bullet: "-", bulletOrdered: ")" });
  assert.deepEqual(detectListMarkers("* * *\n\n+ x\n"), { bullet: "+" });
  assert.deepEqual(detectListMarkers("```\n- in code\n```\n* real\n"), { bullet: "*" });
  assert.deepEqual(detectListMarkers("plain text"), {});
});

test("an empty list item drops the editor's empty-line marker", async () => {
  const { dropEmptyItemBreaks } = await import("../src/fidelity-rules.js");
  const item = (children) => ({ type: "listItem", children });
  const br = { type: "paragraph", children: [{ type: "html", value: "<br />" }] };
  const tree = { type: "root", children: [{ type: "list", children: [item([br]), item([{ type: "paragraph", children: [{ type: "text", value: "a" }] }])] }] };
  dropEmptyItemBreaks(tree);
  assert.deepEqual(tree.children[0].children[0].children, []);
  assert.equal(tree.children[0].children[1].children.length, 1);
});
