use anyhow::{Context, Result, bail};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionInfo {
    pub path: String,
    pub created: String,
    pub pid: Option<u32>,
    pub owner: String,
    pub config_name: String,
    pub device: String,
    pub status: String,
    pub connected_to: String,
}

impl SessionInfo {
    pub fn is_connected(&self) -> bool {
        let s = self.status.to_lowercase();
        s.contains("connected") || s.contains("active") || s.contains("client active")
    }

    pub fn matches_profile(&self, profile_name: &str, profile_path: &std::path::Path) -> bool {
        let clean_s = if let Some(idx) = self.config_name.find('(') {
            self.config_name[..idx].trim()
        } else {
            self.config_name.trim()
        };

        if clean_s.is_empty() {
            return false;
        }

        // 1. Direct path equality
        if clean_s == profile_path.to_string_lossy() {
            return true;
        }

        // 2. Filename equality (e.g. "/path/to/aws-sam.ovpn" -> "aws-sam.ovpn")
        let s_filename = std::path::Path::new(clean_s)
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or(clean_s);

        if s_filename.eq_ignore_ascii_case(profile_name) {
            return true;
        }

        // 3. Stem equality without extension (e.g. "aws-sam" vs "aws-sam.ovpn")
        let stem_s = std::path::Path::new(clean_s)
            .file_stem()
            .and_then(|f| f.to_str())
            .unwrap_or(clean_s);
        let stem_p = std::path::Path::new(profile_name)
            .file_stem()
            .and_then(|f| f.to_str())
            .unwrap_or(profile_name);

        if stem_s.eq_ignore_ascii_case(stem_p) {
            return true;
        }

        // 4. Substring / ends_with check
        if clean_s.ends_with(profile_name) || profile_name.starts_with(clean_s) {
            return true;
        }

        false
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AuthRequest {
    pub session_path: String,
    pub auth_req_id: String,
    pub auth_status: String,
    pub auth_url: Option<String>,
}

pub fn parse_sessions_list(output: &str) -> Vec<SessionInfo> {
    let mut sessions = Vec::new();
    let mut current_session: Option<SessionInfo> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("---") {
            if let Some(session) = current_session.take() {
                if !session.path.is_empty() || !session.config_name.is_empty() {
                    sessions.push(session);
                }
            }
            current_session = Some(SessionInfo::default());
            continue;
        }

        if let Some(ref mut session) = current_session {
            // Handle multi-column lines from openvpn3 output:
            // e.g. "Created: 2026-09-20 22:49:46                       PID: 1507962"
            if trimmed.contains("PID:") && trimmed.contains("Created:") {
                if let Some((c_part, pid_part)) = trimmed.split_once("PID:") {
                    let created_val = if let Some((_, v)) = c_part.split_once(':') {
                        v.trim()
                    } else {
                        c_part.trim()
                    };
                    session.created = created_val.to_string();
                    session.pid = pid_part.trim().parse::<u32>().ok();
                }
                continue;
            }

            // e.g. "Owner: dizba                                  Device: tun0"
            if trimmed.contains("Device:") && trimmed.contains("Owner:") {
                if let Some((o_part, dev_part)) = trimmed.split_once("Device:") {
                    let owner_val = if let Some((_, v)) = o_part.split_once(':') {
                        v.trim()
                    } else {
                        o_part.trim()
                    };
                    session.owner = owner_val.to_string();
                    session.device = dev_part.trim().to_string();
                }
                continue;
            }

            // Standard single key:value lines
            if let Some((key, val)) = trimmed.split_once(':') {
                let key = key.trim().to_lowercase();
                let mut val = val.trim().to_string();
                match key.as_str() {
                    "path" => session.path = val,
                    "created" => session.created = val,
                    "pid" => session.pid = val.parse::<u32>().ok(),
                    "owner" => session.owner = val,
                    "config name" => {
                        // Strip trailing parenthetical info like " (Config not available)"
                        if let Some(idx) = val.find('(') {
                            val = val[..idx].trim().to_string();
                        }
                        session.config_name = val;
                    }
                    "device" => session.device = val,
                    "status" => session.status = val,
                    "connected to" => session.connected_to = val,
                    _ => {}
                }
            }
        }
    }

    if let Some(session) = current_session {
        if !session.path.is_empty() || !session.config_name.is_empty() {
            sessions.push(session);
        }
    }

    sessions
}

pub fn parse_session_auth(output: &str) -> Vec<AuthRequest> {
    let mut requests = Vec::new();
    let mut current_req: Option<AuthRequest> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("---") {
            if let Some(req) = current_req.take() {
                if !req.auth_req_id.is_empty() || !req.session_path.is_empty() {
                    requests.push(req);
                }
            }
            current_req = Some(AuthRequest::default());
            continue;
        }

        if let Some(ref mut req) = current_req {
            if let Some((key, val)) = trimmed.split_once(':') {
                let key = key.trim().to_lowercase();
                let val = val.trim().to_string();
                match key.as_str() {
                    "session path" => req.session_path = val,
                    "auth request" => req.auth_req_id = val,
                    "auth status" => req.auth_status = val,
                    "auth url" => req.auth_url = Some(val),
                    _ => {}
                }
            }
        }
    }

    if let Some(req) = current_req {
        if !req.auth_req_id.is_empty() || !req.session_path.is_empty() {
            requests.push(req);
        }
    }

    requests
}

/// Query active sessions using `openvpn3 sessions-list`
pub async fn list_sessions() -> Result<Vec<SessionInfo>> {
    let output = Command::new("openvpn3")
        .arg("sessions-list")
        .output()
        .await
        .context("Failed to execute `openvpn3 sessions-list`")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_sessions_list(&stdout))
}

/// Query pending 2FA / interactive authentication requests using `openvpn3 session-auth`
pub async fn check_pending_auth() -> Result<Vec<AuthRequest>> {
    let output = Command::new("openvpn3")
        .arg("session-auth")
        .output()
        .await
        .context("Failed to execute `openvpn3 session-auth`")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_session_auth(&stdout))
}

pub fn parse_session_stats_json(output: &str) -> Option<(u64, u64)> {
    let val: serde_json::Value = serde_json::from_str(output).ok()?;
    let mut rx = None;
    let mut tx = None;

    fn search_val(v: &serde_json::Value, rx: &mut Option<u64>, tx: &mut Option<u64>) {
        match v {
            serde_json::Value::Object(map) => {
                for (k, val) in map {
                    let k_upper = k.to_uppercase();
                    if k_upper == "TUN_BYTES_IN"
                        || k_upper == "BYTES_IN"
                        || k_upper == "BYTESIN"
                        || k_upper == "IN_BYTES"
                    {
                        if let Some(n) = val.as_u64() {
                            *rx = Some(n);
                        }
                    } else if k_upper == "TUN_BYTES_OUT"
                        || k_upper == "BYTES_OUT"
                        || k_upper == "BYTESOUT"
                        || k_upper == "OUT_BYTES"
                    {
                        if let Some(n) = val.as_u64() {
                            *tx = Some(n);
                        }
                    }
                    search_val(val, rx, tx);
                }
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    search_val(item, rx, tx);
                }
            }
            _ => {}
        }
    }

    search_val(&val, &mut rx, &mut tx);

    match (rx, tx) {
        (Some(r), Some(t)) => Some((r, t)),
        (Some(r), None) => Some((r, 0)),
        (None, Some(t)) => Some((0, t)),
        (None, None) => None,
    }
}

pub fn parse_session_stats_text(output: &str) -> Option<(u64, u64)> {
    let mut rx = None;
    let mut tx = None;

    for line in output.lines() {
        let trimmed = line.trim();
        // Support both dots format (BYTES_IN........123) and colon format (BYTES_IN: 123)
        let (key_raw, val_raw) = if let Some(idx) = trimmed.find("...") {
            (&trimmed[..idx], &trimmed[idx + 3..])
        } else if let Some((k, v)) = trimmed.split_once(':') {
            (k, v)
        } else {
            continue;
        };

        let key = key_raw.trim().trim_matches('.').to_uppercase();
        let val_clean = val_raw
            .trim()
            .trim_matches('.')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .replace(',', "");
        if let Ok(num) = val_clean.parse::<u64>() {
            if key == "TUN_BYTES_IN" || key == "BYTES_IN" || key.contains("BYTES IN") {
                rx = Some(num);
            } else if key == "TUN_BYTES_OUT" || key == "BYTES_OUT" || key.contains("BYTES OUT") {
                tx = Some(num);
            }
        }
    }

    match (rx, tx) {
        (Some(r), Some(t)) => Some((r, t)),
        (Some(r), None) => Some((r, 0)),
        (None, Some(t)) => Some((0, t)),
        (None, None) => None,
    }
}

/// Query session stats via `openvpn3 session-stats --path <path> --json`
pub async fn query_session_stats(session_path: &str) -> Option<(u64, u64)> {
    if session_path.is_empty() {
        return None;
    }

    // 1. Try with --json
    if let Ok(output) = Command::new("openvpn3")
        .arg("session-stats")
        .arg("--path")
        .arg(session_path)
        .arg("--json")
        .output()
        .await
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(stats) = parse_session_stats_json(&stdout) {
                return Some(stats);
            }
        }
    }

    // 2. Try without --json (plain text)
    if let Ok(output) = Command::new("openvpn3")
        .arg("session-stats")
        .arg("--path")
        .arg(session_path)
        .output()
        .await
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(stats) = parse_session_stats_text(&stdout) {
                return Some(stats);
            }
        }
    }

    None
}

/// Provide response / Authenticator code for a pending auth request
pub async fn provide_auth_response(auth_req_id: &str, response: &str) -> Result<String> {
    let mut cmd = Command::new("openvpn3");
    cmd.arg("session-auth").arg("--auth-req").arg(auth_req_id);

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().with_context(|| {
        format!(
            "Failed to spawn openvpn3 session-auth for req {}",
            auth_req_id
        )
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        let response_data = format!("{}\n", response);
        let _ = stdin.write_all(response_data.as_bytes()).await;
        let _ = stdin.flush().await;
    }

    let output = child
        .wait_with_output()
        .await
        .context("Failed waiting for openvpn3 session-auth")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let err_msg = if !stderr.trim().is_empty() {
            stderr.trim()
        } else if !stdout.trim().is_empty() {
            stdout.trim()
        } else {
            "Failed to provide auth response"
        };
        bail!("{}", err_msg);
    }

    let combined = format!("{}\n{}", stdout, stderr).trim().to_string();
    Ok(combined)
}

/// Start an OpenVPN 3 session in background with optional credentials and OTP piped to stdin.
pub async fn start_session(
    config_path: &str,
    username: Option<&str>,
    password: Option<&str>,
    otp_code: Option<&str>,
) -> Result<String> {
    let mut cmd = Command::new("openvpn3");
    cmd.arg("session-start")
        .arg("--config")
        .arg(config_path)
        .arg("--background");

    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn openvpn3 session-start for {}", config_path))?;

    if let (Some(u), Some(p)) = (username, password) {
        if let Some(mut stdin) = child.stdin.take() {
            let cred_data = if let Some(otp) = otp_code {
                if !otp.trim().is_empty() {
                    // Pipe username, password, and otp line by line
                    format!("{}\n{}\n{}\n", u, p, otp.trim())
                } else {
                    format!("{}\n{}\n", u, p)
                }
            } else {
                format!("{}\n{}\n", u, p)
            };

            let _ = stdin.write_all(cred_data.as_bytes()).await;
            let _ = stdin.flush().await;
        }
    }

    let output = child
        .wait_with_output()
        .await
        .context("Failed waiting for openvpn3 session-start")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let err_msg = if !stderr.trim().is_empty() {
            stderr.trim()
        } else if !stdout.trim().is_empty() {
            stdout.trim()
        } else {
            "Unknown error"
        };
        bail!("Failed to start session: {}", err_msg);
    }

    let combined = format!("{}\n{}", stdout, stderr).trim().to_string();
    Ok(combined)
}

/// Disconnect a running session via path or config name
pub async fn disconnect_session(session_path: &str) -> Result<String> {
    let output = Command::new("openvpn3")
        .arg("session-manage")
        .arg("--path")
        .arg(session_path)
        .arg("--disconnect")
        .output()
        .await
        .context("Failed to execute `openvpn3 session-manage --disconnect`")?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if !output.status.success() {
        let err_msg = if !stderr.trim().is_empty() {
            stderr.trim()
        } else {
            stdout.trim()
        };
        bail!("Failed to disconnect session: {}", err_msg);
    }

    Ok(stdout.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sessions_list_empty() {
        let sample = "No sessions available\n";
        let sessions = parse_sessions_list(sample);
        assert!(sessions.is_empty());
    }

    #[test]
    fn test_parse_sessions_list_single() {
        let sample = r#"
-----------------------------------------------------------------------------
        Path: /net/openvpn/v3/sessions/c25771ccs40ces4d1bs814fs94e6be0c5798
     Created: 2026-09-20 22:49:46                       PID: 1507962
       Owner: dizba                                  Device: tun0
 Config name: /home/dizba/.config/ovpn3-tui/configs/aws-sam.ovpn  (Config not available)
Connected to: udp:43.218.199.109:1194
      Status: Connection, Client connected
-----------------------------------------------------------------------------
"#;
        let sessions = parse_sessions_list(sample);
        assert_eq!(sessions.len(), 1);
        let s = &sessions[0];
        assert_eq!(
            s.path,
            "/net/openvpn/v3/sessions/c25771ccs40ces4d1bs814fs94e6be0c5798"
        );
        assert_eq!(s.created, "2026-09-20 22:49:46");
        assert_eq!(s.pid, Some(1507962));
        assert_eq!(s.owner, "dizba");
        assert_eq!(s.device, "tun0");
        assert_eq!(
            s.config_name,
            "/home/dizba/.config/ovpn3-tui/configs/aws-sam.ovpn"
        );
        assert_eq!(s.connected_to, "udp:43.218.199.109:1194");
        assert_eq!(s.status, "Connection, Client connected");
        assert!(s.is_connected());
        assert!(s.matches_profile(
            "aws-sam.ovpn",
            std::path::Path::new("/home/dizba/.config/ovpn3-tui/configs/aws-sam.ovpn")
        ));
    }

    #[test]
    fn test_parse_sessions_list_multiple() {
        let sample = r#"
-----------------------------------------------------------------------------
Path: /net/openvpn/v3/sessions/1111
Config name: vpn1.ovpn
Device: tun0
Status: Connection, client active
-----------------------------------------------------------------------------
Path: /net/openvpn/v3/sessions/2222
Config name: vpn2.ovpn
Device: tun1
Status: Client connected
-----------------------------------------------------------------------------
"#;
        let sessions = parse_sessions_list(sample);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[0].config_name, "vpn1.ovpn");
        assert_eq!(sessions[0].device, "tun0");
        assert_eq!(sessions[1].config_name, "vpn2.ovpn");
        assert_eq!(sessions[1].device, "tun1");
    }

    #[test]
    fn test_parse_session_auth() {
        let sample = r#"
-----------------------------------------------------------------------------
Session path: /net/openvpn/v3/sessions/0b5a34f5s5fa8s4801sbfd3sbe91cbe04085
Auth Request: 42
Auth status: Challenge/response authentication
Auth URL: https://auth.vpn.company.com/sso
-----------------------------------------------------------------------------
"#;
        let requests = parse_session_auth(sample);
        assert_eq!(requests.len(), 1);
        let req = &requests[0];
        assert_eq!(
            req.session_path,
            "/net/openvpn/v3/sessions/0b5a34f5s5fa8s4801sbfd3sbe91cbe04085"
        );
        assert_eq!(req.auth_req_id, "42");
        assert_eq!(req.auth_status, "Challenge/response authentication");
        assert_eq!(
            req.auth_url,
            Some("https://auth.vpn.company.com/sso".into())
        );
    }

    #[test]
    fn test_parse_session_stats_json() {
        let json_sample = r#"{
            "statistics": {
                "TUN_BYTES_IN": 1048576,
                "TUN_BYTES_OUT": 524288
            }
        }"#;
        let (rx, tx) = parse_session_stats_json(json_sample).unwrap();
        assert_eq!(rx, 1048576);
        assert_eq!(tx, 524288);

        let json_flat = r#"{
            "BYTES_IN": 2048,
            "BYTES_OUT": 1024
        }"#;
        let (rx2, tx2) = parse_session_stats_json(json_flat).unwrap();
        assert_eq!(rx2, 2048);
        assert_eq!(tx2, 1024);
    }

    #[test]
    fn test_parse_session_stats_text() {
        let text_dots = r#"
Connection statistics:
     BYTES_IN...................71949
     BYTES_OUT..................73261
     PACKETS_IN...................302
     PACKETS_OUT..................565
     TUN_BYTES_IN...............54775
     TUN_BYTES_OUT..............59339
"#;
        let (rx, tx) = parse_session_stats_text(text_dots).unwrap();
        assert_eq!(rx, 54775);
        assert_eq!(tx, 59339);

        let text_colon = r#"
Connection statistics:
TUN_BYTES_IN: 3000
TUN_BYTES_OUT: 1500
TUN_PACKETS_IN: 20
"#;
        let (rx2, tx2) = parse_session_stats_text(text_colon).unwrap();
        assert_eq!(rx2, 3000);
        assert_eq!(tx2, 1500);
    }
}
