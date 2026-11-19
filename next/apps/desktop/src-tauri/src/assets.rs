//! Images next to the document: saving pastes into `assets/` and serving
//! them to the editor through the `vellumi-asset` protocol (Swift:
//! `MoltenDocument.saveImageAttachment` + `MoltenAssetSchemeHandler`).

use std::path::PathBuf;
use std::thread;

use base64::Engine;
use tauri::http::{header, HeaderValue, Request, Response, StatusCode};
use tauri::{AppHandle, Manager, UriSchemeContext, UriSchemeResponder, Wry};
use vellumi_core::{assets, CoreError, MAX_IMAGE_SIZE};

use crate::error::AppResult;
use crate::state::AppState;

/// Upper bound for a single served asset (videos are out of scope).
const MAX_SERVED_ASSET: u64 = 64 * 1024 * 1024;

fn document_folder(app: &AppHandle, label: &str) -> Option<PathBuf> {
    let state = app.state::<AppState>();
    let docs = state.docs();
    docs.get(label)?.session.path()?.parent().map(PathBuf::from)
}

/// Writes a pasted/dropped image and returns the Markdown-relative path.
pub fn save_image(app: &AppHandle, label: &str, name: &str, base64: &str) -> AppResult<String> {
    let folder = document_folder(app, label).ok_or(CoreError::Untitled)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64)
        .map_err(|error| CoreError::Io {
            op: "decode image",
            path: PathBuf::from(name),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, error),
        })?;
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    if size > MAX_IMAGE_SIZE || bytes.is_empty() {
        return Err(CoreError::ImageTooLarge {
            size,
            limit: MAX_IMAGE_SIZE,
        }
        .into());
    }
    let folder_name = assets::image_folder_name(&app.state::<AppState>().settings().image_folder);
    let assets_dir = folder.join(&folder_name);
    vellumi_fs::create_dir_all(&assets_dir)?;
    let target = assets::unique_asset_path(&assets_dir, name, |candidate| candidate.exists());
    vellumi_fs::write_new(&target, &bytes)?;
    let file_name = target
        .file_name()
        .map(|file| file.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(assets::document_relative_asset_path(&folder_name, &file_name))
}

fn status(code: StatusCode) -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = code;
    response
}

/// `vellumi-asset://localhost/<percent-encoded relative path>`: resolved
/// against the requesting window's document folder; traversal is refused.
pub fn protocol(context: UriSchemeContext<'_, Wry>, request: Request<Vec<u8>>, responder: UriSchemeResponder) {
    let app = context.app_handle().clone();
    let label = context.webview_label().to_owned();
    let requested = request.uri().path().to_owned();
    thread::spawn(move || {
        let Some(folder) = document_folder(&app, &label) else {
            responder.respond(status(StatusCode::NOT_FOUND));
            return;
        };
        let Ok(path) = assets::resolve_asset_request(&requested, &folder) else {
            responder.respond(status(StatusCode::FORBIDDEN));
            return;
        };
        match vellumi_fs::read_limited(&path, MAX_SERVED_ASSET) {
            Ok((bytes, _)) => {
                let mut response = Response::new(bytes);
                response
                    .headers_mut()
                    .insert(header::CONTENT_TYPE, HeaderValue::from_static(assets::mime_type(&path)));
                responder.respond(response);
            }
            Err(_) => responder.respond(status(StatusCode::NOT_FOUND)),
        }
    });
}
