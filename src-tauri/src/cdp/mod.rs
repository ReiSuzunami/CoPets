//! Narrow, loopback-only Chromium DevTools bridge for the official Codex App.
//!
//! This module deliberately owns no selection or lifecycle state. Callers pass a
//! native-selected `ControlTarget`-derived envelope only after the observer has
//! revalidated it.

mod client;
mod launch;
mod rf;

use serde::Serialize;
use serde_json::Value;
use std::time::Duration;
use tokio::time::{Instant, timeout};

pub(crate) use launch::{
    CdpEndpoint, codex_process_matches_port, launch_codex, official_codex_process_is_running,
    request_codex_restart, reserve_port, restartable_codex_process, running_codex_process,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ControlTransport {
    #[default]
    IpcOnly,
    CdpReady,
    CdpDegraded,
}

pub(crate) async fn verify_rf(port: u16, deadline: Instant) -> Result<(), String> {
    rf::verify_rf(port, deadline).await
}

pub(crate) async fn verify_tracked_listener(endpoint: CdpEndpoint) -> Result<(), String> {
    timeout(
        Duration::from_secs(3),
        tokio::task::spawn_blocking(move || {
            launch::codex_process_owns_port(endpoint.process_id, endpoint.port)
        }),
    )
    .await
    .map_err(|_| "CoPets could not verify ownership of the local Codex bridge.".to_owned())?
    .map_err(|_| "CoPets could not verify ownership of the local Codex bridge.".to_owned())?
}

pub(crate) async fn verify_tracked_process(endpoint: CdpEndpoint) -> Result<(), String> {
    timeout(
        Duration::from_secs(3),
        tokio::task::spawn_blocking(move || {
            codex_process_matches_port(endpoint.process_id, endpoint.port)
        }),
    )
    .await
    .map_err(|_| "CoPets could not verify the tracked Codex process.".to_owned())?
    .map_err(|_| "CoPets could not verify the tracked Codex process.".to_owned())?
}

pub(crate) async fn discover_existing_codex(port: Option<u16>) -> Result<CdpEndpoint, String> {
    timeout(
        Duration::from_secs(3),
        tokio::task::spawn_blocking(move || launch::discover_existing_codex(port)),
    )
    .await
    .map_err(|_| "CoPets could not inspect the local Codex CDP port. Retry.".to_owned())?
    .map_err(|_| "CoPets could not inspect the local Codex CDP port. Retry.".to_owned())?
}

pub(crate) async fn discover_launched_codex(
    port: u16,
    deadline: Instant,
) -> Result<CdpEndpoint, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err("CoPets could not discover the launched Codex process.".to_owned());
    }
    timeout(
        remaining,
        tokio::task::spawn_blocking(move || launch::discover_launched_codex(port)),
    )
    .await
    .map_err(|_| "CoPets could not discover the launched Codex process.".to_owned())?
    .map_err(|_| "CoPets could not discover the launched Codex process.".to_owned())?
}

pub(crate) async fn call_rf(port: u16, operation: &str, params: Value) -> Result<(), String> {
    rf::call_rf(port, operation, params).await
}

#[cfg(test)]
mod tests {
    use super::ControlTransport;

    #[test]
    fn ipc_is_default_transport() {
        assert_eq!(ControlTransport::default(), ControlTransport::IpcOnly);
    }
}
