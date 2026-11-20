// node --test packages/editor/test
import assert from "node:assert/strict";
import { test } from "node:test";

import { Schema } from "@milkdown/prose/model";

import { SPACE_PLACEHOLDER, serializeKeepingRuns } from "../src/mark-runs.js";

const schema = new Schema({
  nodes: {
    doc: { content: "paragraph+" },
    paragraph: { content: "text*" },
    text: {},
  },
  marks: { strong: {}, code: { code: true }, link: { attrs: { href: {} } } },
});
const { strong, code, link } = schema.marks;
const t = (text, marks = []) => schema.text(text, marks);
const doc = (...inline) => schema.node("doc", null, [schema.node("paragraph", null, inline)]);
const seen = (d) => d.textContent;

test("spaces inside a bold run around inline code are protected, then restored", () => {
  const d = doc(t("调用 ", [strong.create()]), t("POST", [strong.create(), code.create()]), t(" 批量提交", [strong.create()]));
  let serialized = null;
  const result = serializeKeepingRuns(d, (x) => (serialized = seen(x)));
  assert.equal(serialized, `调用${SPACE_PLACEHOLDER}POST${SPACE_PLACEHOLDER}批量提交`);
  assert.equal(result, "调用 POST 批量提交");
});

test("a space between two separately bold words is left alone", () => {
  const d = doc(t("a", [strong.create()]), t(" "), t("b", [strong.create()]));
  let serialized = null;
  serializeKeepingRuns(d, (x) => (serialized = seen(x)));
  assert.equal(serialized, "a b");
});

test("link runs and run edges", () => {
  const href = link.create({ href: "u" });
  const d = doc(t("X", [href, code.create()]), t(" docs ", [href]), t("after"));
  let serialized = null;
  serializeKeepingRuns(d, (x) => (serialized = seen(x)));
  assert.equal(serialized, `X${SPACE_PLACEHOLDER}docs after`, "only the interior space");
});
