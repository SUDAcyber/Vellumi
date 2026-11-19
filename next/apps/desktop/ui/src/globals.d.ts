// Globals injected by the Rust init script (src-tauri/shim.js, windows.rs).
export {};

declare global {
  interface Window {
    __vellumiSelftest?: boolean;
  }
}
