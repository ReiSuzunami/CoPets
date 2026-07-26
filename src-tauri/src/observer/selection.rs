use std::{
    cmp::Ordering,
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime},
};

use regex::Regex;
use tauri::AppHandle;
use tokio::time::sleep;

use crate::file_tail::AppendCursor;
use crate::local_trust::{owned_regular_metadata, validate_protected_parent_chain};

use super::{RuntimeHandle, codex_home, hash_id, recent_files, select_thread};

const INITIAL_APP_LOG_TAIL: u64 = 2 * 1024 * 1024;

fn live_log(path: &Path) -> bool {
    static PID: OnceLock<Regex> = OnceLock::new();
    let regex = PID.get_or_init(|| Regex::new(r"-(\d+)-t\d+-i\d+-").unwrap());
    let Some(pid) = path
        .file_name()
        .and_then(|value| value.to_str())
        .and_then(|name| regex.captures(name))
        .and_then(|caps| caps.get(1))
        .and_then(|value| value.as_str().parse::<i32>().ok())
    else {
        return true;
    };
    unsafe { libc::kill(pid, 0) == 0 }
}

fn load_known_threads(db: &Path) -> std::io::Result<HashSet<String>> {
    validate_protected_parent_chain(db)?;
    owned_regular_metadata(db)?;
    let output = std::process::Command::new("/usr/bin/sqlite3")
        .arg("-readonly")
        .arg(db)
        .arg("SELECT id FROM threads WHERE id IS NOT NULL;")
        .output()?;
    if !output.status.success() {
        return Err(std::io::Error::other("Codex thread index query failed"));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| std::io::Error::other("Codex thread index returned invalid UTF-8"))?;
    Ok(stdout.lines().map(str::to_owned).collect())
}

#[derive(Debug, PartialEq)]
struct ViewActivity {
    active: bool,
    conversation: String,
    window_focused: Option<bool>,
    window_visible: Option<bool>,
    window_id: Option<String>,
    owner_stream: bool,
}

fn parse_log_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_view_activity(line: &str) -> Option<ViewActivity> {
    if !line.contains("thread_stream_view_activity_changed") {
        return None;
    }
    let mut active = None;
    let mut conversation = None;
    let mut window_focused = None;
    let mut window_visible = None;
    let mut window_id = None;
    let mut owner_stream = false;
    for field in line.split_whitespace() {
        if let Some(value) = field.strip_prefix("active=") {
            active = parse_log_bool(value);
        }
        if let Some(value) = field.strip_prefix("conversationId=") {
            conversation = Some(value.to_owned());
        }
        if let Some(value) = field.strip_prefix("rendererWindowFocused=") {
            window_focused = parse_log_bool(value);
        }
        if let Some(value) = field.strip_prefix("rendererWindowVisible=") {
            window_visible = parse_log_bool(value);
        }
        if let Some(value) = field.strip_prefix("rendererWindowId=") {
            window_id = Some(value.to_owned());
        }
        if let Some(value) = field.strip_prefix("streamRole=") {
            owner_stream = value == "owner";
        }
    }
    Some(ViewActivity {
        active: active?,
        conversation: conversation?,
        window_focused,
        window_visible,
        window_id,
        owner_stream,
    })
}

fn is_selection_candidate(activity: &ViewActivity) -> bool {
    activity.active
        && activity.window_focused != Some(false)
        && activity.window_visible != Some(false)
}

fn is_thread_uuid(value: &str) -> bool {
    value.len() == 36 && uuid::Uuid::parse_str(value).is_ok()
}

fn is_unindexed_selection_candidate(activity: &ViewActivity) -> bool {
    activity.active
        && activity.window_focused == Some(true)
        && activity.window_visible == Some(true)
        && activity.owner_stream
        && is_thread_uuid(&activity.conversation)
}

#[derive(Debug, PartialEq)]
enum OwnerRoute {
    Conversation(String),
    Clear,
}

fn parse_owner_route(line: &str) -> Option<OwnerRoute> {
    let owner_sync = line.contains("IAB_LIFECYCLE received browser sidebar owner sync")
        || line.contains("thread_stream_owner_sync_broadcast_state_updated");
    if !owner_sync {
        return None;
    }
    let route = line
        .split_whitespace()
        .find_map(|field| field.strip_prefix("ownerRoutePath="))?;
    if route == "/" {
        return Some(OwnerRoute::Clear);
    }
    let conversation = route.strip_prefix("/local/")?;
    if conversation.is_empty()
        || conversation.len() > 160
        || !conversation
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    Some(OwnerRoute::Conversation(conversation.to_owned()))
}

#[derive(Default)]
struct ConfirmedSelection {
    activity: Option<String>,
    route: Option<ConfirmedRoute>,
}

struct ConfirmedRoute {
    conversation: String,
    historical: bool,
}

struct InitialSelectionEvidence {
    timestamp: Option<String>,
    kind: InitialSelectionEvidenceKind,
    order: usize,
    line: String,
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum InitialSelectionEvidenceKind {
    Route,
    Activity,
}

fn parse_log_timestamp(line: &str) -> Option<String> {
    let timestamp = line.get(..24)?;
    let bytes = timestamp.as_bytes();
    let digit = |index: usize| bytes[index].is_ascii_digit();
    (bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b'T'
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'.'
        && bytes[23] == b'Z'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22]
            .into_iter()
            .all(digit))
    .then(|| timestamp.to_owned())
}

fn selection_evidence_kind(line: &str) -> Option<InitialSelectionEvidenceKind> {
    if parse_owner_route(line).is_some() {
        Some(InitialSelectionEvidenceKind::Route)
    } else if parse_view_activity(line).is_some() {
        Some(InitialSelectionEvidenceKind::Activity)
    } else {
        None
    }
}

impl InitialSelectionEvidence {
    fn from_line(line: String, order: usize) -> Option<Self> {
        let kind = selection_evidence_kind(&line)?;
        Some(Self {
            timestamp: parse_log_timestamp(&line),
            kind,
            order,
            line,
        })
    }

    fn compare(&self, other: &Self) -> Ordering {
        let timestamp = match (&self.timestamp, &other.timestamp) {
            (Some(left), Some(right)) => left.cmp(right),
            (None, Some(_)) => Ordering::Less,
            (Some(_), None) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        timestamp
            .then_with(|| self.kind.cmp(&other.kind))
            .then_with(|| self.order.cmp(&other.order))
    }
}

impl ConfirmedSelection {
    fn current(&self) -> Option<&String> {
        self.activity
            .as_ref()
            .or_else(|| self.route.as_ref().map(|route| &route.conversation))
    }

    fn observe_activity(&mut self, activity: &ViewActivity, known: &HashSet<String>) -> bool {
        if !is_selection_candidate(activity)
            || (!known.contains(&activity.conversation)
                && !is_unindexed_selection_candidate(activity))
        {
            return false;
        }
        let previous = self.current().cloned();
        if self.route.as_ref().is_some_and(|route| route.historical) {
            self.route = None;
        }
        self.activity = Some(activity.conversation.clone());
        previous.as_ref() != self.current()
    }

    fn observe_route(
        &mut self,
        conversation: &str,
        known: &HashSet<String>,
        historical: bool,
    ) -> bool {
        if !known.contains(conversation) {
            return false;
        }
        let previous = self.current().cloned();
        self.route = Some(ConfirmedRoute {
            conversation: conversation.to_owned(),
            historical,
        });
        previous.as_ref() != self.current()
    }

    fn clear_route(&mut self) -> bool {
        let previous = self.current().cloned();
        self.route = None;
        self.activity = None;
        previous.as_ref() != self.current()
    }

    fn observe_line(&mut self, line: &str, known: &HashSet<String>) -> bool {
        if let Some(route) = parse_owner_route(line) {
            return match route {
                OwnerRoute::Conversation(conversation) => {
                    self.observe_route(&conversation, known, false)
                }
                OwnerRoute::Clear => self.clear_route(),
            };
        }
        parse_view_activity(line).is_some_and(|activity| self.observe_activity(&activity, known))
    }

    fn observe_initial_line(&mut self, line: &str, known: &HashSet<String>) -> bool {
        if let Some(route) = parse_owner_route(line) {
            return match route {
                OwnerRoute::Conversation(conversation) => {
                    self.observe_route(&conversation, known, true)
                }
                OwnerRoute::Clear => self.clear_route(),
            };
        }
        parse_view_activity(line).is_some_and(|activity| self.observe_activity(&activity, known))
    }
}

pub(super) struct AppLogSelectionAdapter {
    root: PathBuf,
    state_db: PathBuf,
    known: HashSet<String>,
    known_refreshed: Option<SystemTime>,
    cursors: HashMap<PathBuf, AppendCursor>,
    reconciled_paths: HashSet<PathBuf>,
    selection_event_watermark: Option<String>,
    selected: ConfirmedSelection,
}

impl AppLogSelectionAdapter {
    pub(super) fn from_default_paths() -> Option<Self> {
        let root = dirs::home_dir()?.join("Library/Logs/com.openai.codex");
        let state_db = codex_home()?.join("state_5.sqlite");
        Some(Self::new(root, state_db))
    }

    fn new(root: PathBuf, state_db: PathBuf) -> Self {
        Self {
            root,
            state_db,
            known: HashSet::new(),
            known_refreshed: None,
            cursors: HashMap::new(),
            reconciled_paths: HashSet::new(),
            selection_event_watermark: None,
            selected: ConfirmedSelection::default(),
        }
    }

    fn refresh_known_threads(&mut self, force: bool) {
        let due = self
            .known_refreshed
            .and_then(|refreshed| refreshed.elapsed().ok())
            .is_none_or(|elapsed| elapsed >= Duration::from_secs(30));
        if !force && !due {
            return;
        }
        self.known_refreshed = Some(SystemTime::now());
        if let Ok(known) = load_known_threads(&self.state_db) {
            self.known = known;
        }
    }

    fn advance_selection_event_watermark(&mut self, timestamp: Option<&str>) {
        let Some(timestamp) = timestamp else {
            return;
        };
        if self
            .selection_event_watermark
            .as_deref()
            .is_none_or(|current| timestamp > current)
        {
            self.selection_event_watermark = Some(timestamp.to_owned());
        }
    }

    fn observe_live_line(&mut self, line: &str) -> bool {
        let kind = selection_evidence_kind(line);
        let timestamp = kind.and_then(|_| parse_log_timestamp(line));
        if timestamp.as_deref().is_some_and(|timestamp| {
            self.selection_event_watermark
                .as_deref()
                .is_some_and(|current| timestamp < current)
        }) {
            return false;
        }
        let changed = self.selected.observe_line(line, &self.known);
        if kind.is_some() {
            self.advance_selection_event_watermark(timestamp.as_deref());
        }
        changed
    }

    fn reconcile_initial_evidence(&mut self, mut evidence: Vec<InitialSelectionEvidence>) {
        evidence.sort_by(InitialSelectionEvidence::compare);
        let floor = self.selection_event_watermark.clone();
        let mut latest = floor.clone();
        for item in &evidence {
            if item
                .timestamp
                .as_ref()
                .is_some_and(|timestamp| latest.as_ref().is_none_or(|current| timestamp > current))
            {
                latest = item.timestamp.clone();
            }
            let newer_than_floor = floor.as_ref().is_none_or(|floor| {
                item.timestamp
                    .as_ref()
                    .is_some_and(|timestamp| timestamp > floor)
            });
            if newer_than_floor {
                self.selected.observe_initial_line(&item.line, &self.known);
            }
        }
        self.selection_event_watermark = latest;
    }

    fn scan(&mut self, force_index_refresh: bool) -> bool {
        self.refresh_known_threads(force_index_refresh);
        let previous = self.selected.current().cloned();
        let mut initial_evidence = Vec::new();
        let mut initial_order = 0;
        for path in recent_files(&self.root, "log")
            .into_iter()
            .filter(|path| live_log(path))
        {
            let size = owned_regular_metadata(&path)
                .map(|metadata| metadata.len())
                .unwrap_or_default();
            let cursor = self
                .cursors
                .entry(path.clone())
                .or_insert_with(|| AppendCursor::new(size.saturating_sub(INITIAL_APP_LOG_TAIL)));
            let Ok((lines, reset)) = cursor.read_appended_with_reset(&path) else {
                continue;
            };
            let initial_tail = reset || !self.reconciled_paths.contains(&path);
            for line in lines {
                if initial_tail {
                    if let Some(evidence) = InitialSelectionEvidence::from_line(line, initial_order)
                    {
                        initial_order += 1;
                        initial_evidence.push(evidence);
                    }
                } else {
                    self.observe_live_line(&line);
                }
            }
            self.reconciled_paths.insert(path);
        }
        self.reconcile_initial_evidence(initial_evidence);
        previous.as_ref() != self.selected.current()
    }

    fn poll_once(&mut self) -> Option<String> {
        self.scan(false)
            .then(|| self.selected.current().cloned())
            .flatten()
    }

    fn refresh_now(&mut self) -> Option<String> {
        self.scan(true);
        self.selected.current().cloned()
    }
}

pub(super) async fn refresh_foreground_selection(
    app: &AppHandle,
    runtime: &RuntimeHandle,
) -> Result<String, String> {
    let selection = runtime.selection.clone();
    let conversation = tauri::async_runtime::spawn_blocking(move || {
        selection
            .lock()
            .expect("selection adapter poisoned")
            .as_mut()
            .and_then(AppLogSelectionAdapter::refresh_now)
    })
    .await
    .map_err(|_| "Codex foreground selection refresh failed".to_owned())?
    .ok_or_else(|| {
        "Open the intended task in Codex App, then retry steering from the pet".to_owned()
    })?;
    let thread = hash_id(&conversation);
    let changed = {
        let state = runtime.state.lock().expect("runtime state poisoned");
        state.selected.as_ref() != Some(&thread)
    };
    if changed {
        select_thread(app, runtime, thread.clone(), "codex-app-log-refresh");
    }
    Ok(thread)
}

pub(super) async fn run(app: AppHandle, runtime: RuntimeHandle) {
    loop {
        let selection = runtime.selection.clone();
        let Ok(conversation) = tauri::async_runtime::spawn_blocking(move || {
            selection
                .lock()
                .expect("selection adapter poisoned")
                .as_mut()
                .and_then(AppLogSelectionAdapter::poll_once)
        })
        .await
        else {
            return;
        };
        if let Some(conversation) = conversation {
            select_thread(&app, &runtime, hash_id(&conversation), "codex-app-log");
        }
        sleep(Duration::from_millis(500)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AppLogSelectionAdapter, ConfirmedSelection, OwnerRoute, is_selection_candidate,
        parse_owner_route, parse_view_activity,
    };
    use std::{
        collections::HashSet,
        fs::{self, OpenOptions},
        io::Write,
        time::SystemTime,
    };

    #[test]
    fn selection_requires_a_focused_visible_active_view() {
        let focused = parse_view_activity(
            "thread_stream_view_activity_changed active=true conversationId=abc rendererWindowFocused=true rendererWindowVisible=true rendererWindowId=1",
        )
        .unwrap();
        let background = parse_view_activity(
            "thread_stream_view_activity_changed active=true conversationId=def rendererWindowFocused=false rendererWindowVisible=true rendererWindowId=2",
        )
        .unwrap();
        assert!(is_selection_candidate(&focused));
        assert!(!is_selection_candidate(&background));
    }

    #[test]
    fn explicit_foreground_uuid_selects_projectless_conversation_without_index_row() {
        let activity = parse_view_activity(
            "thread_stream_view_activity_changed active=true conversationId=019f8afe-0bf4-7ac1-8eb0-70c747552aaa rendererWindowFocused=true rendererWindowVisible=true rendererWindowId=1 streamRole=owner",
        )
        .unwrap();
        let mut selected = ConfirmedSelection::default();

        assert!(selected.observe_activity(&activity, &HashSet::new()));
        assert_eq!(
            selected.current().map(String::as_str),
            Some("019f8afe-0bf4-7ac1-8eb0-70c747552aaa")
        );
    }

    #[test]
    fn owner_route_clear_allows_following_projectless_foreground_activity() {
        let old_conversation = "indexed-old";
        let new_conversation = "019f8afe-0bf4-7ac1-8eb0-70c747552aaa";
        let known = HashSet::from([old_conversation.to_owned()]);
        let mut selected = ConfirmedSelection::default();

        assert!(selected.observe_line(
            "IAB_LIFECYCLE received browser sidebar owner sync ownerRoutePath=/local/indexed-old",
            &known,
        ));
        assert_eq!(
            selected.current().map(String::as_str),
            Some(old_conversation)
        );

        assert!(selected.observe_line(
            "IAB_LIFECYCLE received browser sidebar owner sync ownerRoutePath=/",
            &known,
        ));
        assert_eq!(selected.current(), None);
        assert!(selected.observe_line(
            "thread_stream_view_activity_changed active=true conversationId=019f8afe-0bf4-7ac1-8eb0-70c747552aaa rendererWindowFocused=true rendererWindowVisible=true streamRole=owner",
            &known,
        ));
        assert_eq!(
            selected.current().map(String::as_str),
            Some(new_conversation)
        );
    }

    #[test]
    fn unindexed_activity_requires_explicit_owner_foreground_evidence() {
        let known = HashSet::new();
        for line in [
            "thread_stream_view_activity_changed active=true conversationId=019f8afe-0bf4-7ac1-8eb0-70c747552aaa rendererWindowFocused=true rendererWindowVisible=true",
            "thread_stream_view_activity_changed active=true conversationId=019f8afe-0bf4-7ac1-8eb0-70c747552aaa rendererWindowFocused=true rendererWindowVisible=true streamRole=follower",
            "thread_stream_view_activity_changed active=true conversationId=019f8afe-0bf4-7ac1-8eb0-70c747552aaa rendererWindowVisible=true streamRole=owner",
            "thread_stream_view_activity_changed active=true conversationId=019f8afe-0bf4-7ac1-8eb0-70c747552aaa rendererWindowFocused=true rendererWindowVisible=false streamRole=owner",
            "thread_stream_view_activity_changed active=true conversationId=client-new-thread rendererWindowFocused=true rendererWindowVisible=true streamRole=owner",
        ] {
            let activity = parse_view_activity(line).unwrap();
            assert!(!ConfirmedSelection::default().observe_activity(&activity, &known));
        }
    }

    #[test]
    fn weak_focus_and_active_negatives_keep_the_last_confirmed_selection() {
        let known = HashSet::from(["abc".to_owned()]);
        let focused = parse_view_activity(
            "thread_stream_view_activity_changed active=true conversationId=abc rendererWindowFocused=true rendererWindowVisible=true rendererWindowId=1",
        )
        .unwrap();
        let unfocused_visible = parse_view_activity(
            "thread_stream_view_activity_changed active=true conversationId=abc rendererWindowFocused=false rendererWindowVisible=true rendererWindowId=1",
        )
        .unwrap();
        let inactive = parse_view_activity(
            "thread_stream_view_activity_changed active=false conversationId=abc rendererWindowFocused=false rendererWindowVisible=true rendererWindowId=1",
        )
        .unwrap();
        let mut selected = ConfirmedSelection::default();
        assert!(selected.observe_activity(&focused, &known));
        assert!(!selected.observe_activity(&unfocused_visible, &known));
        assert!(!selected.observe_activity(&inactive, &known));
        assert_eq!(selected.current().map(String::as_str), Some("abc"));
    }

    #[test]
    fn foreground_activity_outweighs_a_conflicting_owner_route_hint() {
        let known = HashSet::from(["foreground".to_owned(), "background".to_owned()]);
        let Some(OwnerRoute::Conversation(background)) = parse_owner_route(
            "IAB_LIFECYCLE received browser sidebar owner sync conversationId=client-new-thread:abc ownerRoutePath=/local/background windowId=1",
        ) else {
            panic!("expected a local conversation route");
        };
        let foreground = parse_view_activity(
            "thread_stream_view_activity_changed active=true conversationId=foreground rendererWindowFocused=true rendererWindowVisible=true rendererWindowId=1",
        )
        .unwrap();
        let mut selected = ConfirmedSelection::default();
        assert!(selected.observe_activity(&foreground, &known));
        assert!(!selected.observe_route(&background, &known, false));
        assert_eq!(selected.current().map(String::as_str), Some("foreground"));
        assert_eq!(
            parse_owner_route(
                "IAB_LIFECYCLE received browser sidebar owner sync conversationId=client-new-thread:abc ownerRoutePath=/ windowId=1"
            ),
            Some(OwnerRoute::Clear)
        );
    }

    #[test]
    fn parses_only_view_activity() {
        assert_eq!(
            parse_view_activity(
                "thread_stream_view_activity_changed active=true conversationId=abc"
            )
            .map(|activity| (activity.active, activity.conversation)),
            Some((true, "abc".into()))
        );
        assert_eq!(
            parse_view_activity("unrelated active=true conversationId=abc"),
            None
        );
    }

    #[test]
    fn background_poll_and_explicit_refresh_share_one_selection_cursor() {
        let root = std::env::temp_dir().join(format!("copets-selection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let log = root.join("codex-desktop-test.log");
        fs::write(
            &log,
            b"thread_stream_view_activity_changed active=true conversationId=first rendererWindowFocused=true rendererWindowVisible=true\n",
        )
        .unwrap();
        let mut adapter = AppLogSelectionAdapter::new(root.clone(), root.join("missing.sqlite"));
        adapter.known = HashSet::from(["first".to_owned(), "second".to_owned()]);
        adapter.known_refreshed = Some(SystemTime::now());

        assert_eq!(adapter.poll_once().as_deref(), Some("first"));
        assert_eq!(adapter.poll_once(), None);
        OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(
                b"IAB_LIFECYCLE received browser sidebar owner sync ownerRoutePath=/local/second\n",
            )
            .unwrap();
        assert_eq!(adapter.refresh_now().as_deref(), Some("first"));
        OpenOptions::new()
            .append(true)
            .open(&log)
            .unwrap()
            .write_all(
                b"thread_stream_view_activity_changed active=true conversationId=second rendererWindowFocused=true rendererWindowVisible=true\n",
            )
            .unwrap();
        assert_eq!(adapter.poll_once().as_deref(), Some("second"));
        assert_eq!(adapter.poll_once(), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initial_tail_foreground_activity_supersedes_an_older_known_route() {
        let root = std::env::temp_dir().join(format!("copets-selection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let log = root.join("codex-desktop-test.log");
        fs::write(
            &log,
            b"IAB_LIFECYCLE received browser sidebar owner sync ownerRoutePath=/local/old\nthread_stream_view_activity_changed active=true conversationId=new rendererWindowFocused=true rendererWindowVisible=true\n",
        )
        .unwrap();
        let mut adapter = AppLogSelectionAdapter::new(root.clone(), root.join("missing.sqlite"));
        adapter.known = HashSet::from(["old".to_owned(), "new".to_owned()]);
        adapter.known_refreshed = Some(SystemTime::now());

        assert_eq!(adapter.poll_once().as_deref(), Some("new"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn initial_tail_uses_event_timestamps_not_log_file_modification_order() {
        let root = std::env::temp_dir().join(format!("copets-selection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let newer_event = root.join("newer-event.log");
        let older_event = root.join("older-event.log");
        fs::write(
            &newer_event,
            b"2026-07-25T16:48:00.289Z info thread_stream_view_activity_changed active=true conversationId=new rendererWindowFocused=true rendererWindowVisible=true\n",
        )
        .unwrap();
        fs::write(
            &older_event,
            b"2026-07-25T16:03:46.350Z info thread_stream_view_activity_changed active=true conversationId=old rendererWindowFocused=true rendererWindowVisible=true\n",
        )
        .unwrap();
        let now = SystemTime::now();
        fs::File::open(&newer_event)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(now - std::time::Duration::from_secs(2)))
            .unwrap();
        fs::File::open(&older_event)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(now - std::time::Duration::from_secs(1)))
            .unwrap();
        let mut adapter = AppLogSelectionAdapter::new(root.clone(), root.join("missing.sqlite"));
        adapter.known = HashSet::from(["old".to_owned(), "new".to_owned()]);
        adapter.known_refreshed = Some(SystemTime::now());

        assert_eq!(adapter.poll_once().as_deref(), Some("new"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn new_or_rewritten_log_tails_cannot_overwrite_newer_foreground_activity() {
        let root = std::env::temp_dir().join(format!("copets-selection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let current_log = root.join("current.log");
        fs::write(
            &current_log,
            b"2026-07-25T16:48:00.289Z info thread_stream_view_activity_changed active=true conversationId=current rendererWindowFocused=true rendererWindowVisible=true\nextra padding so the rewrite is shorter\n",
        )
        .unwrap();
        let mut adapter = AppLogSelectionAdapter::new(root.clone(), root.join("missing.sqlite"));
        adapter.known =
            HashSet::from(["current".to_owned(), "next".to_owned(), "stale".to_owned()]);
        adapter.known_refreshed = Some(SystemTime::now());

        assert_eq!(adapter.poll_once().as_deref(), Some("current"));

        let next_log = root.join("next.log");
        fs::write(
            &next_log,
            b"2026-07-25T16:49:00.289Z info thread_stream_view_activity_changed active=true conversationId=next rendererWindowFocused=true rendererWindowVisible=true\n",
        )
        .unwrap();
        assert_eq!(adapter.poll_once().as_deref(), Some("next"));

        fs::write(
            &current_log,
            b"2026-07-25T16:03:46.350Z info thread_stream_view_activity_changed active=true conversationId=stale rendererWindowFocused=true rendererWindowVisible=true\n",
        )
        .unwrap();
        assert_eq!(adapter.poll_once(), None);
        assert_eq!(adapter.refresh_now().as_deref(), Some("next"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_adapter_fails_closed_without_a_known_thread_index() {
        let root = std::env::temp_dir().join(format!("copets-selection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("codex-desktop-test.log"),
            b"thread_stream_view_activity_changed active=true conversationId=unknown rendererWindowFocused=true rendererWindowVisible=true\n",
        )
        .unwrap();
        let mut adapter = AppLogSelectionAdapter::new(root.clone(), root.join("missing.sqlite"));

        assert_eq!(adapter.poll_once(), None);
        assert_eq!(adapter.refresh_now(), None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_adapter_accepts_projectless_owner_foreground_evidence() {
        let root = std::env::temp_dir().join(format!("copets-selection-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&root).unwrap();
        let conversation = "019f8afe-0bf4-7ac1-8eb0-70c747552aaa";
        fs::write(
            root.join("codex-desktop-test.log"),
            format!(
                "thread_stream_view_activity_changed active=true conversationId={conversation} rendererWindowFocused=true rendererWindowVisible=true streamRole=owner\n"
            ),
        )
        .unwrap();
        let mut adapter = AppLogSelectionAdapter::new(root.clone(), root.join("missing.sqlite"));

        assert_eq!(adapter.poll_once().as_deref(), Some(conversation));
        fs::remove_dir_all(root).unwrap();
    }
}
