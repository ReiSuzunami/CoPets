use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::cdp::ControlTransport;

const SUMMARY_LIMIT: usize = 180;

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlSnapshot {
    pub can_stop: bool,
    pub can_reply: bool,
    pub can_start_follow_up: bool,
    pub show_working_follow_up: bool,
    pub show_ready_follow_up: bool,
    pub transport: ControlTransport,
    pub notifications: Vec<ControlNotification>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlNotification {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    pub questions: Vec<ControlQuestion>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlQuestion {
    pub id: String,
    pub header: String,
    pub prompt: String,
    pub options: Vec<ControlOption>,
    pub allow_other: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlOption {
    pub id: String,
    pub label: String,
    pub description: String,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ControlTarget {
    pub conversation_id: String,
    pub owner_client_id: String,
    pub host_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Clone)]
pub(crate) struct PendingControl {
    pub id: String,
    pub conversation_id: String,
    pub owner_client_id: String,
    pub request_id: String,
    pub kind: PendingKind,
}

#[derive(Clone)]
pub(crate) enum PendingKind {
    Command,
    FileChange,
    Permission {
        permissions: Value,
    },
    Question {
        question_ids: HashMap<String, String>,
        option_ids: HashMap<String, String>,
    },
    McpElicitation,
}

pub(crate) struct ParsedControls {
    pub notifications: Vec<ControlNotification>,
    pub pending: HashMap<String, PendingControl>,
}

fn opaque_id(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update(part.as_bytes());
        digest.update([0]);
    }
    digest
        .finalize()
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn compact(value: &str) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= SUMMARY_LIMIT {
        return normalized;
    }
    let mut shortened = normalized
        .chars()
        .take(SUMMARY_LIMIT - 1)
        .collect::<String>();
    shortened.push('…');
    shortened
}

fn compact_action_text(value: &str) -> String {
    let mut redact_next = false;
    let words = value
        .split_whitespace()
        .map(|word| {
            if redact_next {
                redact_next = false;
                return "[redacted]".to_owned();
            }
            let trimmed = word.trim_matches(|character: char| {
                matches!(
                    character,
                    '\'' | '"' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';'
                )
            });
            let lower = trimmed.to_ascii_lowercase();
            if matches!(
                lower.as_str(),
                "--token" | "--password" | "--secret" | "--api-key" | "--apikey"
            ) {
                redact_next = true;
                return trimmed.to_owned();
            }
            if ["token=", "password=", "secret=", "api_key=", "apikey="]
                .iter()
                .any(|marker| lower.contains(marker))
            {
                return "[redacted]".to_owned();
            }
            let bytes = trimmed.as_bytes();
            let is_absolute_path = trimmed.starts_with('/')
                || trimmed.starts_with("~/")
                || (bytes.len() >= 3 && bytes[1] == b':' && matches!(bytes[2], b'/' | b'\\'));
            if is_absolute_path {
                let basename = trimmed
                    .trim_end_matches(['/', '\\'])
                    .rsplit(['/', '\\'])
                    .next()
                    .filter(|name| {
                        !name.is_empty()
                            && name.chars().all(|character| {
                                character.is_ascii_alphanumeric() || "._+-".contains(character)
                            })
                    });
                return basename
                    .map(|name| format!("…/{name}"))
                    .unwrap_or_else(|| "[path]".into());
            }
            word.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ");
    compact(&words)
}

fn text_at<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .filter(|text| !text.trim().is_empty())
}

fn executable_name(command: &str) -> Option<String> {
    let executable = command
        .split_whitespace()
        .find(|token| !token.contains('=') && *token != "env")?
        .trim_matches(['\'', '"']);
    let basename = executable.rsplit(['/', '\\']).next()?.trim();
    (!basename.is_empty()
        && basename
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._+-".contains(character)))
    .then(|| basename.to_owned())
}

fn command_summary(params: &Value) -> String {
    let command = text_at(params, &["command"]).or_else(|| {
        params
            .get("commandActions")
            .and_then(Value::as_array)
            .and_then(|actions| actions.first())
            .and_then(|action| text_at(action, &["cmd", "command", "name"]))
    });
    command
        .and_then(executable_name)
        .map(|executable| format!("Run {executable}"))
        .unwrap_or_else(|| "Codex wants to run a command.".into())
}

fn network_summary(params: &Value) -> String {
    let target = params
        .get("networkApprovalContext")
        .and_then(|context| text_at(context, &["host", "domain"]))
        .filter(|target| {
            target.len() <= 253
                && target.chars().all(|character| {
                    character.is_ascii_alphanumeric() || ".:-_[]".contains(character)
                })
        });
    target
        .map(|target| format!("Connect to {target}"))
        .unwrap_or_else(|| "Codex wants network access.".into())
}

fn file_summary(_params: &Value) -> String {
    "Codex wants to apply file changes.".into()
}

fn permission_summary(params: &Value) -> String {
    let names = params
        .get("permissions")
        .and_then(Value::as_object)
        .map(|permissions| {
            permissions
                .keys()
                .map(|key| key.replace(['_', '-'], " "))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .filter(|value| !value.is_empty());
    names.unwrap_or_else(|| "Codex is requesting additional permissions.".into())
}

fn parse_questions(
    conversation_id: &str,
    request_id: &str,
    params: &Value,
) -> (
    Vec<ControlQuestion>,
    HashMap<String, String>,
    HashMap<String, String>,
) {
    let mut ids = HashMap::new();
    let mut option_ids = HashMap::new();
    let questions = params
        .get("questions")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|question| {
            let raw_id = question.get("id")?.as_str()?;
            let id = opaque_id(&[conversation_id, request_id, raw_id]);
            ids.insert(id.clone(), raw_id.to_owned());
            let options = question
                .get("options")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
                .filter_map(|(index, option)| {
                    let raw_label = option.get("label")?.as_str()?;
                    let option_id =
                        opaque_id(&[conversation_id, request_id, raw_id, &index.to_string()]);
                    option_ids.insert(option_id.clone(), raw_label.to_owned());
                    Some(ControlOption {
                        id: option_id,
                        label: compact_action_text(raw_label),
                        description: option
                            .get("description")
                            .and_then(Value::as_str)
                            .map(compact_action_text)
                            .unwrap_or_default(),
                    })
                })
                .collect();
            Some(ControlQuestion {
                id,
                header: question
                    .get("header")
                    .and_then(Value::as_str)
                    .map(compact_action_text)
                    .unwrap_or_else(|| "Codex needs input".into()),
                prompt: question
                    .get("question")
                    .and_then(Value::as_str)
                    .map(compact_action_text)
                    .unwrap_or_else(|| "Choose a response.".into()),
                options,
                allow_other: question
                    .get("isOther")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect();
    (questions, ids, option_ids)
}

pub(crate) fn parse_controls(
    conversation_id: &str,
    owner_client_id: &str,
    state: &Value,
) -> ParsedControls {
    let mut notifications = Vec::new();
    let mut pending = HashMap::new();
    let requests = state
        .get("requests")
        .and_then(Value::as_array)
        .into_iter()
        .flatten();

    for request in requests {
        let Some(request_id) = request.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(method) = request.get("method").and_then(Value::as_str) else {
            continue;
        };
        let params = request.get("params").unwrap_or(&Value::Null);
        let id = opaque_id(&[conversation_id, request_id]);
        let (kind, title, summary, questions, pending_kind) = match method {
            "item/commandExecution/requestApproval" => {
                let network = params.get("networkApprovalContext").is_some()
                    || params
                        .get("proposedNetworkPolicyAmendments")
                        .and_then(Value::as_array)
                        .is_some_and(|amendments| !amendments.is_empty());
                (
                    if network { "network" } else { "exec" },
                    if network {
                        "Network access"
                    } else {
                        "Command approval"
                    },
                    if network {
                        network_summary(params)
                    } else {
                        command_summary(params)
                    },
                    Vec::new(),
                    PendingKind::Command,
                )
            }
            "item/fileChange/requestApproval" => (
                "patch",
                "File changes",
                file_summary(params),
                Vec::new(),
                PendingKind::FileChange,
            ),
            "item/permissions/requestApproval" => (
                "permission",
                "Permission request",
                permission_summary(params),
                Vec::new(),
                PendingKind::Permission {
                    permissions: params
                        .get("permissions")
                        .cloned()
                        .unwrap_or_else(|| json!({})),
                },
            ),
            "item/tool/requestUserInput" => {
                let (questions, question_ids, option_ids) =
                    parse_questions(conversation_id, request_id, params);
                (
                    "question",
                    "Codex has a question",
                    "Answer to continue the task.".into(),
                    questions,
                    PendingKind::Question {
                        question_ids,
                        option_ids,
                    },
                )
            }
            "mcpServer/elicitation/request" => (
                "tool",
                "Tool request",
                "A connected tool needs confirmation.".into(),
                Vec::new(),
                PendingKind::McpElicitation,
            ),
            _ => continue,
        };
        notifications.push(ControlNotification {
            id: id.clone(),
            kind: kind.into(),
            title: title.into(),
            summary,
            questions,
        });
        pending.insert(
            id.clone(),
            PendingControl {
                id,
                conversation_id: conversation_id.into(),
                owner_client_id: owner_client_id.into(),
                request_id: request_id.into(),
                kind: pending_kind,
            },
        );
    }
    ParsedControls {
        notifications,
        pending,
    }
}

pub(crate) fn build_control_request(
    pending: &PendingControl,
    action: &str,
    answers: &HashMap<String, Vec<String>>,
) -> Result<(String, Value), String> {
    let base = |value: Value| {
        json!({
            "conversationId": pending.conversation_id,
            "requestId": pending.request_id,
            "decision": value,
        })
    };
    match &pending.kind {
        PendingKind::Command => {
            let decision = match action {
                "accept" => json!("accept"),
                "accept-session" => json!("acceptForSession"),
                "decline" => json!("decline"),
                _ => return Err("unsupported command decision".into()),
            };
            Ok((
                "thread-follower-command-approval-decision".into(),
                base(decision),
            ))
        }
        PendingKind::FileChange => {
            let decision = match action {
                "accept" => json!("accept"),
                "accept-session" => json!("acceptForSession"),
                "decline" => json!("decline"),
                _ => return Err("unsupported file-change decision".into()),
            };
            Ok((
                "thread-follower-file-approval-decision".into(),
                base(decision),
            ))
        }
        PendingKind::Permission { permissions } => {
            let permissions = match action {
                "accept" => permissions.clone(),
                "decline" => json!({}),
                _ => return Err("unsupported permission decision".into()),
            };
            Ok((
                "thread-follower-permissions-request-approval-response".into(),
                json!({
                    "conversationId": pending.conversation_id,
                    "requestId": pending.request_id,
                    "response": { "permissions": permissions, "scope": "turn" },
                }),
            ))
        }
        PendingKind::Question {
            question_ids,
            option_ids,
        } => {
            if action != "answer" {
                return Err("unsupported question response".into());
            }
            let mut raw_answers = serde_json::Map::new();
            for (opaque, raw) in question_ids {
                let values = answers
                    .get(opaque)
                    .filter(|values| !values.is_empty())
                    .ok_or_else(|| "answer every question before submitting".to_owned())?;
                let values = values
                    .iter()
                    .map(|value| {
                        option_ids
                            .get(value)
                            .cloned()
                            .unwrap_or_else(|| value.clone())
                    })
                    .collect::<Vec<_>>();
                raw_answers.insert(raw.clone(), json!({ "answers": values }));
            }
            Ok((
                "thread-follower-submit-user-input".into(),
                json!({
                    "conversationId": pending.conversation_id,
                    "requestId": pending.request_id,
                    "response": { "answers": raw_answers },
                }),
            ))
        }
        PendingKind::McpElicitation => {
            let action = match action {
                "accept" => "accept",
                "decline" => "decline",
                _ => return Err("unsupported tool response".into()),
            };
            Ok((
                "thread-follower-submit-mcp-server-elicitation-response".into(),
                json!({
                    "conversationId": pending.conversation_id,
                    "requestId": pending.request_id,
                    "response": {
                        "action": action,
                        "content": if action == "accept" { json!({}) } else { Value::Null },
                        "_meta": null
                    },
                }),
            ))
        }
    }
}

struct PreparedFollowUp {
    prompt: String,
    client_message_id: String,
    cwd: String,
    input: Value,
}

fn restore_message(follow_up: &PreparedFollowUp, restore_id: String) -> Value {
    json!({
        "id": restore_id,
        "text": follow_up.prompt,
        "context": {
            "prompt": follow_up.prompt,
            "addedFiles": [],
            "fileAttachments": [],
            "ideContext": null,
            "imageAttachments": [],
            "workspaceRoots": [follow_up.cwd]
        },
        "cwd": follow_up.cwd,
        "createdAt": SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    })
}

fn cdp_host(target: &ControlTarget) -> Result<&str, String> {
    target
        .host_id
        .as_deref()
        .filter(|host| !host.trim().is_empty())
        .ok_or_else(|| "The Codex task host is unavailable".to_owned())
}

fn prepare_follow_up(target: &ControlTarget, prompt: &str) -> Result<PreparedFollowUp, String> {
    let prompt = prompt.trim().to_owned();
    if prompt.is_empty() {
        return Err("follow-up cannot be empty".into());
    }
    if prompt.chars().count() > 16_000 {
        return Err("follow-up is too long".into());
    }
    let cwd = target
        .cwd
        .as_deref()
        .filter(|cwd| !cwd.trim().is_empty())
        .ok_or_else(|| "The Codex task workspace is unavailable".to_owned())?
        .to_owned();
    Ok(PreparedFollowUp {
        input: json!([{ "type": "text", "text": prompt, "text_elements": [] }]),
        prompt,
        client_message_id: uuid::Uuid::new_v4().to_string(),
        cwd,
    })
}

pub(crate) fn build_follow_up(
    target: &ControlTarget,
    prompt: &str,
) -> Result<(String, Value), String> {
    let follow_up = prepare_follow_up(target, prompt)?;
    let steer = json!({
        "conversationId": target.conversation_id,
        "clientUserMessageId": follow_up.client_message_id,
        "input": follow_up.input,
        "serviceTier": null,
        "attachments": [],
        "restoreMessage": restore_message(&follow_up, uuid::Uuid::new_v4().to_string())
    });
    Ok(("thread-follower-steer-turn".into(), steer))
}

/// Build the live-proven Pets `Rf('send-follow-up-message', …)` envelope.
///
/// `cwd` is intentionally not emitted, but `prepare_follow_up` still requires it
/// as a native same-workspace gate before the App-local manager receives a turn.
pub(crate) fn build_cdp_ready_params(
    target: &ControlTarget,
    prompt: &str,
) -> Result<Value, String> {
    let follow_up = prepare_follow_up(target, prompt)?;
    let host_id = cdp_host(target)?;
    Ok(json!({
        "hostId": host_id,
        "conversationId": target.conversation_id,
        "prompt": follow_up.prompt,
        "serviceTier": null,
    }))
}

/// Build the live-proven Pets `Rf('steer-turn-for-host', …)` envelope.
pub(crate) fn build_cdp_steer_params(
    target: &ControlTarget,
    prompt: &str,
) -> Result<Value, String> {
    let follow_up = prepare_follow_up(target, prompt)?;
    let host_id = cdp_host(target)?;
    Ok(json!({
        "hostId": host_id,
        "conversationId": target.conversation_id,
        "input": follow_up.input,
        "serviceTier": null,
        "attachments": [],
        "restoreMessage": restore_message(&follow_up, uuid::Uuid::new_v4().to_string())
    }))
}

pub(crate) fn build_ready_follow_up(
    target: &ControlTarget,
    prompt: &str,
) -> Result<(String, Value), String> {
    let follow_up = prepare_follow_up(target, prompt)?;
    let start = json!({
        "conversationId": target.conversation_id,
        "turnStartParams": {
            "input": follow_up.input,
            "cwd": follow_up.cwd,
            "clientUserMessageId": follow_up.client_message_id,
            "serviceTier": null,
            "attachments": []
        }
    });
    Ok(("thread-follower-start-turn".into(), start))
}

#[cfg(test)]
mod tests {
    use super::{
        ControlTarget, PendingKind, build_cdp_ready_params, build_cdp_steer_params,
        build_control_request, build_follow_up, build_ready_follow_up, parse_controls,
    };
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn parses_only_actionable_request_summaries() {
        let parsed = parse_controls(
            "conversation-secret",
            "owner-secret",
            &json!({
                "requests": [
                    {"id":"request-secret","method":"item/commandExecution/requestApproval","params":{"command":"/private/project/node_modules/.bin/npm test --token secret-value","output":"do not expose"}},
                    {"id":"question-secret","method":"item/tool/requestUserInput","params":{"questions":[{"id":"q-secret","header":"Choice","question":"Continue?","options":[{"label":"Yes","description":"Proceed"}]}]}},
                    {"id":"ignored","method":"account/chatgptAuthTokens/refresh","params":{}}
                ]
            }),
        );
        assert_eq!(parsed.notifications.len(), 2);
        let serialized = serde_json::to_string(&parsed.notifications).unwrap();
        assert!(!serialized.contains("conversation-secret"));
        assert!(!serialized.contains("request-secret"));
        assert!(!serialized.contains("do not expose"));
        assert!(!serialized.contains("/private/project"));
        assert!(!serialized.contains("secret-value"));
        assert!(!serialized.contains("npm test"));
        assert!(serialized.contains("Run npm"));
    }

    #[test]
    fn builds_official_follower_approval_shape() {
        let pending = super::PendingControl {
            id: "opaque".into(),
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            request_id: "request".into(),
            kind: PendingKind::Command,
        };
        let (method, params) =
            build_control_request(&pending, "accept-session", &HashMap::new()).unwrap();
        assert_eq!(method, "thread-follower-command-approval-decision");
        assert_eq!(params["decision"], "acceptForSession");
        assert_eq!(params["requestId"], "request");
    }

    #[test]
    fn maps_opaque_question_ids_back_only_in_backend() {
        let parsed = parse_controls(
            "conversation",
            "owner",
            &json!({"requests":[{"id":"request","method":"item/tool/requestUserInput","params":{"questions":[{"id":"raw-question","header":"H","question":"Q?","options":[]}]}}]}),
        );
        let pending = parsed.pending.values().next().unwrap();
        let question_id = parsed.notifications[0].questions[0].id.clone();
        let (method, params) = build_control_request(
            pending,
            "answer",
            &HashMap::from([(question_id, vec!["A".into()])]),
        )
        .unwrap();
        assert_eq!(method, "thread-follower-submit-user-input");
        assert_eq!(
            params["response"]["answers"]["raw-question"]["answers"][0],
            "A"
        );
    }

    #[test]
    fn redacts_question_paths_but_restores_the_raw_option_in_backend() {
        let parsed = parse_controls(
            "conversation",
            "owner",
            &json!({"requests":[{
                "id":"request",
                "method":"item/tool/requestUserInput",
                "params":{"questions":[{
                    "id":"raw-question",
                    "header":"Choose path",
                    "question":"Use /private/project/secrets/config.json with --token secret-value?",
                    "options":[{"label":"/private/project/secrets/config.json","description":"token=secret-value"}]
                }]}
            }]}),
        );
        let serialized = serde_json::to_string(&parsed.notifications).unwrap();
        assert!(!serialized.contains("/private/project"));
        assert!(!serialized.contains("secret-value"));
        assert!(serialized.contains("…/config.json"));

        let pending = parsed.pending.values().next().unwrap();
        let question = &parsed.notifications[0].questions[0];
        let (method, params) = build_control_request(
            pending,
            "answer",
            &HashMap::from([(question.id.clone(), vec![question.options[0].id.clone()])]),
        )
        .unwrap();
        assert_eq!(method, "thread-follower-submit-user-input");
        assert_eq!(
            params["response"]["answers"]["raw-question"]["answers"][0],
            "/private/project/secrets/config.json"
        );
    }

    #[test]
    fn distinguishes_network_approval_from_shell_command() {
        let parsed = parse_controls(
            "conversation",
            "owner",
            &json!({"requests":[{
                "id":"request",
                "method":"item/commandExecution/requestApproval",
                "params":{"command":"curl example.test","networkApprovalContext":{"host":"example.test"}}
            }]}),
        );
        assert_eq!(parsed.notifications[0].kind, "network");
        assert_eq!(parsed.notifications[0].title, "Network access");
    }

    #[test]
    fn builds_official_mcp_elicitation_response() {
        let pending = super::PendingControl {
            id: "opaque".into(),
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            request_id: "request".into(),
            kind: PendingKind::McpElicitation,
        };
        let (method, params) = build_control_request(&pending, "accept", &HashMap::new()).unwrap();
        assert_eq!(
            method,
            "thread-follower-submit-mcp-server-elicitation-response"
        );
        assert_eq!(params["response"]["action"], "accept");
        assert_eq!(params["response"]["content"], json!({}));
        assert!(params["response"]["_meta"].is_null());
    }

    #[test]
    fn follow_up_builds_only_a_steering_request() {
        let target = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            host_id: Some("host".into()),
            cwd: Some("/workspace".into()),
        };
        let (steer_method, steer) = build_follow_up(&target, " continue ").unwrap();
        assert_eq!(steer_method, "thread-follower-steer-turn");
        assert_eq!(steer["input"][0]["text"], "continue");
        assert_eq!(
            steer["restoreMessage"]["context"]["workspaceRoots"][0],
            "/workspace"
        );
    }

    #[test]
    fn follow_up_requires_the_codex_workspace() {
        let target = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            host_id: Some("host".into()),
            cwd: None,
        };
        assert!(build_follow_up(&target, "continue").is_err());
        assert!(build_ready_follow_up(&target, "continue").is_err());
    }

    #[test]
    fn ready_follow_up_builds_a_start_turn_request() {
        let target = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            host_id: Some("host".into()),
            cwd: Some("/workspace".into()),
        };
        let (method, params) = build_ready_follow_up(&target, " continue ").unwrap();
        assert_eq!(method, "thread-follower-start-turn");
        assert_eq!(params["conversationId"], "conversation");
        assert_eq!(params["turnStartParams"]["input"][0]["text"], "continue");
        assert_eq!(params["turnStartParams"]["cwd"], "/workspace");
        assert!(
            params["turnStartParams"]["clientUserMessageId"]
                .as_str()
                .is_some_and(|id| !id.is_empty())
        );
    }

    #[test]
    fn cdp_ready_params_use_prompt_and_exact_native_host() {
        let target = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            host_id: Some("local".into()),
            cwd: Some("/workspace".into()),
        };
        let params = build_cdp_ready_params(&target, " continue ").unwrap();
        assert_eq!(params["conversationId"], "conversation");
        assert_eq!(params["hostId"], "local");
        assert_eq!(params["prompt"], "continue");
        assert!(params.get("input").is_none());
        assert!(params.get("cwd").is_none());
    }

    #[test]
    fn cdp_steer_params_match_rf_restore_shape() {
        let target = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            host_id: Some("local".into()),
            cwd: Some("/workspace".into()),
        };
        let params = build_cdp_steer_params(&target, "steer").unwrap();
        assert_eq!(params["hostId"], "local");
        assert_eq!(params["input"][0]["text"], "steer");
        assert_eq!(params["restoreMessage"]["cwd"], "/workspace");
        assert!(params["attachments"].as_array().is_some_and(Vec::is_empty));
        assert!(params.get("prompt").is_none());
    }

    #[test]
    fn cdp_envelopes_fail_closed_without_workspace_or_host() {
        let no_workspace = ControlTarget {
            conversation_id: "conversation".into(),
            owner_client_id: "owner".into(),
            host_id: Some("local".into()),
            cwd: None,
        };
        assert!(build_cdp_ready_params(&no_workspace, "continue").is_err());
        let no_host = ControlTarget {
            cwd: Some("/workspace".into()),
            host_id: None,
            ..no_workspace
        };
        assert!(build_cdp_ready_params(&no_host, "continue").is_err());
        assert!(build_cdp_steer_params(&no_host, "continue").is_err());
    }
}
