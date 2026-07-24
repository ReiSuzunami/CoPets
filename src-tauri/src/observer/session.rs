use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::Duration,
};

use regex::Regex;
use serde_json::Value;
use tauri::AppHandle;
use tokio::time::sleep;

use crate::file_tail::AppendCursor;
use crate::local_trust::owned_regular_metadata;

use super::{
    RuntimeEvent, RuntimeHandle, SessionContextEvent, apply_runtime_event, codex_home,
    compact_tail_text, compact_text, hash_id, recent_files,
};

const INITIAL_SESSION_TAIL: u64 = 2 * 1024 * 1024;
const QUESTION_LIMIT: usize = 240;
const UPDATE_LIMIT: usize = 180;

fn session_state(record: &Value) -> Option<&'static str> {
    let kind = record.get("type")?.as_str()?;
    let payload = record.get("payload")?;
    let signal = payload.get("type")?.as_str()?;
    if kind == "event_msg" {
        return match signal {
            "agent_reasoning" | "task_started" | "exited_review_mode" => Some("working"),
            "task_complete" => Some("completed"),
            "turn_aborted" => Some("interrupted"),
            "error" => Some("failed"),
            "guardian_assessment" => Some("working"),
            "entered_review_mode" => Some("reviewing"),
            _ => None,
        };
    }
    if kind == "response_item" {
        let operation = payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if signal == "function_call" && operation == "request_user_input" {
            return Some("working");
        }
        if signal == "function_call"
            && (operation.contains("approval") || operation.contains("permission"))
        {
            return Some("working");
        }
        if matches!(signal, "reasoning" | "function_call" | "custom_tool_call") {
            return Some("working");
        }
    }
    None
}

fn session_context_event(record: &Value) -> Option<SessionContextEvent> {
    let kind = record.get("type")?.as_str()?;
    let payload = record.get("payload")?;
    let signal = payload.get("type")?.as_str()?;
    if kind == "event_msg" {
        return match signal {
            "user_message" => compact_text(payload.get("message")?.as_str()?, QUESTION_LIMIT)
                .map(SessionContextEvent::UserQuestion),
            "task_started" => Some(SessionContextEvent::ResponseStarted),
            "agent_message" => compact_tail_text(payload.get("message")?.as_str()?, UPDATE_LIMIT)
                .map(SessionContextEvent::AssistantUpdate),
            _ => None,
        };
    }
    if kind == "response_item"
        && signal == "message"
        && payload.get("role").and_then(Value::as_str) == Some("user")
    {
        let text = payload
            .get("content")?
            .as_array()?
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some("input_text"))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        return compact_text(&text, QUESTION_LIMIT).map(SessionContextEvent::UserQuestion);
    }
    None
}

fn thread_from_rollout(path: &Path) -> Option<String> {
    static UUID: OnceLock<Regex> = OnceLock::new();
    let regex = UUID.get_or_init(|| {
        Regex::new(r"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\.jsonl$")
            .unwrap()
    });
    regex
        .captures(path.file_name()?.to_str()?)
        .and_then(|caps| caps.get(1))
        .map(|id| hash_id(id.as_str()))
}

struct SessionAdapter {
    root: PathBuf,
    cursors: HashMap<PathBuf, AppendCursor>,
    initialized: bool,
}

impl SessionAdapter {
    fn new(root: PathBuf) -> Self {
        Self {
            root,
            cursors: HashMap::new(),
            initialized: false,
        }
    }

    fn poll(&mut self) -> Vec<RuntimeEvent> {
        let mut events = Vec::new();
        for path in recent_files(&self.root, "jsonl") {
            let size = owned_regular_metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            let cursor = self.cursors.entry(path.clone()).or_insert_with(|| {
                AppendCursor::new(if self.initialized {
                    0
                } else {
                    size.saturating_sub(INITIAL_SESSION_TAIL)
                })
            });
            let Some(thread) = thread_from_rollout(&path) else {
                continue;
            };
            let Ok(lines) = cursor.read_appended(&path) else {
                continue;
            };
            for line in lines {
                let Ok(record) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                let context = session_context_event(&record);
                let state = session_state(&record);
                if context.is_some() || state.is_some() {
                    events.push(RuntimeEvent::Session {
                        thread: thread.clone(),
                        context,
                        state,
                    });
                }
            }
        }
        self.initialized = true;
        events
    }
}

pub(super) async fn run(app: AppHandle, runtime: RuntimeHandle) {
    let Some(root) = codex_home().map(|home| home.join("sessions")) else {
        return;
    };
    let mut adapter = SessionAdapter::new(root);
    loop {
        let Ok((next_adapter, events)) = tauri::async_runtime::spawn_blocking(move || {
            let events = adapter.poll();
            (adapter, events)
        })
        .await
        else {
            return;
        };
        adapter = next_adapter;
        for event in events {
            apply_runtime_event(&app, &runtime, event, "codex-session-jsonl");
        }
        sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{session_context_event, session_state};
    use crate::observer::runtime::SessionContextEvent;
    use serde_json::json;

    #[test]
    fn maps_session_states_without_content() {
        for (signal, expected) in [
            ("task_started", Some("working")),
            ("entered_review_mode", Some("reviewing")),
            ("exited_review_mode", Some("working")),
            ("task_complete", Some("completed")),
            ("turn_aborted", Some("interrupted")),
            ("error", Some("failed")),
            ("unknown", None),
        ] {
            assert_eq!(
                session_state(&json!({
                    "type":"event_msg",
                    "payload":{"type":signal,"message":"secret"}
                })),
                expected,
                "unexpected mapping for {signal}"
            );
        }
        assert_eq!(
            session_state(
                &json!({"type":"response_item","payload":{"type":"function_call","name":"request_user_input"}})
            ),
            Some("working")
        );
    }

    #[test]
    fn extracts_only_user_visible_task_context() {
        assert_eq!(
            session_context_event(&json!({
                "type":"event_msg",
                "payload":{"type":"user_message","message":"  Please   repair the pet  "}
            })),
            Some(SessionContextEvent::UserQuestion(
                "Please repair the pet".into()
            ))
        );
        assert_eq!(
            session_context_event(&json!({
                "type":"event_msg",
                "payload":{"type":"agent_message","message":"Inspecting the runtime state"}
            })),
            Some(SessionContextEvent::AssistantUpdate(
                "Inspecting the runtime state".into()
            ))
        );
        assert_eq!(
            session_context_event(&json!({
                "type":"event_msg",
                "payload":{"type":"agent_reasoning","text":"private reasoning"}
            })),
            None
        );
        assert_eq!(
            session_context_event(&json!({
                "type":"response_item",
                "payload":{"type":"custom_tool_call","name":"exec","input":"secret command"}
            })),
            None
        );
    }
}
