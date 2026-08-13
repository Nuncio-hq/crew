//! Localhost MJPEG multipart proxy. Production pulls frames from the sim-bridge
//! process; tests and bridge-missing serve a static placeholder JPEG.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use bytes::Bytes;
use futures_util::stream::unfold;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Minimal valid 1×1 JPEG (public domain).
const PLACEHOLDER_JPEG: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0x08, 0x06, 0x06, 0x07, 0x06, 0x05, 0x08,
    0x07, 0x07, 0x07, 0x09, 0x09, 0x08, 0x0A, 0x0C, 0x14, 0x0D, 0x0C, 0x0B, 0x0B, 0x0C, 0x19, 0x12,
    0x13, 0x0F, 0x14, 0x1D, 0x1A, 0x1F, 0x1E, 0x1D, 0x1A, 0x1C, 0x1C, 0x20, 0x24, 0x2E, 0x27, 0x20,
    0x22, 0x2C, 0x23, 0x1C, 0x1C, 0x28, 0x37, 0x29, 0x2C, 0x30, 0x31, 0x34, 0x34, 0x34, 0x1F, 0x27,
    0x39, 0x3D, 0x38, 0x32, 0x3C, 0x2E, 0x33, 0x34, 0x32, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x14, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xFF, 0xC4, 0x00, 0x14,
    0x10, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x7F, 0x3F, 0xFF, 0xD9,
];

#[derive(Clone, Default)]
pub struct FrameStore {
    inner: Arc<RwLock<std::collections::HashMap<String, Vec<u8>>>>,
}

impl FrameStore {
    pub async fn put(&self, udid: &str, jpeg: Vec<u8>) {
        self.inner.write().await.insert(udid.to_string(), jpeg);
    }

    pub async fn get(&self, udid: &str) -> Vec<u8> {
        self.inner
            .read()
            .await
            .get(udid)
            .cloned()
            .unwrap_or_else(|| PLACEHOLDER_JPEG.to_vec())
    }
}

pub fn placeholder_jpeg() -> &'static [u8] {
    PLACEHOLDER_JPEG
}

/// Build an MJPEG multipart body for one device. The loop ends when the
/// client drops — CPU goes to ~0 once the pane unsubscribes.
async fn mjpeg_stream(
    Path(udid): Path<String>,
    axum::extract::State(store): axum::extract::State<FrameStore>,
) -> Response {
    let stream = unfold(store, move |store| {
        let udid = udid.clone();
        async move {
            let jpeg = store.get(&udid).await;
            let mut buf = format!(
                "--crewframe\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n",
                jpeg.len()
            )
            .into_bytes();
            buf.extend_from_slice(&jpeg);
            buf.extend_from_slice(b"\r\n");
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            Some((Ok::<Bytes, Infallible>(Bytes::from(buf)), store))
        }
    });
    let mut response = Response::new(Body::from_stream(stream));
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("multipart/x-mixed-replace; boundary=crewframe"),
    );
    response
}

pub fn router(store: FrameStore) -> Router {
    Router::new()
        .route("/sim/{udid}/mjpeg", get(mjpeg_stream))
        .with_state(store)
}

pub async fn bind_local(store: FrameStore) -> Result<u16, String> {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("mjpeg bind: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let app = router(store);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_is_jpeg() {
        assert_eq!(&PLACEHOLDER_JPEG[0..2], &[0xFF, 0xD8]);
        assert_eq!(
            &PLACEHOLDER_JPEG[PLACEHOLDER_JPEG.len() - 2..],
            &[0xFF, 0xD9]
        );
    }
}
