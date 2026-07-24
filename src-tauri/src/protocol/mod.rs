use crate::session::{PreviewBlob, SessionManager};
use tauri::http::{header, Method, Request, Response, StatusCode};

pub fn respond(manager: &SessionManager, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let token = request.uri().path().trim_start_matches('/');
    match manager.preview(token) {
        Some(blob) => preview_response(&blob, request),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(b"preview not found".to_vec())
            .expect("valid not-found response"),
    }
}

fn preview_response(blob: &PreviewBlob, request: &Request<Vec<u8>>) -> Response<Vec<u8>> {
    let full_len = blob.bytes.len();
    let requested_range = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok());

    let range = match requested_range {
        Some(value) => match parse_range(value, full_len) {
            Some(range) => Some(range),
            None => {
                return Response::builder()
                    .status(StatusCode::RANGE_NOT_SATISFIABLE)
                    .header(header::CONTENT_RANGE, format!("bytes */{full_len}"))
                    .header(header::ACCEPT_RANGES, "bytes")
                    .body(Vec::new())
                    .expect("valid range-not-satisfiable response")
            }
        },
        None => None,
    };

    let (status, start, end) = match range {
        Some((start, end)) => (StatusCode::PARTIAL_CONTENT, start, end),
        None => (StatusCode::OK, 0, full_len),
    };
    let content_len = end.saturating_sub(start);
    let mut response = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, blob.mime)
        .header(header::CONTENT_LENGTH, content_len.to_string())
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CACHE_CONTROL, "private, max-age=300")
        .header("X-Content-Type-Options", "nosniff");
    if status == StatusCode::PARTIAL_CONTENT {
        response = response.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{full_len}", end - 1),
        );
    }

    let body = if request.method() == Method::HEAD {
        Vec::new()
    } else {
        blob.bytes[start..end].to_vec()
    };
    response.body(body).expect("valid preview response")
}

fn parse_range(value: &str, full_len: usize) -> Option<(usize, usize)> {
    let value = value.strip_prefix("bytes=")?;
    if full_len == 0 || value.contains(',') {
        return None;
    }
    let (start, end) = value.split_once('-')?;
    if start.is_empty() {
        let suffix_len = end.parse::<usize>().ok()?;
        if suffix_len == 0 {
            return None;
        }
        return Some((full_len.saturating_sub(suffix_len), full_len));
    }

    let start = start.parse::<usize>().ok()?;
    if start >= full_len {
        return None;
    }
    let end = if end.is_empty() {
        full_len
    } else {
        end.parse::<usize>().ok()?.saturating_add(1).min(full_len)
    };
    (end > start).then_some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blob() -> PreviewBlob {
        PreviewBlob {
            mime: "audio/wav",
            bytes: (0..10).collect(),
        }
    }

    fn request(range: Option<&str>) -> Request<Vec<u8>> {
        let mut builder = Request::builder().uri("nge2-preview://localhost/token");
        if let Some(range) = range {
            builder = builder.header(header::RANGE, range);
        }
        builder.body(Vec::new()).unwrap()
    }

    #[test]
    fn returns_complete_preview() {
        let response = preview_response(&blob(), &request(None));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_LENGTH], "10");
        assert_eq!(response.body(), &(0..10).collect::<Vec<_>>());
    }

    #[test]
    fn serves_bounded_and_suffix_ranges() {
        let response = preview_response(&blob(), &request(Some("bytes=2-5")));
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
        assert_eq!(response.body(), &vec![2, 3, 4, 5]);

        let response = preview_response(&blob(), &request(Some("bytes=-3")));
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes 7-9/10");
        assert_eq!(response.body(), &vec![7, 8, 9]);
    }

    #[test]
    fn rejects_unsatisfiable_ranges() {
        let response = preview_response(&blob(), &request(Some("bytes=10-20")));
        assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */10");
    }
}
