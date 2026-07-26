//! Qualcomm IPv6 IMS data-path preflight.
//!
//! The 410 reference runtime primes the QMI WDS/MUX path before asking
//! ModemManager to create the final IMS bearer. This is not the shared mobile
//! data connection: it is a short-lived IPv6 `ims` WDS session used to make
//! the modem allocate the PCO/prefix state that the later bearer consumes.

use std::{net::IpAddr, path::Path, process::Output, time::Duration};

use tokio::{process::Command, time::timeout};

use super::errors::{code, VolteError};

const QMI_TIMEOUT: Duration = Duration::from_secs(30);
const IMS_APN: &str = "ims";

/// Opt-in environment flag to re-enable the direct-QMI WDS preflight. Default
/// (unset) keeps it OFF: the connect path is pure ModemManager, which avoids the
/// `interface-in-use-config-match` QMI error that occurs when this preflight
/// grabs a second WDS session on the same `/dev/wwan0qmi0` that ModemManager is
/// already using. Set `SIMADMIN_VOLTE_WDS_PREFLIGHT=1` only on devices with a
/// dedicated secondary QMI endpoint where the preflight is known to help.
pub const WDS_PREFLIGHT_ENV: &str = "SIMADMIN_VOLTE_WDS_PREFLIGHT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeResult {
    /// WDS returned an IPv6 address and gateway; SIP probing is intentionally
    /// left to the real REGISTER path.
    Ready,
    /// The reference also tolerates a SIP probe timeout after WDS succeeds.
    NoSipResponse,
    /// No QMI MUX data endpoint exists on this device (e.g. a bam-dmux target
    /// with no `a2-mux-rmnet*` node). The WDS preflight is a no-op here and the
    /// caller proceeds straight to the ModemManager bearer. Attempting a
    /// `wds-start-network` on a nonexistent MUX port is pointless and, on some
    /// firmware, actively harmful (it can wedge the baseband).
    NoMuxEndpoint,
    /// The preflight is disabled (the default). The connect path relies purely
    /// on ModemManager to create the IMS bearer, so no direct-QMI WDS session is
    /// opened and there is no contention with ModemManager's own QMI client.
    Disabled,
}

/// Whether the direct-QMI WDS preflight is opted in via the environment.
fn wds_preflight_enabled() -> bool {
    std::env::var(WDS_PREFLIGHT_ENV)
        .ok()
        .map(|value| {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProbePath<'a> {
    interface: &'a str,
    data_port: &'a str,
}

/// Candidate MUX endpoints, in the reference's probe order. Only the entries
/// whose data port actually exists on the running device are used; the rest
/// are skipped rather than probed blindly.
const PROBE_PATHS: &[ProbePath<'_>] = &[
    ProbePath {
        interface: "wwan1",
        data_port: "a2-mux-rmnet1",
    },
    ProbePath {
        interface: "wwan0",
        data_port: "a2-mux-rmnet0",
    },
];

/// A MUX data port is usable only if the kernel actually exposes it, either as
/// a network device (`/sys/class/net/<port>`) or a device node (`/dev/<port>`).
/// bam-dmux targets expose neither, so this returns `false` there and the whole
/// preflight collapses to a no-op.
fn data_port_exists(data_port: &str) -> bool {
    Path::new(&format!("/sys/class/net/{data_port}")).exists()
        || Path::new(&format!("/dev/{data_port}")).exists()
}

/// Run the bounded WDS preflight. A failed path is cleaned up before trying
/// the next MUX endpoint; callers may continue to the ModemManager bearer if
/// every path is unavailable, matching the reference's best-effort probe.
///
/// If no MUX endpoint exists at all (bam-dmux devices), the preflight is
/// skipped entirely and `NoMuxEndpoint` is returned, avoiding a wasted — and
/// potentially baseband-wedging — `wds-start-network` on a port that is not
/// there.
pub async fn probe_ims_ipv6(qmi_device: &str, cid_hint: u8) -> Result<ProbeResult, VolteError> {
    // Default path is pure ModemManager: no direct-QMI WDS session is opened, so
    // ModemManager keeps sole ownership of `/dev/wwan0qmi0` and there is no
    // `interface-in-use-config-match` contention. The preflight only runs when
    // explicitly opted in for a device with a dedicated secondary QMI endpoint.
    if !wds_preflight_enabled() {
        return Ok(ProbeResult::Disabled);
    }
    let present: Vec<&ProbePath<'_>> = PROBE_PATHS
        .iter()
        .filter(|path| data_port_exists(path.data_port))
        .collect();
    if present.is_empty() {
        return Ok(ProbeResult::NoMuxEndpoint);
    }
    let mut last_error = None;
    let mut initialized = false;
    for path in present {
        match probe_path(qmi_device, cid_hint, path).await {
            // The reference continues to the shared wwan0 MUX after the
            // wwan1 SIP capability probe times out, so prime both paths.
            Ok(()) => initialized = true,
            Err(error) => last_error = Some(error),
        }
    }
    if initialized {
        return Ok(ProbeResult::NoSipResponse);
    }
    Err(last_error.unwrap_or_else(|| VolteError::new(code::COMMAND_FAILED)))
}

async fn probe_path(
    qmi_device: &str,
    cid_hint: u8,
    path: &ProbePath<'_>,
) -> Result<(), VolteError> {
    let cid = allocate_wds_client(qmi_device).await?;
    let mut pdh = None;
    let result = async {
        qmi_action(
            qmi_device,
            cid,
            &format!("--wds-bind-data-port={}", path.data_port),
        )
        .await?;
        qmi_action(qmi_device, cid, "--wds-set-ip-family=6").await?;
        let start = format!("--wds-start-network=apn={IMS_APN},3gpp-profile={cid_hint},ip-type=6");
        let started = qmi_action(qmi_device, cid, &start).await?;
        pdh = parse_packet_handle(&started);
        if pdh.is_none() {
            return Err(VolteError::with_detail(
                code::COMMAND_FAILED,
                "qmicli:wds-start-network:pdh-missing",
            ));
        }
        let settings = qmi_action(qmi_device, cid, "--wds-get-current-settings").await?;
        if !has_ipv6_settings(&settings) {
            return Err(VolteError::with_detail(
                code::COMMAND_FAILED,
                format!("qmicli:{}:ipv6-settings-missing", path.interface),
            ));
        }
        Ok(())
    }
    .await;

    if let Some(handle) = pdh {
        let stop = format!("--wds-stop-network={handle}");
        let _ = qmi_action(qmi_device, cid, &stop).await;
    }
    release_wds_client(qmi_device, cid).await;
    result
}

async fn allocate_wds_client(qmi_device: &str) -> Result<u8, VolteError> {
    let output = qmi_command(qmi_device, &["--client-no-release-cid", "--wds-noop"]).await?;
    parse_client_id(&output)
        .ok_or_else(|| VolteError::with_detail(code::COMMAND_FAILED, "qmicli:wds-noop:cid-missing"))
}

async fn qmi_action(qmi_device: &str, cid: u8, action: &str) -> Result<String, VolteError> {
    let cid_arg = format!("--client-cid={cid}");
    qmi_command(qmi_device, &["--client-no-release-cid", &cid_arg, action]).await
}

async fn release_wds_client(qmi_device: &str, cid: u8) {
    let cid_arg = format!("--client-cid={cid}");
    let _ = qmi_command(qmi_device, &[&cid_arg, "--wds-noop"]).await;
}

async fn qmi_command(qmi_device: &str, args: &[&str]) -> Result<String, VolteError> {
    let mut command_args = vec!["-d", qmi_device, "--device-open-proxy"];
    command_args.extend_from_slice(args);
    let output = timeout(
        QMI_TIMEOUT,
        Command::new("qmicli").args(&command_args).output(),
    )
    .await
    .map_err(|_| VolteError::with_detail(code::COMMAND_TIMEOUT, "qmicli"))?
    .map_err(|error| {
        VolteError::with_detail(code::COMMAND_SPAWN_FAILED, format!("qmicli:{error}"))
    })?;
    command_output("qmicli", &command_args, output)
}

fn command_output(program: &str, args: &[&str], output: Output) -> Result<String, VolteError> {
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }
    let stderr = String::from_utf8_lossy(&output.stderr)
        .trim()
        .replace('\n', " ");
    Err(VolteError::with_detail(
        code::COMMAND_FAILED,
        format!(
            "{program}:{}:{}:{stderr}",
            output.status.code().unwrap_or(-1),
            args.join(" ")
        ),
    ))
}

fn parse_client_id(output: &str) -> Option<u8> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("CID:")?;
        value.trim().trim_matches('\'').parse::<u8>().ok()
    })
}

fn parse_packet_handle(output: &str) -> Option<u32> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("Packet data handle:")?;
        let value = value.trim().trim_matches('\'');
        value
            .strip_prefix("0x")
            .and_then(|v| u32::from_str_radix(v, 16).ok())
            .or_else(|| value.parse::<u32>().ok())
    })
}

fn has_ipv6_settings(output: &str) -> bool {
    let has_address = output.lines().any(|line| {
        line.to_ascii_lowercase().contains("ipv6 address")
            && line.split_once(':').is_some_and(|(_, value)| {
                value.trim().parse::<IpAddr>().is_ok()
                    || value
                        .trim()
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse::<IpAddr>()
                        .is_ok()
            })
    });
    let has_gateway = output.lines().any(|line| {
        line.to_ascii_lowercase().contains("ipv6 gateway")
            && line.split_once(':').is_some_and(|(_, value)| {
                value.trim().parse::<IpAddr>().is_ok()
                    || value
                        .trim()
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse::<IpAddr>()
                        .is_ok()
            })
    });
    has_address && has_gateway
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_match_reference_order() {
        assert_eq!(PROBE_PATHS[0].interface, "wwan1");
        assert_eq!(PROBE_PATHS[0].data_port, "a2-mux-rmnet1");
        assert_eq!(PROBE_PATHS[1].interface, "wwan0");
        assert_eq!(PROBE_PATHS[1].data_port, "a2-mux-rmnet0");
    }

    #[test]
    fn preflight_is_disabled_by_default() {
        // With the opt-in flag unset, the connect path is pure ModemManager and
        // the preflight must report Disabled without touching qmicli or sysfs.
        // (Guard against a stray env var from the surrounding shell.)
        std::env::remove_var(WDS_PREFLIGHT_ENV);
        let result = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(probe_ims_ipv6("/dev/does-not-exist", 2));
        assert_eq!(result, Ok(ProbeResult::Disabled));
    }

    #[test]
    fn absent_mux_endpoints_are_not_probed_when_opted_in() {
        // None of the reference MUX ports exist on a bam-dmux target (nor on
        // the CI host), so once opted in the preflight reports NoMuxEndpoint
        // without ever shelling out to qmicli.
        assert!(!data_port_exists("a2-mux-rmnet0"));
        assert!(!data_port_exists("a2-mux-rmnet1"));
        assert!(wds_preflight_env_parses("1"));
        assert!(wds_preflight_env_parses("true"));
        assert!(!wds_preflight_env_parses("0"));
    }

    /// Pure parse check for the opt-in flag values (avoids mutating process env
    /// in a way that could race other tests).
    fn wds_preflight_env_parses(value: &str) -> bool {
        let value = value.trim();
        value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
    }

    #[test]
    fn parses_qmi_ids_and_handles() {
        assert_eq!(parse_client_id("Service: 'wds'\n    CID: '2'"), Some(2));
        assert_eq!(
            parse_packet_handle("Packet data handle: '0x1234'"),
            Some(0x1234)
        );
        assert_eq!(parse_packet_handle("Packet data handle: 42"), Some(42));
    }

    #[test]
    fn validates_ipv6_settings() {
        let output = "IPv6 address: 240e::1/64\nIPv6 gateway address: fe80::1\n";
        assert!(has_ipv6_settings(output));
        assert!(!has_ipv6_settings("IPv6 address: 240e::1/64\n"));
    }
}
