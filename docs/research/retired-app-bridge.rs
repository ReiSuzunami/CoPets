//! Retired Accessibility composer prototype.
//!
//! This file is intentionally outside the runtime crate. Production steering uses follower IPC
//! only and never activates or edits Codex App UI.

pub async fn send_follow_up_to_codex_app(prompt: &str) -> Result<(), String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("follow-up cannot be empty".to_owned());
    }
    if prompt.chars().count() > 16_000 {
        return Err("follow-up is too long".to_owned());
    }

    #[cfg(target_os = "macos")]
    {
        let prompt = prompt.to_owned();
        tokio::task::spawn_blocking(move || macos::send(&prompt))
            .await
            .map_err(|error| format!("Codex App bridge stopped unexpectedly: {error}"))?
    }

    #[cfg(not(target_os = "macos"))]
    Err("Replying through the running Codex App is currently supported on macOS only".to_owned())
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{
        collections::HashSet,
        ffi::{CString, c_char, c_void},
        ptr, thread,
        time::Duration,
    };

    use objc2_app_kit::NSRunningApplication;
    use objc2_foundation::NSString;

    type CFTypeRef = *const c_void;
    type CFStringRef = *const c_void;
    type CFArrayRef = *const c_void;
    type CFDictionaryRef = *const c_void;
    type AXUIElementRef = *const c_void;
    type CFIndex = isize;
    type CFTypeId = usize;
    type AXError = i32;

    const AX_SUCCESS: AXError = 0;
    const AX_NO_VALUE: AXError = -25212;
    const AX_ATTRIBUTE_UNSUPPORTED: AXError = -25205;
    const UTF8_ENCODING: u32 = 0x0800_0100;
    const MAX_TREE_DEPTH: usize = 40;
    const MAX_TREE_NODES: usize = 4_000;
    const AX_VALUE_CG_POINT: u32 = 1;
    const AX_VALUE_CG_SIZE: u32 = 2;
    const KEY_A: u16 = 0x00;
    const KEY_RETURN: u16 = 0x24;
    const KEY_COMMAND: u16 = 0x37;
    const COMMAND_FLAG: u64 = 0x0010_0000;

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct Point {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct Size {
        width: f64,
        height: f64,
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct Bounds {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    }

    struct OwnedCf(CFTypeRef);

    impl OwnedCf {
        fn from_create(value: CFTypeRef) -> Option<Self> {
            (!value.is_null()).then_some(Self(value))
        }

        fn retained(value: CFTypeRef) -> Self {
            unsafe { CFRetain(value) };
            Self(value)
        }

        fn as_ptr(&self) -> CFTypeRef {
            self.0
        }
    }

    impl Drop for OwnedCf {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { CFRelease(self.0) };
            }
        }
    }

    struct Candidate {
        element: OwnedCf,
        bounds: Bounds,
        score: i32,
    }

    #[derive(Default)]
    struct ScanResult {
        editor: Option<Candidate>,
        button: Option<Candidate>,
    }

    #[derive(Default)]
    struct NodeMetadata {
        role: String,
        title: String,
        description: String,
        placeholder: String,
        identifier: String,
        help: String,
        value: String,
    }

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: CFDictionaryRef) -> bool;
        static kAXTrustedCheckOptionPrompt: CFStringRef;
        fn AXUIElementCreateApplication(pid: i32) -> AXUIElementRef;
        fn AXUIElementCopyAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: *mut CFTypeRef,
        ) -> AXError;
        fn AXUIElementIsAttributeSettable(
            element: AXUIElementRef,
            attribute: CFStringRef,
            settable: *mut u8,
        ) -> AXError;
        fn AXUIElementSetAttributeValue(
            element: AXUIElementRef,
            attribute: CFStringRef,
            value: CFTypeRef,
        ) -> AXError;
        fn AXUIElementPerformAction(element: AXUIElementRef, action: CFStringRef) -> AXError;
        fn AXUIElementSetMessagingTimeout(element: AXUIElementRef, timeout: f32) -> AXError;
        fn AXValueGetTypeID() -> CFTypeId;
        fn AXValueGetType(value: CFTypeRef) -> u32;
        fn AXValueGetValue(value: CFTypeRef, value_type: u32, output: *mut c_void) -> bool;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRetain(value: CFTypeRef) -> CFTypeRef;
        fn CFRelease(value: CFTypeRef);
        fn CFHash(value: CFTypeRef) -> usize;
        fn CFGetTypeID(value: CFTypeRef) -> CFTypeId;
        fn CFStringGetTypeID() -> CFTypeId;
        fn CFStringCreateWithCString(
            allocator: *const c_void,
            value: *const c_char,
            encoding: u32,
        ) -> CFStringRef;
        fn CFStringGetLength(value: CFStringRef) -> CFIndex;
        fn CFStringGetMaximumSizeForEncoding(length: CFIndex, encoding: u32) -> CFIndex;
        fn CFStringGetCString(
            value: CFStringRef,
            buffer: *mut c_char,
            buffer_size: CFIndex,
            encoding: u32,
        ) -> bool;
        fn CFArrayGetTypeID() -> CFTypeId;
        fn CFArrayGetCount(array: CFArrayRef) -> CFIndex;
        fn CFArrayGetValueAtIndex(array: CFArrayRef, index: CFIndex) -> CFTypeRef;
        fn CFBooleanGetTypeID() -> CFTypeId;
        fn CFBooleanGetValue(value: CFTypeRef) -> bool;
        static kCFBooleanTrue: CFTypeRef;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const CFTypeRef,
            values: *const CFTypeRef,
            count: CFIndex,
            key_callbacks: *const c_void,
            value_callbacks: *const c_void,
        ) -> CFDictionaryRef;
    }

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreateKeyboardEvent(
            source: *const c_void,
            virtual_key: u16,
            key_down: bool,
        ) -> CFTypeRef;
        fn CGEventSetFlags(event: CFTypeRef, flags: u64);
        fn CGEventKeyboardSetUnicodeString(
            event: CFTypeRef,
            string_length: usize,
            unicode_string: *const u16,
        );
        fn CGEventPostToPid(pid: i32, event: CFTypeRef);
    }

    pub(super) fn send(prompt: &str) -> Result<(), String> {
        let pid = codex_pid()?;
        send_to_active_codex(pid, prompt)
    }

    fn send_to_active_codex(pid: i32, prompt: &str) -> Result<(), String> {
        ensure_accessibility_permission()?;
        let app = OwnedCf::from_create(unsafe { AXUIElementCreateApplication(pid) })
            .ok_or_else(|| "The running Codex App accessibility tree is unavailable".to_owned())?;
        let _ = unsafe { AXUIElementSetMessagingTimeout(app.as_ptr(), 2.0) };
        let mut located = None;
        for _ in 0..24 {
            if let Ok(window) = focused_window(app.as_ptr()) {
                let window_bounds = bounds(window.as_ptr()).unwrap_or_default();
                if let Some(editor) = scan(window.as_ptr(), window_bounds, None)
                    .editor
                    .filter(|candidate| candidate.score >= 95)
                {
                    located = Some((window, window_bounds, editor));
                    break;
                }
            }
            thread::sleep(Duration::from_millis(75));
        }
        let (window, window_bounds, editor) = located.ok_or_else(|| {
            "Codex App follow-up box was not found; keep the intended task visible and retry"
                .to_owned()
        })?;

        let existing_draft = editor_value(editor.element.as_ptr())?.unwrap_or_default();
        if !is_empty_editor_value(&existing_draft) && existing_draft.trim() != prompt {
            return Err(
                "Codex App already has a draft; send or clear that draft before retrying"
                    .to_owned(),
            );
        }

        focus(editor.element.as_ptr())?;
        replace_editor_text_with_keyboard(pid, editor.element.as_ptr(), prompt)?;

        let mut send_button = None;
        for _ in 0..8 {
            thread::sleep(Duration::from_millis(70));
            send_button = scan(window.as_ptr(), window_bounds, Some(editor.bounds))
                .button
                .filter(|candidate| candidate.score >= 120);
            if send_button.is_some() {
                break;
            }
        }
        if let Some(send_button) = send_button {
            press(send_button.element.as_ptr())
        } else {
            post_return(pid)
        }
    }

    fn ensure_accessibility_permission() -> Result<(), String> {
        let ax_trusted = unsafe { AXIsProcessTrusted() };
        if ax_trusted {
            return Ok(());
        }

        let keys = [unsafe { kAXTrustedCheckOptionPrompt }];
        let values = [unsafe { kCFBooleanTrue }];
        let options = unsafe {
            CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                ptr::null(),
                ptr::null(),
            )
        };
        if !options.is_null() {
            let _ = unsafe { AXIsProcessTrustedWithOptions(options) };
            unsafe { CFRelease(options) };
        }
        Err("Allow Codex Pet Sidecar in System Settings > Privacy & Security > Accessibility, then retry".to_owned())
    }

    fn codex_pid() -> Result<i32, String> {
        let bundle_id = NSString::from_str("com.openai.codex");
        let apps = NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle_id);
        apps.iter()
            .filter(|app| !app.isTerminated())
            .max_by_key(|app| (app.isActive(), app.isFinishedLaunching()))
            .map(|app| app.processIdentifier())
            .filter(|pid| *pid > 0)
            .ok_or_else(|| "Codex App is not running".to_owned())
    }

    fn focused_window(app: AXUIElementRef) -> Result<OwnedCf, String> {
        copy_attribute(app, "AXFocusedWindow")
            .or_else(|| copy_attribute(app, "AXMainWindow"))
            .ok_or_else(|| "Codex App has no active window".to_owned())
    }

    fn scan(root: AXUIElementRef, window: Bounds, editor_bounds: Option<Bounds>) -> ScanResult {
        let mut result = ScanResult::default();
        let mut visited = HashSet::new();
        let mut nodes = 0;
        visit(
            root,
            window,
            editor_bounds,
            0,
            &mut nodes,
            &mut visited,
            &mut result,
        );
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        element: AXUIElementRef,
        window: Bounds,
        editor_bounds: Option<Bounds>,
        depth: usize,
        nodes: &mut usize,
        visited: &mut HashSet<usize>,
        result: &mut ScanResult,
    ) {
        if element.is_null()
            || depth > MAX_TREE_DEPTH
            || *nodes >= MAX_TREE_NODES
            || !visited.insert(unsafe { CFHash(element) })
        {
            return;
        }
        *nodes += 1;

        let metadata = metadata(element);
        let element_bounds = bounds(element).unwrap_or_default();
        if matches!(metadata.role.as_str(), "AXTextArea" | "AXTextField") {
            let score = editor_score(&metadata, element_bounds, window);
            consider(&mut result.editor, element, element_bounds, score);
        } else if metadata.role == "AXButton"
            && is_enabled(element)
            && let Some(editor_bounds) = editor_bounds
        {
            let score = button_score(&metadata, element_bounds, editor_bounds);
            consider(&mut result.button, element, element_bounds, score);
        }

        let Some(children) = copy_attribute(element, "AXChildren") else {
            return;
        };
        if unsafe { CFGetTypeID(children.as_ptr()) } != unsafe { CFArrayGetTypeID() } {
            return;
        }
        let count = unsafe { CFArrayGetCount(children.as_ptr()) }.clamp(0, 10_000);
        for index in 0..count {
            let child = unsafe { CFArrayGetValueAtIndex(children.as_ptr(), index) };
            visit(
                child,
                window,
                editor_bounds,
                depth + 1,
                nodes,
                visited,
                result,
            );
            if *nodes >= MAX_TREE_NODES {
                break;
            }
        }
    }

    fn metadata(element: AXUIElementRef) -> NodeMetadata {
        NodeMetadata {
            role: string_attribute(element, "AXRole").unwrap_or_default(),
            title: string_attribute(element, "AXTitle").unwrap_or_default(),
            description: string_attribute(element, "AXDescription").unwrap_or_default(),
            placeholder: string_attribute(element, "AXPlaceholderValue").unwrap_or_default(),
            identifier: string_attribute(element, "AXIdentifier").unwrap_or_default(),
            help: string_attribute(element, "AXHelp").unwrap_or_default(),
            value: string_attribute(element, "AXValue").unwrap_or_default(),
        }
    }

    fn editor_score(metadata: &NodeMetadata, bounds: Bounds, window: Bounds) -> i32 {
        let mut score = match metadata.role.as_str() {
            "AXTextArea" => 80,
            "AXTextField" => 45,
            _ => return i32::MIN,
        };
        let labels = format!(
            "{} {} {} {} {} {}",
            metadata.title,
            metadata.description,
            metadata.placeholder,
            metadata.identifier,
            metadata.help,
            metadata.value
        )
        .to_lowercase();
        if contains_any(
            &labels,
            &[
                "follow-up",
                "follow up",
                "后续",
                "後續",
                "message codex",
                "codex composer",
                "do anything",
                "ask anything",
                "add a follow-up",
                "add follow-up",
            ],
        ) {
            score += 100;
        } else if labels.contains("codex") {
            score += 35;
        }
        if metadata.identifier.to_lowercase().contains("composer") {
            score += 100;
        }
        if contains_any(&labels, &["search", "搜索", "搜尋", "find in"]) {
            score -= 300;
        }
        if bounds.width >= 280.0 {
            score += 25;
        }
        if bounds.height >= 32.0 {
            score += 10;
        }
        if window.height > 0.0 && bounds.y + bounds.height / 2.0 >= window.y + window.height * 0.55
        {
            score += 25;
        }
        score
    }

    fn button_score(metadata: &NodeMetadata, bounds: Bounds, editor: Bounds) -> i32 {
        let labels = format!(
            "{} {} {} {} {}",
            metadata.title,
            metadata.description,
            metadata.help,
            metadata.identifier,
            metadata.value
        )
        .trim()
        .to_lowercase();
        let mut score = if matches!(labels.as_str(), "send" | "发送" | "發送") {
            180
        } else if contains_any(
            &labels,
            &[
                "send message",
                "send reply",
                "submit message",
                "发送",
                "發送",
            ],
        ) {
            145
        } else {
            return i32::MIN;
        };
        if contains_any(&labels, &["stop", "cancel", "停止", "取消"]) {
            return i32::MIN;
        }
        let button_center_x = bounds.x + bounds.width / 2.0;
        let button_center_y = bounds.y + bounds.height / 2.0;
        let editor_center_x = editor.x + editor.width / 2.0;
        let editor_center_y = editor.y + editor.height / 2.0;
        let dx = (button_center_x - editor_center_x).abs();
        let dy = (button_center_y - editor_center_y).abs();
        if dx <= editor.width / 2.0 + 100.0 && dy <= editor.height / 2.0 + 100.0 {
            score += 60;
        } else if dx + dy <= 500.0 {
            score += 20;
        }
        score
    }

    fn consider(slot: &mut Option<Candidate>, element: AXUIElementRef, bounds: Bounds, score: i32) {
        if score <= slot.as_ref().map_or(i32::MIN, |candidate| candidate.score) {
            return;
        }
        *slot = Some(Candidate {
            element: OwnedCf::retained(element),
            bounds,
            score,
        });
    }

    fn contains_any(value: &str, needles: &[&str]) -> bool {
        needles.iter().any(|needle| value.contains(needle))
    }

    fn is_empty_editor_value(value: &str) -> bool {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "" | "do anything"
                | "ask anything"
                | "message codex"
                | "add a follow-up"
                | "add follow-up"
        )
    }

    fn focus(element: AXUIElementRef) -> Result<(), String> {
        let attribute = cf_string("AXFocused")?;
        let mut settable = 0;
        let result =
            unsafe { AXUIElementIsAttributeSettable(element, attribute.as_ptr(), &mut settable) };
        if result != AX_SUCCESS || settable == 0 {
            return Err("Codex App follow-up box cannot receive focus".to_owned());
        }
        let result =
            unsafe { AXUIElementSetAttributeValue(element, attribute.as_ptr(), kCFBooleanTrue) };
        if result == AX_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "Codex App follow-up box could not be focused ({result})"
            ))
        }
    }

    fn press(element: AXUIElementRef) -> Result<(), String> {
        let action = cf_string("AXPress")?;
        let result = unsafe { AXUIElementPerformAction(element, action.as_ptr()) };
        if result == AX_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "Codex App Send button could not be pressed ({result})"
            ))
        }
    }

    fn post_key_event(
        pid: i32,
        keycode: u16,
        key_down: bool,
        flags: u64,
        unicode: Option<&[u16]>,
    ) -> Result<(), String> {
        let event = OwnedCf::from_create(unsafe {
            CGEventCreateKeyboardEvent(ptr::null(), keycode, key_down)
        })
        .ok_or_else(|| "Codex App keyboard event could not be created".to_owned())?;
        if flags != 0 {
            unsafe { CGEventSetFlags(event.as_ptr(), flags) };
        }
        if let Some(unicode) = unicode {
            unsafe {
                CGEventKeyboardSetUnicodeString(event.as_ptr(), unicode.len(), unicode.as_ptr())
            };
        }
        unsafe { CGEventPostToPid(pid, event.as_ptr()) };
        thread::sleep(Duration::from_millis(8));
        Ok(())
    }

    fn replace_editor_text_with_keyboard(
        pid: i32,
        editor: AXUIElementRef,
        prompt: &str,
    ) -> Result<(), String> {
        post_key_event(pid, KEY_COMMAND, true, COMMAND_FLAG, None)?;
        post_key_event(pid, KEY_A, true, COMMAND_FLAG, None)?;
        post_key_event(pid, KEY_A, false, COMMAND_FLAG, None)?;
        post_key_event(pid, KEY_COMMAND, false, 0, None)?;

        let unicode: Vec<u16> = prompt.encode_utf16().collect();
        post_key_event(pid, KEY_A, true, 0, Some(&unicode))?;
        post_key_event(pid, KEY_A, false, 0, None)?;

        for _ in 0..8 {
            if editor_value(editor)?.as_deref() == Some(prompt) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(40));
        }
        set_editor_value(editor, prompt)
    }

    fn post_return(pid: i32) -> Result<(), String> {
        post_key_event(pid, KEY_RETURN, true, 0, None)?;
        post_key_event(pid, KEY_RETURN, false, 0, None)
    }

    fn set_editor_value(element: AXUIElementRef, value: &str) -> Result<(), String> {
        let attribute = cf_string("AXValue")?;
        let mut settable = 0;
        let result =
            unsafe { AXUIElementIsAttributeSettable(element, attribute.as_ptr(), &mut settable) };
        if result != AX_SUCCESS || settable == 0 {
            return Err(format!(
                "Codex App follow-up box cannot be edited through Accessibility ({result})"
            ));
        }

        let value_ref = cf_string(value)
            .map_err(|_| "Codex App follow-up contains an unsupported null byte".to_owned())?;
        let result = unsafe {
            AXUIElementSetAttributeValue(element, attribute.as_ptr(), value_ref.as_ptr())
        };
        if result != AX_SUCCESS {
            return Err(format!(
                "Codex App follow-up text could not be set through Accessibility ({result})"
            ));
        }

        for _ in 0..8 {
            if editor_value(element)?.as_deref() == Some(value) {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(40));
        }
        Err("Codex App did not accept the follow-up text through Accessibility".to_owned())
    }

    fn editor_value(element: AXUIElementRef) -> Result<Option<String>, String> {
        let attribute = cf_string("AXValue")?;
        let mut value = ptr::null();
        let result =
            unsafe { AXUIElementCopyAttributeValue(element, attribute.as_ptr(), &mut value) };
        if result == AX_NO_VALUE {
            return Ok(Some(String::new()));
        }
        if result == AX_ATTRIBUTE_UNSUPPORTED {
            return Err("Codex App follow-up text cannot be checked safely".to_owned());
        }
        if result != AX_SUCCESS {
            return Err(format!(
                "Codex App follow-up text is unavailable ({result})"
            ));
        }
        let Some(value) = OwnedCf::from_create(value) else {
            return Ok(Some(String::new()));
        };
        cf_to_string(value.as_ptr())
            .map(Some)
            .ok_or_else(|| "Codex App follow-up text has an unsupported format".to_owned())
    }

    fn copy_attribute(element: AXUIElementRef, name: &str) -> Option<OwnedCf> {
        let attribute = cf_string(name).ok()?;
        let mut value = ptr::null();
        let result =
            unsafe { AXUIElementCopyAttributeValue(element, attribute.as_ptr(), &mut value) };
        (result == AX_SUCCESS)
            .then(|| OwnedCf::from_create(value))
            .flatten()
    }

    fn string_attribute(element: AXUIElementRef, name: &str) -> Option<String> {
        copy_attribute(element, name).and_then(|value| cf_to_string(value.as_ptr()))
    }

    fn is_enabled(element: AXUIElementRef) -> bool {
        let Some(value) = copy_attribute(element, "AXEnabled") else {
            return true;
        };
        if unsafe { CFGetTypeID(value.as_ptr()) } != unsafe { CFBooleanGetTypeID() } {
            return true;
        }
        unsafe { CFBooleanGetValue(value.as_ptr()) }
    }

    fn bounds(element: AXUIElementRef) -> Option<Bounds> {
        let position = copy_attribute(element, "AXPosition")?;
        let size = copy_attribute(element, "AXSize")?;
        if unsafe { CFGetTypeID(position.as_ptr()) } != unsafe { AXValueGetTypeID() }
            || unsafe { CFGetTypeID(size.as_ptr()) } != unsafe { AXValueGetTypeID() }
            || unsafe { AXValueGetType(position.as_ptr()) } != AX_VALUE_CG_POINT
            || unsafe { AXValueGetType(size.as_ptr()) } != AX_VALUE_CG_SIZE
        {
            return None;
        }
        let mut point = Point::default();
        let mut value_size = Size::default();
        if !unsafe {
            AXValueGetValue(
                position.as_ptr(),
                AX_VALUE_CG_POINT,
                (&mut point as *mut Point).cast(),
            )
        } || !unsafe {
            AXValueGetValue(
                size.as_ptr(),
                AX_VALUE_CG_SIZE,
                (&mut value_size as *mut Size).cast(),
            )
        } {
            return None;
        }
        Some(Bounds {
            x: point.x,
            y: point.y,
            width: value_size.width,
            height: value_size.height,
        })
    }

    fn cf_string(value: &str) -> Result<OwnedCf, String> {
        let value = CString::new(value).map_err(|_| "invalid accessibility key".to_owned())?;
        OwnedCf::from_create(unsafe {
            CFStringCreateWithCString(ptr::null(), value.as_ptr(), UTF8_ENCODING)
        })
        .ok_or_else(|| "accessibility key allocation failed".to_owned())
    }

    fn cf_to_string(value: CFTypeRef) -> Option<String> {
        if value.is_null() || unsafe { CFGetTypeID(value) } != unsafe { CFStringGetTypeID() } {
            return None;
        }
        let length = unsafe { CFStringGetLength(value) };
        let capacity =
            unsafe { CFStringGetMaximumSizeForEncoding(length, UTF8_ENCODING) }.checked_add(1)?;
        if capacity <= 0 {
            return Some(String::new());
        }
        let mut bytes = vec![0_u8; capacity as usize];
        if !unsafe { CFStringGetCString(value, bytes.as_mut_ptr().cast(), capacity, UTF8_ENCODING) }
        {
            return None;
        }
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8(bytes[..end].to_vec()).ok()
    }

    #[cfg(test)]
    mod tests {
        use super::{
            Bounds, NodeMetadata, button_score, cf_string, cf_to_string, editor_score,
            is_empty_editor_value,
        };

        #[test]
        fn follow_up_editor_beats_search() {
            let window = Bounds {
                x: 0.0,
                y: 0.0,
                width: 900.0,
                height: 700.0,
            };
            let composer = NodeMetadata {
                role: "AXTextArea".into(),
                placeholder: "Ask for follow-up changes".into(),
                ..Default::default()
            };
            let search = NodeMetadata {
                role: "AXTextField".into(),
                placeholder: "Search tasks".into(),
                ..Default::default()
            };
            let bottom = Bounds {
                x: 250.0,
                y: 610.0,
                width: 520.0,
                height: 48.0,
            };
            assert!(
                editor_score(&composer, bottom, window) > editor_score(&search, bottom, window)
            );
        }

        #[test]
        fn current_codex_composer_value_is_recognized_without_geometry() {
            let composer = NodeMetadata {
                role: "AXTextArea".into(),
                value: "Do anything".into(),
                ..Default::default()
            };
            assert!(
                editor_score(&composer, Bounds::default(), Bounds::default()) >= 95,
                "the current Codex composer exposes its prompt only through AXValue"
            );
        }

        #[test]
        fn current_codex_placeholder_is_not_treated_as_a_user_draft() {
            assert!(is_empty_editor_value(""));
            assert!(is_empty_editor_value("ask anything"));
            assert!(is_empty_editor_value("Do anything"));
            assert!(!is_empty_editor_value("keep this user draft"));
        }

        #[test]
        fn localized_send_button_is_recognized() {
            let button = NodeMetadata {
                role: "AXButton".into(),
                description: "发送".into(),
                ..Default::default()
            };
            let editor = Bounds {
                x: 100.0,
                y: 500.0,
                width: 500.0,
                height: 60.0,
            };
            let bounds = Bounds {
                x: 560.0,
                y: 510.0,
                width: 32.0,
                height: 32.0,
            };
            assert!(button_score(&button, bounds, editor) >= 120);
        }

        #[test]
        fn accessibility_strings_preserve_unicode() {
            let value = cf_string("ab😀cd").unwrap();
            assert_eq!(cf_to_string(value.as_ptr()).as_deref(), Some("ab😀cd"));
        }

    }
}
