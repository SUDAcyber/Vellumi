//! Export (Swift: MoltenExporter / image cards). The shell hands over the
//! editor's cleaned HTML plus the stylesheets it needs; Rust assembles the
//! page and writes files.
//!
//! PDF and print no longer snapshot the live editor (audit A10: CodeMirror
//! virtualizes long code blocks, focus mode dims text). The cleaned page is
//! rendered in its own window, served by the `vellumi-export` protocol, and
//! PDF'd or printed from there (decision D11).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Mutex, PoisonError};
use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::http::{header, HeaderValue, Request, Response, StatusCode};
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Emitter, Manager, UriSchemeContext, UriSchemeResponder, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Wry};
use tauri_plugin_dialog::DialogExt;
use vellumi_core::{assets, export, CoreError};

use crate::error::{AppError, AppResult};
use crate::state::{headless, AppState};
use crate::windows;

/// A rendered page the export protocol serves, with the document folder its
/// relative images resolve against.
#[derive(Debug, Clone)]
pub struct ExportPage {
    pub html: String,
    pub body_html: String,
    pub folder: Option<PathBuf>,
    pub base_name: String,
}

#[derive(Default)]
pub struct ExportState {
    pages: Mutex<HashMap<String, ExportPage>>,
    next: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn register(app: &AppHandle, page: ExportPage) -> String {
    let state = app.state::<ExportState>();
    let id = format!("x{}", state.next.fetch_add(1, Ordering::SeqCst));
    lock(&state.pages).insert(id.clone(), page);
    id
}

fn page_url(id: &str) -> Option<tauri::Url> {
    tauri::Url::parse(&format!("vellumi-export://localhost/{id}/page.html")).ok()
}

/// What the document window knows about itself.
fn doc_context(app: &AppHandle, window: &WebviewWindow) -> (String, Option<PathBuf>) {
    let state = app.state::<AppState>();
    let docs = state.docs();
    let Some(entry) = docs.get(window.label()) else {
        return ("Untitled".to_owned(), None);
    };
    let base = entry
        .session
        .path()
        .and_then(|p| p.file_stem())
        .map_or_else(|| windows::display_name(&entry.session), |s| s.to_string_lossy().into_owned());
    let folder = entry.session.path().and_then(|p| p.parent()).map(Path::to_path_buf);
    (base, folder)
}

fn build_page(app: &AppHandle, window: &WebviewWindow, body_html: &str, css: &str) -> ExportPage {
    let (base_name, folder) = doc_context(app, window);
    let body = export::expand_toc(body_html);
    let html = export::html_page(&base_name, css, &body, windows::app_language());
    ExportPage {
        html,
        body_html: body,
        folder,
        base_name,
    }
}

fn save_target(app: &AppHandle, page: &ExportPage, extension: &str, filter: &str) -> Option<PathBuf> {
    let name = format!("{}.{extension}", page.base_name);
    if headless() {
        // Self-test: next to the document, no dialog.
        return page.folder.as_ref().map(|folder| folder.join(format!("export-test.{extension}")));
    }
    let mut dialog = app.dialog().file().set_file_name(name).add_filter(filter, &[extension]);
    if let Some(folder) = &page.folder {
        dialog = dialog.set_directory(folder);
    }
    dialog.blocking_save_file().and_then(|picked| picked.into_path().ok())
}

/// File ▸ Export ▸ HTML. Returns the written path (None = cancelled).
#[tauri::command]
pub async fn export_html(app: AppHandle, window: WebviewWindow, body_html: String, css: String) -> AppResult<Option<String>> {
    let page = build_page(&app, &window, &body_html, &css);
    let Some(target) = save_target(&app, &page, "html", "HTML") else { return Ok(None) };
    vellumi_fs::write_atomic(&target, page.html.as_bytes())?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

fn render_window(app: &AppHandle, id: &str, visible: bool, title: &str) -> AppResult<(WebviewWindow, mpsc::Receiver<()>)> {
    let url = page_url(id).ok_or_else(|| AppError::Ipc("bad export url".into()))?;
    let (loaded_tx, loaded_rx) = mpsc::channel();
    let loaded_tx = Mutex::new(Some(loaded_tx));
    let window = WebviewWindowBuilder::new(app, format!("export-{id}"), WebviewUrl::CustomProtocol(url))
        .title(title)
        .inner_size(820.0, 1060.0)
        .visible(visible)
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled)
        .on_page_load(move |_, payload| {
            if payload.event() == PageLoadEvent::Finished {
                if let Some(tx) = lock(&loaded_tx).take() {
                    let _ = tx.send(());
                }
            }
        })
        .build()?;
    Ok((window, loaded_rx))
}

/// Waits (off the main thread) for the export window's page to finish
/// loading, then for fonts and images to settle.
async fn wait_loaded(loaded: mpsc::Receiver<()>) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || loaded.recv_timeout(Duration::from_secs(20)))
        .await
        .map_err(|error| AppError::Ipc(error.to_string()))?
        .map_err(|_| AppError::Ipc("export page did not load".into()))?;
    tokio_sleep(Duration::from_millis(400)).await;
    Ok(())
}

async fn tokio_sleep(duration: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(duration)).await;
}

/// File ▸ Export ▸ PDF: renders the cleaned page in a hidden window and asks
/// WebKit for a PDF of it.
#[tauri::command]
pub async fn export_pdf(app: AppHandle, window: WebviewWindow, body_html: String, css: String) -> AppResult<Option<String>> {
    let page = build_page(&app, &window, &body_html, &css);
    let Some(target) = save_target(&app, &page, "pdf", "PDF") else { return Ok(None) };
    let id = register(&app, page);
    let result = async {
        let (render, loaded) = render_window(&app, &id, false, "PDF")?;
        wait_loaded(loaded).await?;
        let bytes = create_pdf(&render).await;
        let _ = render.destroy();
        let bytes = bytes?;
        vellumi_fs::write_atomic(&target, &bytes)?;
        Ok::<_, AppError>(())
    }
    .await;
    lock(&app.state::<ExportState>().pages).remove(&id);
    result?;
    Ok(Some(target.to_string_lossy().into_owned()))
}

#[cfg(target_os = "macos")]
async fn create_pdf(window: &WebviewWindow) -> AppResult<Vec<u8>> {
    use objc2_foundation::{NSData, NSError};
    use objc2_web_kit::WKWebView;

    let (tx, rx) = mpsc::channel::<Result<Vec<u8>, String>>();
    let tx = std::sync::Arc::new(Mutex::new(Some(tx)));
    window.with_webview(move |webview| {
        let tx = tx.clone();
        let block = block2::RcBlock::new(move |data: *mut NSData, error: *mut NSError| {
            // SAFETY: WebKit passes either valid NSData or a valid NSError
            // (or null) for the duration of the callback.
            let result = unsafe {
                match (data.as_ref(), error.as_ref()) {
                    (Some(data), _) => Ok(data.to_vec()),
                    (None, Some(error)) => Err(error.localizedDescription().to_string()),
                    (None, None) => Err("WebKit returned no PDF data".to_owned()),
                }
            };
            if let Some(tx) = lock(&tx).take() {
                let _ = tx.send(result);
            }
        });
        // SAFETY: `inner()` is the live WKWebView of this window, used on the
        // main thread (with_webview runs there); a nil configuration means
        // "the whole page".
        unsafe {
            let webview: &WKWebView = &*webview.inner().cast::<WKWebView>();
            webview.createPDFWithConfiguration_completionHandler(None, &block);
        }
    })?;
    tauri::async_runtime::spawn_blocking(move || rx.recv_timeout(Duration::from_secs(60)))
        .await
        .map_err(|error| AppError::Ipc(error.to_string()))?
        .map_err(|_| AppError::Ipc("PDF rendering timed out".into()))?
        .map_err(AppError::Ipc)
}

#[cfg(not(target_os = "macos"))]
async fn create_pdf(_window: &WebviewWindow) -> AppResult<Vec<u8>> {
    Err(AppError::Ipc("PDF export is not available on this platform yet".into()))
}

/// File ▸ Print (⌘P): a preview window with the cleaned page, then the
/// system print panel (which also offers Save as PDF with pagination).
#[tauri::command]
pub async fn print_document(app: AppHandle, window: WebviewWindow, body_html: String, css: String) -> AppResult<()> {
    let page = build_page(&app, &window, &body_html, &css);
    let title = if windows::is_chinese() { format!("打印 – {}", page.base_name) } else { format!("Print – {}", page.base_name) };
    let id = register(&app, page);
    let (preview, loaded) = render_window(&app, &id, !headless(), &title)?;
    wait_loaded(loaded).await?;
    if !headless() {
        preview.print()?;
    }
    Ok(())
}

// ---- Image cards -----------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardSource {
    pub id: String,
    pub body_html: String,
    pub base_name: String,
}

/// Opens the image-card window for the document's cleaned HTML.
#[tauri::command]
pub async fn cards_open(app: AppHandle, window: WebviewWindow, body_html: String) -> AppResult<String> {
    let (base_name, folder) = doc_context(&app, &window);
    let id = register(
        &app,
        ExportPage {
            html: String::new(),
            body_html: String::new(),
            folder,
            base_name,
        },
    );
    // Images go through this export's own protocol path (needs the id).
    if let Some(page) = lock(&app.state::<ExportState>().pages).get_mut(&id) {
        page.body_html = rewrite_image_sources(&export::strip_toc_marker(&body_html), &id);
    }
    let title = if windows::is_chinese() { "导出为图片卡" } else { "Export Image Cards" };
    let url = format!("cards.html?id={id}");
    let builder = WebviewWindowBuilder::new(&app, format!("cards-{id}"), WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(1100.0, 800.0)
        .min_inner_size(860.0, 600.0)
        .initialization_script(windows::init_script())
        .background_throttling(tauri::utils::config::BackgroundThrottlingPolicy::Disabled);
    // The card renderer paginates on requestAnimationFrame, which never fires
    // in a hidden window: the self-test parks a visible window off-screen
    // instead (same trick as the 0.5.x card E2E test).
    let builder = if headless() {
        builder.position(-30000.0, -30000.0).focused(false).skip_taskbar(true)
    } else {
        builder
    };
    builder.build()?;
    Ok(id)
}

#[tauri::command]
pub async fn cards_source(app: AppHandle, id: String) -> AppResult<CardSource> {
    let state = app.state::<ExportState>();
    let pages = lock(&state.pages);
    let page = pages.get(&id).ok_or_else(|| AppError::Ipc(format!("no export {id}")))?;
    Ok(CardSource {
        id,
        body_html: page.body_html.clone(),
        base_name: page.base_name.clone(),
    })
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CardOutput {
    Single,
    Folder,
    Zip,
}

fn decode_data_url(data_url: &str) -> AppResult<Vec<u8>> {
    let (header, payload) = data_url
        .split_once(',')
        .ok_or_else(|| AppError::Ipc("card page is not a data URL".into()))?;
    if !header.starts_with("data:image/") || !header.contains(";base64") {
        return Err(AppError::Ipc("card page is not an image data URL".into()));
    }
    base64::engine::general_purpose::STANDARD
        .decode(payload)
        .map_err(|error| AppError::Ipc(error.to_string()))
}

/// Writes rendered card pages (data URLs, in order). Returns how many files
/// were written, 0 when cancelled.
#[tauri::command]
pub async fn cards_write(app: AppHandle, id: String, pages: Vec<String>, extension: String, output: CardOutput) -> AppResult<usize> {
    let (base, folder) = {
        let state = app.state::<ExportState>();
        let pages = lock(&state.pages);
        let page = pages.get(&id).ok_or_else(|| AppError::Ipc(format!("no export {id}")))?;
        (page.base_name.clone(), page.folder.clone())
    };
    let extension = if extension == "jpg" { "jpg" } else { "png" };
    let images = pages.iter().map(|page| decode_data_url(page)).collect::<AppResult<Vec<_>>>()?;
    let count = images.len();
    let names: Vec<String> = (0..count).map(|i| export::card_page_name(&base, i, count, extension)).collect();
    let pick = |name: String, ext: &str, filter: &str| -> Option<PathBuf> {
        if headless() {
            return folder.as_ref().map(|f| f.join(&name));
        }
        let mut dialog = app.dialog().file().set_file_name(name).add_filter(filter, &[ext]);
        if let Some(folder) = &folder {
            dialog = dialog.set_directory(folder);
        }
        dialog.blocking_save_file().and_then(|picked| picked.into_path().ok())
    };
    match output {
        CardOutput::Single if count == 1 => {
            let name = format!("{}.{extension}", export::card_base_name(&base));
            let Some(target) = pick(name, extension, "Image") else { return Ok(0) };
            vellumi_fs::write_atomic(&target, &images[0])?;
        }
        CardOutput::Zip => {
            let name = format!("{}.zip", export::card_base_name(&base));
            let Some(target) = pick(name, "zip", "ZIP") else { return Ok(0) };
            let files: Vec<(String, Vec<u8>)> = names.into_iter().zip(images).collect();
            let archive = export::zip_stored(&files).map_err(|error| AppError::Ipc(error.to_string()))?;
            vellumi_fs::write_atomic(&target, &archive)?;
        }
        CardOutput::Folder | CardOutput::Single => {
            let target = if headless() {
                folder.clone()
            } else {
                let mut dialog = app.dialog().file().set_can_create_directories(true);
                if let Some(folder) = &folder {
                    dialog = dialog.set_directory(folder);
                }
                dialog.blocking_pick_folder().and_then(|picked| picked.into_path().ok())
            };
            let Some(target) = target else { return Ok(0) };
            for (name, bytes) in names.iter().zip(&images) {
                vellumi_fs::write_atomic(&target.join(name), bytes)?;
            }
        }
    }
    if let Err(error) = app.emit("cards:written", count) {
        eprintln!("[vellumi] emit cards:written failed: {error}");
    }
    Ok(count)
}

/// Releases an export's in-memory page (the cards window closed).
#[tauri::command]
pub async fn export_release(app: AppHandle, id: String) -> AppResult<()> {
    lock(&app.state::<ExportState>().pages).remove(&id);
    Ok(())
}

/// Document-relative `<img src>` values → this export's protocol path, so
/// the card renderer (another window) can load them, with CORS for canvas.
fn rewrite_image_sources(html: &str, id: &str) -> String {
    #[allow(clippy::expect_used)]
    static SRC: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?i)(<img\b[^>]*?\bsrc=")([^"]+)(")"#).expect("valid img pattern")
    });
    SRC.replace_all(html, |caps: &regex::Captures<'_>| {
        let src = &caps[2];
        let lower = src.to_ascii_lowercase();
        let absolute = ["http://", "https://", "data:", "file:", "vellumi-"].iter().any(|p| lower.starts_with(p))
            || src.starts_with('/');
        if absolute {
            caps[0].to_owned()
        } else {
            format!("{}vellumi-export://localhost/{id}/{src}{}", &caps[1], &caps[3])
        }
    })
    .into_owned()
}

// ---- Protocol ----------------------------------------------------------------

fn respond(status: StatusCode, body: Vec<u8>, mime: &'static str) -> Response<Vec<u8>> {
    let mut response = Response::new(body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    // The card renderer draws these images into a canvas.
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    response
}

/// `vellumi-export://localhost/<id>/page.html` → the page;
/// `vellumi-export://localhost/<id>/<relative path>` → a file from the
/// document's folder (traversal refused).
pub fn protocol(context: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = context.app_handle().clone();
    let path = request.uri().path().trim_start_matches('/').to_owned();
    std::thread::spawn(move || {
        let (id, rest) = path.split_once('/').unwrap_or((path.as_str(), ""));
        let page = lock(&app.state::<ExportState>().pages).get(id).cloned();
        let Some(page) = page else {
            responder.respond(respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain"));
            return;
        };
        if rest == "page.html" {
            responder.respond(respond(StatusCode::OK, page.html.into_bytes(), "text/html; charset=utf-8"));
            return;
        }
        let Some(folder) = page.folder else {
            responder.respond(respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain"));
            return;
        };
        match assets::resolve_asset_request(rest, &folder) {
            Ok(file) => match vellumi_fs::read_limited(&file, 64 * 1024 * 1024) {
                Ok((bytes, _)) => responder.respond(respond(StatusCode::OK, bytes, assets::mime_type(&file))),
                Err(_) => responder.respond(respond(StatusCode::NOT_FOUND, Vec::new(), "text/plain")),
            },
            Err(CoreError::PathEscapesRoot { .. }) | Err(_) => {
                responder.respond(respond(StatusCode::FORBIDDEN, Vec::new(), "text/plain"));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_sources_are_rewritten_for_cards() {
        let html = r#"<p><img src="assets/a%20b.png" alt="x"><img src="https://e.com/x.png"><img src="/abs.png"></p>"#;
        let out = rewrite_image_sources(html, "x7");
        assert!(out.contains(r#"src="vellumi-export://localhost/x7/assets/a%20b.png""#));
        assert!(out.contains(r#"src="https://e.com/x.png""#));
        assert!(out.contains(r#"src="/abs.png""#));
    }

    #[test]
    fn data_urls_decode_and_reject_others() {
        assert_eq!(decode_data_url("data:image/png;base64,YWJj").ok(), Some(b"abc".to_vec()));
        assert!(decode_data_url("data:text/plain;base64,YWJj").is_err());
        assert!(decode_data_url("nope").is_err());
    }
}
