// Injected at document start into every document window, before editor.js.
// It replaces the WKWebView message handler the Swift shell used with Tauri
// IPC, keeping the editor's bridge contract (see packages/editor/src/main.js)
// unchanged.
(() => {
  const invoke = (command, args) => window.__TAURI_INTERNALS__.invoke(command, args);

  // Uncaught errors reach the app log (stderr), so failures in a hidden or
  // remote window are never silent.
  const report = (level, message) =>
    invoke("ui_log", { level, message: String(message).slice(0, 4000) }).catch(() => {});
  window.addEventListener("error", (event) => report("error", event.error?.stack ?? event.message));
  window.addEventListener("unhandledrejection", (event) => report("error", event.reason?.stack ?? event.reason));
  window.__vellumiLog = report;

  // Document-relative images are served by the Rust `vellumi-asset` protocol.
  window.__vellumiAssetBase = "vellumi-asset://localhost/";

  // JS → Rust: ready / change / normalized / boot-error / image-save.
  window.__vellumiPost = (message) => {
    invoke("bridge_message", { message }).catch((error) => console.error("bridge_message failed", error));
  };

  // Rust → JS → Rust: Rust never waits on JS synchronously. When it needs
  // the freshest serialization (save, close, quit) it calls this, and the
  // editor answers with doc_flush. null markdown = no live editor; Rust then
  // keeps the text it already has.
  window.__vellumiFlush = (reason) => {
    let markdown = null;
    let source = null;
    try {
      // Source mode (the shell's CodeMirror view) is authoritative while on.
      source = window.__vellumiSourceText?.() ?? null;
      if (source === null) markdown = window.moltenAPI?.getMarkdown?.() ?? null;
    } catch (error) {
      console.error("reading the editor failed", error);
    }
    return invoke("doc_flush", { reason, markdown, source }).catch((error) => console.error("doc_flush failed", error));
  };

  // Edit ▸ Undo/Redo arrive as menu events. Route them like AppKit's
  // responder chain would: the source view and plain inputs undo themselves,
  // everything else goes to the editor's ProseMirror history.
  window.__vellumiHistory = (direction) => {
    if (window.__vellumiSourceHistory?.(direction)) return true;
    const active = document.activeElement;
    const inPlainInput =
      active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA") && !active.closest(".ProseMirror");
    if (inPlainInput) {
      document.execCommand(direction);
    } else if (direction === "redo") {
      window.moltenAPI?.redo();
    } else {
      window.moltenAPI?.undo();
    }
  };
})();
