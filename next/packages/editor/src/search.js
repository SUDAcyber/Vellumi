// In-document find & replace as a ProseMirror plugin (decision D9). Replaces
// 0.5.x's WebKit `window.find`, which could not report a position, ignored
// case settings and disagreed with the replace logic about matches spanning
// formatting (audit B11/B12). Here one matcher drives highlighting,
// navigation, counting and replacing:
//
// - each textblock is flattened to a string (inline atoms become U+FFFC) with
//   a parallel offset → document-position map, so "the **quick** fox" matches
//   "quick fox";
// - options: caseSensitive, wholeWord, regex.

import { $prose } from "@milkdown/utils";
import { Plugin, PluginKey, TextSelection } from "@milkdown/prose/state";
import { Decoration, DecorationSet } from "@milkdown/prose/view";

export const searchKey = new PluginKey("vellumi-search");

const EMPTY = { query: null, matches: [], current: -1 };

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Builds the matcher, or null for an empty / invalid query. */
export function compileQuery(query) {
  if (!query || typeof query.text !== "string" || query.text === "") return null;
  let source = query.regex ? query.text : escapeRegExp(query.text);
  if (query.wholeWord) source = `(?<![\\p{L}\\p{N}_])(?:${source})(?![\\p{L}\\p{N}_])`;
  try {
    return new RegExp(source, query.caseSensitive ? "gu" : "giu");
  } catch {
    return null;
  }
}

/** All matches in `doc` as [{from, to}] document positions. */
export function findMatches(doc, query) {
  const pattern = compileQuery(query);
  if (!pattern) return [];
  const matches = [];
  doc.descendants((node, pos) => {
    if (!node.isTextblock) return true;
    // Flatten the block; positions[i] is the doc position of text[i].
    let text = "";
    const positions = [];
    node.forEach((child, offset) => {
      const start = pos + 1 + offset;
      if (child.isText) {
        for (let i = 0; i < child.text.length; i += 1) positions.push(start + i);
        text += child.text;
      } else {
        positions.push(start);
        text += "￼";
      }
    });
    const blockEnd = pos + 1 + node.content.size;
    pattern.lastIndex = 0;
    let found;
    while ((found = pattern.exec(text)) !== null) {
      if (found[0].length === 0) {
        pattern.lastIndex += 1;
        continue;
      }
      const from = positions[found.index];
      const last = found.index + found[0].length;
      const to = last < positions.length ? positions[last] : blockEnd;
      matches.push({ from, to });
    }
    return false;
  });
  return matches;
}

function decorations(doc, state) {
  if (!state.matches.length) return DecorationSet.empty;
  return DecorationSet.create(
    doc,
    state.matches.map((match, index) =>
      Decoration.inline(match.from, match.to, {
        class: index === state.current ? "vellumi-match vellumi-match-current" : "vellumi-match",
      }),
    ),
  );
}

/** The current match: the first one at or after `from`. */
function nearest(matches, from) {
  const index = matches.findIndex((match) => match.from >= from);
  return index === -1 ? (matches.length ? 0 : -1) : index;
}

export const searchPlugin = $prose(
  () =>
    new Plugin({
      key: searchKey,
      state: {
        init: () => EMPTY,
        apply(tr, previous, _oldState, newState) {
          const meta = tr.getMeta(searchKey);
          if (meta?.type === "clear") return EMPTY;
          if (meta?.type === "query") {
            const matches = findMatches(newState.doc, meta.query);
            return { query: meta.query, matches, current: nearest(matches, newState.selection.from) };
          }
          if (meta?.type === "current") {
            return { ...previous, current: meta.current };
          }
          if (tr.docChanged && previous.query) {
            const matches = findMatches(newState.doc, previous.query);
            return {
              query: previous.query,
              matches,
              current: Math.min(previous.current, matches.length - 1),
            };
          }
          return previous;
        },
      },
      props: {
        decorations(state) {
          return decorations(state.doc, searchKey.getState(state) ?? EMPTY);
        },
      },
    }),
);

function result(state) {
  const search = searchKey.getState(state) ?? EMPTY;
  return { total: search.matches.length, current: search.current < 0 ? 0 : search.current + 1 };
}

function reveal(view, match) {
  view.dispatch(view.state.tr.setSelection(TextSelection.create(view.state.doc, match.from, match.to)).scrollIntoView());
  const dom = view.domAtPos(match.from)?.node;
  const element = dom instanceof Element ? dom : dom?.parentElement;
  element?.scrollIntoView?.({ block: "center", behavior: "instant" });
}

/** Commands used by moltenAPI (view = live EditorView). */
export const searchCommands = {
  setQuery(view, query) {
    view.dispatch(view.state.tr.setMeta(searchKey, { type: "query", query }));
    return result(view.state);
  },
  step(view, backwards) {
    const search = searchKey.getState(view.state) ?? EMPTY;
    if (!search.matches.length) return result(view.state);
    const count = search.matches.length;
    const current = search.current < 0 ? 0 : (search.current + (backwards ? -1 : 1) + count) % count;
    view.dispatch(view.state.tr.setMeta(searchKey, { type: "current", current }));
    reveal(view, search.matches[current]);
    return result(view.state);
  },
  replaceCurrent(view, replacement) {
    const search = searchKey.getState(view.state) ?? EMPTY;
    const match = search.matches[search.current];
    if (!match) return result(view.state);
    const text = expandReplacement(view.state.doc.textBetween(match.from, match.to, "", "￼"), search.query, replacement);
    view.dispatch(view.state.tr.insertText(text, match.from, match.to));
    const after = searchKey.getState(view.state) ?? EMPTY;
    if (after.matches.length) {
      const next = nearest(after.matches, match.from + text.length);
      view.dispatch(view.state.tr.setMeta(searchKey, { type: "current", current: next }));
      reveal(view, after.matches[next]);
    }
    return result(view.state);
  },
  replaceAll(view, replacement) {
    const search = searchKey.getState(view.state) ?? EMPTY;
    if (!search.matches.length) return 0;
    let tr = view.state.tr;
    // Back to front so earlier replacements don't shift later ranges; one
    // transaction = one undo step.
    for (let i = search.matches.length - 1; i >= 0; i -= 1) {
      const { from, to } = search.matches[i];
      const text = expandReplacement(view.state.doc.textBetween(from, to, "", "￼"), search.query, replacement);
      tr = tr.insertText(text, from, to);
    }
    view.dispatch(tr);
    return search.matches.length;
  },
  clear(view) {
    view.dispatch(view.state.tr.setMeta(searchKey, { type: "clear" }));
  },
  state(view) {
    return result(view.state);
  },
};

/** `$1`-style groups when searching by regular expression. */
function expandReplacement(matched, query, replacement) {
  const value = typeof replacement === "string" ? replacement : "";
  if (!query?.regex) return value;
  const pattern = compileQuery({ ...query, wholeWord: false });
  if (!pattern) return value;
  pattern.lastIndex = 0;
  return matched.replace(new RegExp(pattern.source, pattern.flags.replace("g", "")), value);
}
