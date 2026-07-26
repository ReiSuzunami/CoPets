use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Instant, timeout},
};
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::{Message, protocol::WebSocketConfig},
};

pub(super) const CDP_OPERATION_TIMEOUT: Duration = Duration::from_secs(3);
const CDP_MAX_HTTP_BYTES: usize = 1024 * 1024;
const CDP_MAX_WS_BYTES: usize = 1024 * 1024;
const CDP_MAX_PAGE_TARGETS: usize = 8;

#[derive(Clone)]
pub(super) struct PageTarget {
    websocket_url: String,
}

#[derive(Deserialize)]
struct DevToolsTarget {
    id: String,
    #[serde(rename = "type")]
    target_type: String,
    url: String,
}

fn bridge_error() -> String {
    "CoPets bridge could not verify Codex. Keep it open, then retry the bridge.".to_owned()
}

fn send_error() -> String {
    "Codex bridge could not send this follow-up. Retry in Codex or restart the bridge.".to_owned()
}

fn valid_target_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn remaining_until(deadline: Instant) -> Option<Duration> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    (!remaining.is_zero()).then_some(remaining)
}

async fn read_http_response(port: u16, path: &str, deadline: Instant) -> Result<Vec<u8>, String> {
    let request = match path {
        "/json/version" => {
            b"GET /json/version HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n".as_slice()
        }
        "/json/list" => {
            b"GET /json/list HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n".as_slice()
        }
        _ => return Err(bridge_error()),
    };
    let mut stream = timeout(
        remaining_until(deadline).ok_or_else(bridge_error)?,
        TcpStream::connect(("127.0.0.1", port)),
    )
    .await
    .map_err(|_| bridge_error())
    .and_then(|result| result.map_err(|_| bridge_error()))?;
    timeout(
        remaining_until(deadline).ok_or_else(bridge_error)?,
        stream.write_all(request),
    )
    .await
    .map_err(|_| bridge_error())
    .and_then(|result| result.map_err(|_| bridge_error()))?;

    let mut response = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        if let Some(expected_length) = complete_http_response_length(&response)?
            && response.len() >= expected_length
        {
            response.truncate(expected_length);
            return Ok(response);
        }
        let read = timeout(
            remaining_until(deadline).ok_or_else(bridge_error)?,
            stream.read(&mut chunk),
        )
        .await
        .map_err(|_| bridge_error())
        .and_then(|result| result.map_err(|_| bridge_error()))?;
        if read == 0 {
            break;
        }
        if response.len().saturating_add(read) > CDP_MAX_HTTP_BYTES {
            return Err(bridge_error());
        }
        response.extend_from_slice(&chunk[..read]);
    }
    Ok(response)
}

/// Returns the complete response byte length once HTTP headers reveal a
/// `Content-Length`. Chromium's DevTools endpoint can keep HTTP/1.1 sockets
/// open even when asked to close, so EOF is not a reliable body boundary.
fn complete_http_response_length(response: &[u8]) -> Result<Option<usize>, String> {
    let Some(separator) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Ok(None);
    };
    let header = std::str::from_utf8(&response[..separator]).map_err(|_| bridge_error())?;
    let mut content_length = None;
    for line in header.lines().skip(1) {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        if !name.eq_ignore_ascii_case("content-length") {
            continue;
        }
        if content_length.is_some() {
            return Err(bridge_error());
        }
        let value = value.trim().parse::<usize>().map_err(|_| bridge_error())?;
        content_length = Some(value);
    }
    let Some(content_length) = content_length else {
        return Ok(None);
    };
    let complete_length = separator
        .checked_add(4)
        .and_then(|value| value.checked_add(content_length))
        .filter(|value| *value <= CDP_MAX_HTTP_BYTES)
        .ok_or_else(bridge_error)?;
    Ok(Some(complete_length))
}

fn response_body(response: &[u8]) -> Result<&[u8], String> {
    let separator = response
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(bridge_error)?;
    let header = &response[..separator];
    if !header.starts_with(b"HTTP/1.1 200") && !header.starts_with(b"HTTP/1.0 200") {
        return Err(bridge_error());
    }
    Ok(&response[separator + 4..])
}

async fn version_ready(port: u16, deadline: Instant) -> Result<(), String> {
    let response = read_http_response(port, "/json/version", deadline).await?;
    let version: Value =
        serde_json::from_slice(response_body(&response)?).map_err(|_| bridge_error())?;
    version
        .get("Browser")
        .and_then(Value::as_str)
        .filter(|browser| !browser.is_empty())
        .map(|_| ())
        .ok_or_else(bridge_error)
}

fn is_codex_renderer_page(url: &str) -> bool {
    url == "app://-/index.html" || url.starts_with("app://-/index.html?")
}

fn renderer_page_targets(targets: Vec<DevToolsTarget>, port: u16) -> Vec<PageTarget> {
    let mut pages = targets
        .into_iter()
        .filter(|target| {
            target.target_type == "page"
                && valid_target_id(&target.id)
                && is_codex_renderer_page(&target.url)
        })
        .map(|target| {
            (
                target.url != "app://-/index.html",
                PageTarget {
                    // Ignore `webSocketDebuggerUrl` from the endpoint. CDP must
                    // stay on the verified loopback port, even if another local
                    // process tries to hand us an external endpoint.
                    websocket_url: format!("ws://127.0.0.1:{port}/devtools/page/{}", target.id),
                },
            )
        })
        .collect::<Vec<_>>();
    // The main Codex window first avoids letting the avatar overlay delay a
    // useful cold-start fingerprint. Keep a small bounded set even if an App
    // build unexpectedly exposes many renderer targets.
    pages.sort_by_key(|(is_secondary, _)| *is_secondary);
    pages
        .into_iter()
        .take(CDP_MAX_PAGE_TARGETS)
        .map(|(_, target)| target)
        .collect()
}

pub(super) async fn page_targets(port: u16, deadline: Instant) -> Result<Vec<PageTarget>, String> {
    version_ready(port, deadline).await?;
    let response = read_http_response(port, "/json/list", deadline).await?;
    let body = response_body(&response)?;
    let targets: Vec<DevToolsTarget> = serde_json::from_slice(body).map_err(|_| bridge_error())?;
    let pages = renderer_page_targets(targets, port);
    (!pages.is_empty())
        .then_some(pages)
        .ok_or_else(bridge_error)
}

fn cdp_exception_error(response: &Value) -> String {
    if response.get("error").is_some() || response.pointer("/result/exceptionDetails").is_some() {
        return send_error();
    }
    send_error()
}

pub(super) async fn evaluate(
    target: &PageTarget,
    expression: &str,
    deadline: Instant,
) -> Result<Value, String> {
    let mut config = WebSocketConfig::default();
    config.max_message_size = Some(CDP_MAX_WS_BYTES);
    config.max_frame_size = Some(CDP_MAX_WS_BYTES);
    let (mut socket, _) = timeout(
        remaining_until(deadline).ok_or_else(bridge_error)?,
        connect_async_with_config(target.websocket_url.as_str(), Some(config), false),
    )
    .await
    .map_err(|_| bridge_error())
    .and_then(|result| result.map_err(|_| bridge_error()))?;
    let request = json!({
        "id": 1,
        "method": "Runtime.evaluate",
        "params": {
            "expression": expression,
            "awaitPromise": true,
            "returnByValue": true,
            "userGesture": true,
        }
    });
    let serialized = serde_json::to_string(&request).map_err(|_| send_error())?;
    timeout(
        remaining_until(deadline).ok_or_else(send_error)?,
        socket.send(Message::Text(serialized.into())),
    )
    .await
    .map_err(|_| send_error())
    .and_then(|result| result.map_err(|_| send_error()))?;

    loop {
        let message = timeout(
            remaining_until(deadline).ok_or_else(send_error)?,
            socket.next(),
        )
        .await
        .map_err(|_| send_error())?
        .ok_or_else(send_error)
        .and_then(|result| result.map_err(|_| send_error()))?;
        let Message::Text(text) = message else {
            continue;
        };
        let response: Value = serde_json::from_str(&text).map_err(|_| send_error())?;
        if response.get("id").and_then(Value::as_u64) != Some(1) {
            continue;
        }
        if response.get("error").is_some() || response.pointer("/result/exceptionDetails").is_some()
        {
            return Err(cdp_exception_error(&response));
        }
        return Ok(response
            .pointer("/result/result/value")
            .cloned()
            .unwrap_or(Value::Null));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        time::sleep,
    };

    use super::{
        DevToolsTarget, Instant, complete_http_response_length, is_codex_renderer_page,
        read_http_response, remaining_until, renderer_page_targets, response_body, valid_target_id,
    };

    #[test]
    fn target_ids_reject_path_and_url_injection() {
        assert!(valid_target_id("A1_b-2.3"));
        assert!(!valid_target_id("../other"));
        assert!(!valid_target_id("page?next=http://elsewhere"));
        assert!(!valid_target_id(""));
    }

    #[test]
    fn cdp_response_requires_success_status_and_complete_headers() {
        assert_eq!(response_body(b"HTTP/1.1 200 OK\r\n\r\n[]").unwrap(), b"[]");
        assert!(response_body(b"HTTP/1.1 404 Nope\r\n\r\n[]").is_err());
        assert!(response_body(b"not http").is_err());
    }

    #[test]
    fn http_content_length_marks_a_response_complete_without_waiting_for_eof() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}trailing";
        assert_eq!(complete_http_response_length(response).unwrap(), Some(40));
        assert!(
            complete_http_response_length(b"HTTP/1.1 200 OK\r\n")
                .unwrap()
                .is_none()
        );
        assert!(
            complete_http_response_length(
                b"HTTP/1.1 200 OK\r\nContent-Length: not-a-number\r\n\r\n"
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn http_reader_returns_after_content_length_while_devtools_keeps_socket_open() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut chunk = [0_u8; 256];
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = socket.read(&mut chunk).await.unwrap();
                assert_ne!(read, 0, "client closed before sending a complete request");
                request.extend_from_slice(&chunk[..read]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}")
                .await
                .unwrap();
            sleep(Duration::from_millis(120)).await;
        });

        let deadline = Instant::now() + Duration::from_millis(80);
        assert_eq!(
            read_http_response(port, "/json/version", deadline)
                .await
                .unwrap(),
            b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"
        );
        server.await.unwrap();
    }

    #[test]
    fn cdp_operation_deadline_does_not_renew_after_an_expired_step() {
        let expired = Instant::now() - Duration::from_millis(1);
        assert!(remaining_until(expired).is_none());
    }

    #[test]
    fn renderer_target_selection_prefers_the_main_codex_window_and_rejects_other_pages() {
        assert!(is_codex_renderer_page("app://-/index.html"));
        assert!(is_codex_renderer_page(
            "app://-/index.html?initialRoute=%2Favatar-overlay"
        ));
        assert!(!is_codex_renderer_page("https://example.test/"));
        let targets = vec![
            DevToolsTarget {
                id: "overlay".to_owned(),
                target_type: "page".to_owned(),
                url: "app://-/index.html?initialRoute=%2Favatar-overlay".to_owned(),
            },
            DevToolsTarget {
                id: "main".to_owned(),
                target_type: "page".to_owned(),
                url: "app://-/index.html".to_owned(),
            },
            DevToolsTarget {
                id: "remote".to_owned(),
                target_type: "page".to_owned(),
                url: "https://example.test/".to_owned(),
            },
        ];
        let pages = renderer_page_targets(targets, 52_001);
        assert_eq!(pages.len(), 2);
        assert!(pages[0].websocket_url.ends_with("/main"));
        assert!(pages[1].websocket_url.ends_with("/overlay"));
    }
}
