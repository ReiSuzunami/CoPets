use std::{
    collections::BTreeSet,
    net::TcpListener,
    path::Path,
    process::{Child, Command},
};

const CODEX_BUNDLE: &str = "/Applications/ChatGPT.app";
const CODEX_EXECUTABLE: &str = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT";
const AUTOMATIC_PORT_START: u16 = 49_152;
const AUTOMATIC_PORT_COUNT: u16 = 16_384;

pub(crate) struct ManagedCodexProcess {
    pub(crate) pid: u32,
    pub(crate) child: Child,
}

/// Native-only provenance for the one CDP endpoint CoPets currently tracks.
/// The WebView receives neither this value nor the endpoint itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CdpEndpointOrigin {
    CoPetsLaunched,
    UserAttached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CdpEndpoint {
    pub(crate) process_id: u32,
    pub(crate) port: u16,
    pub(crate) origin: CdpEndpointOrigin,
}

impl CdpEndpoint {
    pub(crate) const fn launched(process_id: u32, port: u16) -> Self {
        Self {
            process_id,
            port,
            origin: CdpEndpointOrigin::CoPetsLaunched,
        }
    }

    pub(crate) const fn attached(process_id: u32, port: u16) -> Self {
        Self {
            process_id,
            port,
            origin: CdpEndpointOrigin::UserAttached,
        }
    }
}

fn launch_error() -> String {
    "CoPets could not launch Codex with its local bridge. Check that Codex is installed, then retry."
        .to_owned()
}

fn restart_unavailable_error() -> String {
    "No running Codex could be restarted. Use Launch Codex instead.".to_owned()
}

fn restart_multiple_processes_error() -> String {
    "CoPets found multiple Codex App processes. Close extras yourself, then retry.".to_owned()
}

fn restart_existing_cdp_error() -> String {
    "Codex already exposes a local CDP bridge. Use Connect existing instead.".to_owned()
}

fn restart_termination_error() -> String {
    "CoPets could not request Codex to close. Close it yourself, then use Launch Codex.".to_owned()
}

fn port_error() -> String {
    "Choose an unused local port from 1024 to 65535.".to_owned()
}

fn port_is_available(port: u16) -> bool {
    TcpListener::bind(("127.0.0.1", port)).is_ok()
}

pub(crate) fn validate_port(port: u16) -> Result<(), String> {
    (port >= 1024).then_some(()).ok_or_else(port_error)
}

pub(crate) fn reserve_port(custom_port: Option<u16>) -> Result<u16, String> {
    if let Some(port) = custom_port {
        if validate_port(port).is_err() || !port_is_available(port) {
            return Err(port_error());
        }
        return Ok(port);
    }
    for _ in 0..64 {
        let offset = (uuid::Uuid::new_v4().as_u128() % u128::from(AUTOMATIC_PORT_COUNT)) as u16;
        let port = AUTOMATIC_PORT_START + offset;
        if port_is_available(port) {
            return Ok(port);
        }
    }
    Err("CoPets could not reserve a local bridge port. Retry launch.".to_owned())
}

fn process_table() -> Option<String> {
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,uid=,command="])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

/// Returns same-user official App processes and their optional CDP ports from
/// the macOS process table. `pgrep -x` matches a mutable display name on macOS,
/// so it cannot be the authority for a direct CDP attachment.
fn official_codex_processes_from_ps(output: &str, expected_uid: u32) -> Vec<(u32, Option<u16>)> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let process_id = columns.next()?.parse::<u32>().ok()?;
            let uid = columns.next()?.parse::<u32>().ok()?;
            let executable = columns.next()?;
            if uid != expected_uid || executable != CODEX_EXECUTABLE {
                return None;
            }
            let port = columns.find_map(|argument| {
                argument
                    .strip_prefix("--remote-debugging-port=")
                    .and_then(|value| value.parse::<u16>().ok())
                    .filter(|port| validate_port(*port).is_ok())
            });
            Some((process_id, port))
        })
        .collect()
}

pub(crate) fn running_codex_process() -> Result<bool, String> {
    let table = process_table().ok_or_else(launch_error)?;
    Ok(!official_codex_processes_from_ps(&table, unsafe { libc::geteuid() }).is_empty())
}

fn restart_target_from_processes(processes: &[(u32, Option<u16>)]) -> Result<u32, String> {
    match processes {
        [] => Err(restart_unavailable_error()),
        [(process_id, None)] => Ok(*process_id),
        [(_, Some(_))] => Err(restart_existing_cdp_error()),
        _ => Err(restart_multiple_processes_error()),
    }
}

/// Returns the one same-user official Codex App that can safely be restarted
/// into bridge mode. The caller must still revalidate immediately before the
/// terminating signal because the native process table is only a snapshot.
pub(crate) fn restartable_codex_process() -> Result<u32, String> {
    let table = process_table().ok_or_else(launch_error)?;
    let expected_uid = unsafe { libc::geteuid() };
    let processes = official_codex_processes_from_ps(&table, expected_uid);
    let process_id = restart_target_from_processes(&processes)?;
    match process_command(process_id)
        .as_deref()
        .and_then(|output| official_codex_process_cdp_flag(output, expected_uid))
    {
        Some(false) => Ok(process_id),
        Some(true) => Err(restart_existing_cdp_error()),
        None => Err(restart_unavailable_error()),
    }
}

fn launch_args(port: u16) -> [String; 2] {
    [
        "--remote-debugging-address=127.0.0.1".to_owned(),
        format!("--remote-debugging-port={port}"),
    ]
}

fn ownership_error() -> String {
    "CoPets could not verify ownership of the local Codex bridge.".to_owned()
}

fn listener_pids_from_lsof(output: &str, port: u16) -> BTreeSet<u32> {
    let expected_listener = format!("127.0.0.1:{port}");
    let mut current_pid = None;
    let mut listener_pids = BTreeSet::new();
    for line in output.lines() {
        let Some((field, value)) = line.split_at_checked(1) else {
            continue;
        };
        match field {
            "p" => current_pid = value.parse::<u32>().ok(),
            "n" if value == expected_listener => {
                if let Some(pid) = current_pid {
                    listener_pids.insert(pid);
                }
            }
            _ => {}
        }
    }
    listener_pids
}

fn process_command_matches_codex(output: &str, port: u16, expected_uid: u32) -> bool {
    cdp_port_from_process_command(output, expected_uid) == Some(port)
}

fn cdp_port_from_process_command(output: &str, expected_uid: u32) -> Option<u16> {
    let Some(line) = output.lines().find(|line| !line.trim().is_empty()) else {
        return None;
    };
    let mut columns = line.split_whitespace();
    let Some(uid) = columns.next().and_then(|value| value.parse::<u32>().ok()) else {
        return None;
    };
    let Some(executable) = columns.next() else {
        return None;
    };
    if uid != expected_uid || executable != CODEX_EXECUTABLE {
        return None;
    }
    columns.find_map(|argument| {
        argument
            .strip_prefix("--remote-debugging-port=")
            .and_then(|value| value.parse::<u16>().ok())
            .filter(|port| validate_port(*port).is_ok())
    })
}

fn official_codex_process_cdp_flag(output: &str, expected_uid: u32) -> Option<bool> {
    let Some(line) = output.lines().find(|line| !line.trim().is_empty()) else {
        return None;
    };
    let mut columns = line.split_whitespace();
    let Some(uid) = columns.next().and_then(|value| value.parse::<u32>().ok()) else {
        return None;
    };
    let Some(executable) = columns.next() else {
        return None;
    };
    (uid == expected_uid && executable == CODEX_EXECUTABLE)
        .then(|| columns.any(|argument| argument.starts_with("--remote-debugging-port=")))
}

fn process_command_is_official_non_cdp_codex(output: &str, expected_uid: u32) -> bool {
    official_codex_process_cdp_flag(output, expected_uid) == Some(false)
}

fn listener_pids(port: u16) -> Result<BTreeSet<u32>, String> {
    let status = Command::new("/usr/sbin/lsof")
        .args([
            "-nP",
            &format!("-iTCP:{port}"),
            "-sTCP:LISTEN",
            "-Fpn",
            "-w",
        ])
        .output()
        .map_err(|_| ownership_error())?;
    if !status.status.success() {
        return Err(ownership_error());
    }
    let output = String::from_utf8(status.stdout).map_err(|_| ownership_error())?;
    let pids = listener_pids_from_lsof(&output, port);
    (!pids.is_empty())
        .then_some(pids)
        .ok_or_else(ownership_error)
}

fn process_command(pid: u32) -> Option<String> {
    let output = Command::new("/bin/ps")
        .args(["-p", &pid.to_string(), "-o", "uid=", "-o", "command="])
        .output();
    let Ok(output) = output else {
        return None;
    };
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())
        .flatten()
}

fn process_is_codex_cdp_owner(pid: u32, port: u16) -> bool {
    process_command(pid).is_some_and(|value| {
        process_command_matches_codex(&value, port, unsafe { libc::geteuid() })
    })
}

/// Requests a graceful close of exactly one revalidated, same-user official
/// Codex App process. This deliberately never escalates to a force kill.
pub(crate) fn request_codex_restart(process_id: u32) -> Result<(), String> {
    let expected_uid = unsafe { libc::geteuid() };
    let is_exact_restart_target = process_command(process_id)
        .is_some_and(|output| process_command_is_official_non_cdp_codex(&output, expected_uid));
    if !is_exact_restart_target {
        return Err(restart_termination_error());
    }
    let process_id = i32::try_from(process_id).map_err(|_| restart_termination_error())?;
    (unsafe { libc::kill(process_id, libc::SIGTERM) } == 0)
        .then_some(())
        .ok_or_else(restart_termination_error)
}

/// Checks whether the exact old official App PID is still present. A reused PID
/// for another executable does not block the subsequent, guarded bridge launch.
pub(crate) fn official_codex_process_is_running(process_id: u32) -> Result<bool, String> {
    let table = process_table().ok_or_else(launch_error)?;
    Ok(
        official_codex_processes_from_ps(&table, unsafe { libc::geteuid() })
            .into_iter()
            .any(|(candidate, _)| candidate == process_id),
    )
}

fn automatic_codex_candidates() -> Result<Vec<CdpEndpoint>, String> {
    let table = process_table().ok_or_else(ownership_error)?;
    Ok(
        official_codex_processes_from_ps(&table, unsafe { libc::geteuid() })
            .into_iter()
            .filter_map(|(process_id, port)| {
                port.map(|port| CdpEndpoint::attached(process_id, port))
            })
            .filter(|endpoint| codex_process_owns_port(endpoint.process_id, endpoint.port).is_ok())
            .collect(),
    )
}

/// Verifies an exact, same-user official App process still owns an IPv4 loopback
/// CDP listener. This intentionally rejects an arbitrary local DevTools port.
pub(crate) fn codex_process_owns_port(pid: u32, port: u16) -> Result<(), String> {
    if pid == 0 || validate_port(port).is_err() {
        return Err(ownership_error());
    }
    let pids = listener_pids(port)?;
    (pids.contains(&pid) && process_is_codex_cdp_owner(pid, port))
        .then_some(())
        .ok_or_else(ownership_error)
}

/// Finds the one same-user official Codex App that currently owns the explicit
/// user-selected loopback CDP port. Child helpers inheriting the listener do
/// not qualify because their executable is not the official App executable.
pub(crate) fn discover_existing_codex(port: Option<u16>) -> Result<CdpEndpoint, String> {
    let candidates = if let Some(port) = port {
        validate_port(port)?;
        listener_pids(port)?
            .into_iter()
            .filter(|pid| process_is_codex_cdp_owner(*pid, port))
            .map(|process_id| CdpEndpoint::attached(process_id, port))
            .collect::<Vec<_>>()
    } else {
        automatic_codex_candidates()?
    };
    match candidates.as_slice() {
        [endpoint] => Ok(*endpoint),
        _ => Err(
            "CoPets could not verify one local Codex CDP bridge. Use its loopback port explicitly, then retry."
                .to_owned(),
        ),
    }
}

pub(crate) fn launch_codex(port: u16) -> Result<ManagedCodexProcess, String> {
    if !Path::new(CODEX_BUNDLE).is_dir() || !Path::new(CODEX_EXECUTABLE).is_file() {
        return Err(launch_error());
    }
    let child = Command::new(CODEX_EXECUTABLE)
        .args(launch_args(port))
        .spawn()
        .map_err(|_| launch_error())?;
    Ok(ManagedCodexProcess {
        pid: child.id(),
        child,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AUTOMATIC_PORT_START, CODEX_EXECUTABLE, cdp_port_from_process_command, launch_args,
        listener_pids_from_lsof, official_codex_process_cdp_flag, official_codex_processes_from_ps,
        process_command_is_official_non_cdp_codex, process_command_matches_codex, reserve_port,
        restart_target_from_processes,
    };

    #[test]
    fn automatic_port_stays_in_dynamic_high_range() {
        assert!(reserve_port(None).unwrap() >= AUTOMATIC_PORT_START);
    }

    #[test]
    fn privileged_port_is_not_accepted_as_custom_bridge_port() {
        assert!(reserve_port(Some(80)).is_err());
    }

    #[test]
    fn launcher_never_passes_a_non_loopback_debugging_address() {
        let args = launch_args(52_000);
        assert_eq!(args[0], "--remote-debugging-address=127.0.0.1");
        assert_eq!(args[1], "--remote-debugging-port=52000");
    }

    #[test]
    fn listener_discovery_accepts_only_the_explicit_ipv4_loopback_port() {
        let output =
            "p4242\nf56\nn127.0.0.1:52001\np4243\nf56\nn*:52001\np4244\nf56\nn[::1]:52001\n";
        assert_eq!(
            listener_pids_from_lsof(output, 52_001)
                .into_iter()
                .collect::<Vec<_>>(),
            vec![4242]
        );
    }

    #[test]
    fn existing_listener_requires_the_same_user_official_codex_command_with_that_port() {
        let valid = format!(
            " 501 {CODEX_EXECUTABLE} --remote-debugging-address=127.0.0.1 --remote-debugging-port=52001\n"
        );
        assert!(process_command_matches_codex(&valid, 52_001, 501));
        assert!(!process_command_matches_codex(&valid, 52_002, 501));
        assert!(!process_command_matches_codex(
            " 501 /Applications/Other.app/Contents/MacOS/Other --remote-debugging-port=52001\n",
            52_001,
            501,
        ));
        assert!(!process_command_matches_codex(&valid, 52_001, 502));
        assert_eq!(cdp_port_from_process_command(&valid, 501), Some(52_001));
    }

    #[test]
    fn automatic_discovery_uses_the_official_process_table_not_a_display_name() {
        let table = format!(
            " 81418 501 {CODEX_EXECUTABLE} --remote-debugging-address=127.0.0.1 --remote-debugging-port=52001\n\
             81611 501 /Applications/Other.app/Contents/MacOS/Other --remote-debugging-port=52001\n\
             81419 501 {CODEX_EXECUTABLE}\n\
             81420 502 {CODEX_EXECUTABLE} --remote-debugging-port=52002\n"
        );

        assert_eq!(
            official_codex_processes_from_ps(&table, 501),
            vec![(81_418, Some(52_001)), (81_419, None)]
        );
    }

    #[test]
    fn restart_requires_exactly_one_non_cdp_official_process() {
        assert_eq!(restart_target_from_processes(&[(81_418, None)]), Ok(81_418));
        assert!(
            restart_target_from_processes(&[])
                .unwrap_err()
                .contains("Use Launch Codex")
        );
        assert!(
            restart_target_from_processes(&[(81_418, Some(52_001))])
                .unwrap_err()
                .contains("Connect existing")
        );
        assert!(
            restart_target_from_processes(&[(81_418, None), (81_419, None)])
                .unwrap_err()
                .contains("multiple Codex")
        );
    }

    #[test]
    fn restart_revalidation_refuses_any_remote_debugging_argument() {
        let plain = format!(" 501 {CODEX_EXECUTABLE} --some-other-flag\n");
        let cdp = format!(" 501 {CODEX_EXECUTABLE} --remote-debugging-port=52001\n");
        let malformed_cdp = format!(" 501 {CODEX_EXECUTABLE} --remote-debugging-port=not-a-port\n");

        assert!(process_command_is_official_non_cdp_codex(&plain, 501));
        assert!(!process_command_is_official_non_cdp_codex(&cdp, 501));
        assert!(!process_command_is_official_non_cdp_codex(
            &malformed_cdp,
            501
        ));
        assert_eq!(official_codex_process_cdp_flag(&plain, 501), Some(false));
        assert_eq!(
            official_codex_process_cdp_flag(&malformed_cdp, 501),
            Some(true)
        );
    }
}
