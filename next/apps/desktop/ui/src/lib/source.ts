// Source mode (⌘/): the full file — front matter included — in CodeMirror 6.
// Rust holds the document; while this view is active it is authoritative
// (doc_source_* commands), and the rich editor is parked until exit.
import { defaultKeymap, history, historyKeymap, indentWithTab, redo, undo } from "@codemirror/commands";
import { markdown } from "@codemirror/lang-markdown";
import { defaultHighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { findNext, findPrevious, openSearchPanel, search, searchKeymap } from "@codemirror/search";
import { EditorState } from "@codemirror/state";
import { drawSelection, EditorView, highlightActiveLine, keymap, lineNumbers } from "@codemirror/view";

import { editor } from "./editor";
import { api } from "./ipc";
import { ui } from "./store.svelte";

declare global {
  interface Window {
    __vellumiSourceText?: () => string | null;
    __vellumiSourceHistory?: (direction: string) => boolean;
  }
}

let host: HTMLElement | null = null;
let view: EditorView | null = null;
let pushTimer: ReturnType<typeof setTimeout> | undefined;
let busy = false;

export function setSourceHost(element: HTMLElement): void {
  host = element;
}

export function sourceView(): EditorView | null {
  return view;
}

function pushNow(): Promise<void> {
  clearTimeout(pushTimer);
  if (!view) return Promise.resolve();
  return api.sourceChanged(view.state.doc.toString());
}

function scrollFraction(target: EditorView): number {
  const scroller = target.scrollDOM;
  const max = scroller.scrollHeight - scroller.clientHeight;
  return max > 1 ? scroller.scrollTop / max : 0;
}

function createView(parent: HTMLElement, text: string): EditorView {
  return new EditorView({
    parent,
    state: EditorState.create({
      doc: text,
      extensions: [
        lineNumbers(),
        history(),
        drawSelection(),
        highlightActiveLine(),
        markdown(),
        syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
        search({ top: true }),
        EditorView.lineWrapping,
        EditorView.contentAttributes.of({ spellcheck: "false", autocorrect: "off", autocapitalize: "off" }),
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, indentWithTab]),
        EditorView.updateListener.of((update) => {
          if (!update.docChanged) return;
          clearTimeout(pushTimer);
          pushTimer = setTimeout(() => void pushNow(), 150);
        }),
      ],
    }),
  });
}

export async function enterSource(): Promise<void> {
  if (!host || view || busy) return;
  busy = true;
  try {
    const fraction = editor()?.getScrollFraction() ?? 0;
    editor()?.clearSearch();
    ui.findOpen = false;
    const text = await api.sourceEnter(editor()?.getMarkdown() ?? null);
    view = createView(host, text);
    ui.sourceMode = true;
    window.__vellumiSourceText = () => view?.state.doc.toString() ?? null;
    window.__vellumiSourceHistory = (direction) => {
      const active = document.activeElement;
      if (!view || (active instanceof HTMLInputElement && active.closest(".cm-panels"))) return false;
      (direction === "redo" ? redo : undo)(view);
      return true;
    };
    requestAnimationFrame(() => {
      if (!view) return;
      const scroller = view.scrollDOM;
      scroller.scrollTop = fraction * Math.max(0, scroller.scrollHeight - scroller.clientHeight);
      view.focus();
    });
  } finally {
    busy = false;
  }
}

export async function exitSource(): Promise<void> {
  if (!view || busy) return;
  busy = true;
  try {
    clearTimeout(pushTimer);
    const fraction = scrollFraction(view);
    const body = await api.sourceExit(view.state.doc.toString());
    view.destroy();
    view = null;
    delete window.__vellumiSourceText;
    delete window.__vellumiSourceHistory;
    ui.sourceMode = false;
    editor()?.setMarkdown(body);
    editor()?.setScrollFraction(fraction);
  } finally {
    busy = false;
  }
}

export function toggleSource(): Promise<void> {
  return view ? exitSource() : enterSource();
}

/** Replaces the source text wholesale (revert / external reload). The undo
 *  history is reset: old entries would replay onto different text. */
export function replaceSourceText(text: string): void {
  if (!view || !host) return;
  view.destroy();
  view = createView(host, text);
}

export function sourceFind(kind: "open" | "next" | "previous"): boolean {
  if (!view) return false;
  if (kind === "open") openSearchPanel(view);
  else if (kind === "next") findNext(view);
  else findPrevious(view);
  return true;
}

/** Waits for any pending source edits to reach Rust. */
export function flushSource(): Promise<void> {
  return pushNow();
}
