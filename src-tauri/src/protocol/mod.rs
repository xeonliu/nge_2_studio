use crate::session::SessionManager;
use tauri::http::{header, Request, Response, StatusCode};

pub fn respond(
    manager: &SessionManager,
    request: &Request<Vec<u8>>,
) -> Response<Vec<u8>> {
    let token = request.uri().path().trim_start_matches('/');
    match manager.preview(token) {
        Some(blob) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, blob.mime)
            .header(header::CACHE_CONTROL, "private, max-age=300")
            .header("X-Content-Type-Options", "nosniff")
            .body(blob.bytes.clone())
            .expect("valid preview response"),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(b"preview not found".to_vec())
            .expect("valid not-found response"),
    }
}

