use std::{future::Future, pin::Pin};

use futures_util::{StreamExt, stream::FuturesUnordered};
use serde_json::Value;
use tokio::time::Instant;

use super::client::{CDP_OPERATION_TIMEOUT, PageTarget, evaluate, page_targets};

pub(super) const RF_SOURCE: &str = "function Rf(e,t){return _Ze.sendRequest(e,t)}";
pub(super) const EMPTY_PROMPT_ERROR: &str = "Cannot send an empty follow-up message.";
const SENTINEL_CONVERSATION_ID: &str = "00000000-0000-4000-8000-000000000000";
const MISSING_MANAGER_ERROR: &str =
    "No AppServerManager registered for conversationId: 00000000-0000-4000-8000-000000000000";
const FINGERPRINT_ERROR: &str = "CoPets CDP fingerprint failed";

type FingerprintProbe = Pin<Box<dyn Future<Output = Result<(), String>> + Send>>;

fn rf_resolver_script() -> String {
    let source = serde_json::to_string(RF_SOURCE).expect("fixed source serializes");
    format!(
        r#"
        const candidates = Array.from(document.querySelectorAll('link[rel="modulepreload"],script[src]'))
          .map((element) => element.href || element.src)
          .filter(Boolean);
        const initial = candidates.find((candidate) => /\/app-initial-[^\/?]+\.js(?:\?.*)?$/.test(candidate));
        if (!initial) throw new Error('{FINGERPRINT_ERROR}');
        const module = await import(initial);
        const expected = {source};
        let Rf;
        for (const key of Object.keys(module)) {{
          try {{
            const value = module[key];
            if (typeof value === 'function' && Function.prototype.toString.call(value) === expected) {{
              Rf = value;
              break;
            }}
          }} catch {{
            // Some live ESM exports throw when read. They are not candidates.
          }}
        }}
        if (!Rf) throw new Error('{FINGERPRINT_ERROR}');
        return Rf;
        "#
    )
}

fn fingerprint_script() -> String {
    let empty_error = serde_json::to_string(EMPTY_PROMPT_ERROR).expect("fixed error serializes");
    let missing_manager_error =
        serde_json::to_string(MISSING_MANAGER_ERROR).expect("fixed error serializes");
    let sentinel_conversation_id =
        serde_json::to_string(SENTINEL_CONVERSATION_ID).expect("fixed id serializes");
    format!(
        r#"
        (async () => {{
          if (!window.electronBridge || typeof window.electronBridge.getBuildFlavor !== 'function') {{
            throw new Error('{FINGERPRINT_ERROR}');
          }}
          await window.electronBridge.getBuildFlavor();
          const Rf = await (async () => {{ {resolver} }})();
          try {{
            await Rf('send-follow-up-message', {{
              conversationId: {sentinel_conversation_id},
              prompt: '',
              serviceTier: null,
            }});
          }} catch (error) {{
            const message = String((error && error.message) || error);
            if (message.includes({empty_error})) return {{ ok: true, probe: 'empty-prompt' }};
            if (message.includes({missing_manager_error})) return {{ ok: true, probe: 'missing-manager' }};
            throw new Error('{FINGERPRINT_ERROR}');
          }}
          throw new Error('{FINGERPRINT_ERROR}');
        }})()
        "#,
        resolver = rf_resolver_script(),
    )
}

fn call_script(operation: &str, params: &Value) -> Result<String, String> {
    if !matches!(operation, "send-follow-up-message" | "steer-turn-for-host") {
        return Err("Codex bridge rejected an unknown follow-up operation.".to_owned());
    }
    let operation = serde_json::to_string(operation).map_err(|_| {
        "Codex bridge could not send this follow-up. Retry in Codex or restart the bridge."
            .to_owned()
    })?;
    let params = serde_json::to_string(params).map_err(|_| {
        "Codex bridge could not send this follow-up. Retry in Codex or restart the bridge."
            .to_owned()
    })?;
    Ok(format!(
        r#"
        (async () => {{
          const Rf = await (async () => {{ {resolver} }})();
          const result = await Rf({operation}, {params});
          return {{ ok: true, result: result === undefined ? null : result }};
        }})()
        "#,
        resolver = rf_resolver_script(),
    ))
}

fn valid_fingerprint_result(value: &Value) -> bool {
    value.get("ok").and_then(Value::as_bool) == Some(true)
        && matches!(
            value.get("probe").and_then(Value::as_str),
            Some("empty-prompt" | "missing-manager")
        )
}

fn fingerprint_probe(target: PageTarget, script: String, deadline: Instant) -> FingerprintProbe {
    Box::pin(async move {
        let value = evaluate(&target, &script, deadline).await?;
        valid_fingerprint_result(&value)
            .then_some(())
            .ok_or_else(|| FINGERPRINT_ERROR.to_owned())
    })
}

async fn first_fingerprint_probe_succeeds(mut probes: FuturesUnordered<FingerprintProbe>) -> bool {
    while let Some(result) = probes.next().await {
        if result.is_ok() {
            return true;
        }
    }
    false
}

pub(super) async fn verify_rf(port: u16, deadline: Instant) -> Result<(), String> {
    let script = fingerprint_script();
    let probes = page_targets(port, deadline)
        .await?
        .into_iter()
        .map(|target| fingerprint_probe(target, script.clone(), deadline))
        .collect();
    first_fingerprint_probe_succeeds(probes)
        .await
        .then_some(())
        .ok_or_else(|| {
            "CoPets bridge could not verify Codex. Keep it open, then retry the bridge.".to_owned()
        })
}

pub(super) async fn call_rf(port: u16, operation: &str, params: Value) -> Result<(), String> {
    let fingerprint = fingerprint_script();
    let call = call_script(operation, &params)?;
    let deadline = Instant::now() + CDP_OPERATION_TIMEOUT;
    for target in page_targets(port, deadline).await? {
        let Ok(value) = evaluate(&target, &fingerprint, deadline).await else {
            continue;
        };
        if !valid_fingerprint_result(&value) {
            continue;
        }
        return evaluate(&target, &call, deadline).await.map(|_| ());
    }
    Err("CoPets bridge could not verify Codex. Keep it open, then retry the bridge.".to_owned())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::stream::FuturesUnordered;
    use serde_json::json;
    use tokio::time::sleep;

    use super::{
        EMPTY_PROMPT_ERROR, FingerprintProbe, MISSING_MANAGER_ERROR, RF_SOURCE,
        SENTINEL_CONVERSATION_ID, call_script, fingerprint_script,
        first_fingerprint_probe_succeeds, valid_fingerprint_result,
    };

    #[test]
    fn fingerprint_uses_exact_rf_source_and_only_recognized_no_content_gates() {
        let script = fingerprint_script();
        assert!(script.contains(RF_SOURCE));
        assert!(script.contains(EMPTY_PROMPT_ERROR));
        assert!(script.contains(MISSING_MANAGER_ERROR));
        assert!(script.contains(SENTINEL_CONVERSATION_ID));
        assert!(script.contains("getBuildFlavor"));
        assert!(script.contains("Object.keys(module)"));
        assert!(!script.contains("Object.values(module)"));
        assert!(!script.contains("sendMessageFromView"));
        assert!(!script.contains("__copetsRf"));
    }

    #[test]
    fn send_script_serializes_native_params_without_global_cache() {
        let script = call_script(
            "send-follow-up-message",
            &json!({ "conversationId": "native-only", "prompt": "hello" }),
        )
        .unwrap();
        assert!(script.contains("send-follow-up-message"));
        assert!(script.contains("native-only"));
        assert!(!script.contains("sendMessageFromView"));
        assert!(!script.contains("__copetsRf"));
    }

    #[test]
    fn unknown_operation_is_rejected_before_evaluate() {
        assert!(call_script("thread/resume", &json!({})).is_err());
    }

    #[test]
    fn fingerprint_result_accepts_only_the_two_recognized_probe_outcomes() {
        assert!(valid_fingerprint_result(
            &json!({ "ok": true, "probe": "empty-prompt" })
        ));
        assert!(valid_fingerprint_result(
            &json!({ "ok": true, "probe": "missing-manager" })
        ));
        assert!(!valid_fingerprint_result(
            &json!({ "ok": true, "probe": "anything-else" })
        ));
        assert!(!valid_fingerprint_result(
            &json!({ "ok": false, "probe": "empty-prompt" })
        ));
    }

    #[tokio::test]
    async fn concurrent_fingerprint_probes_do_not_wait_for_a_stalled_page() {
        let delayed_failure: FingerprintProbe = Box::pin(async {
            sleep(Duration::from_millis(40)).await;
            Err("not ready".to_owned())
        });
        let ready_success: FingerprintProbe = Box::pin(async { Ok(()) });
        let probes = FuturesUnordered::from_iter([delayed_failure, ready_success]);
        assert!(first_fingerprint_probe_succeeds(probes).await);
    }
}
