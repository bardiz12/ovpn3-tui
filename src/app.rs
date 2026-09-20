use crate::config::{AppPaths, get_profile_name};
use crate::crypto::SymmetricKey;
use crate::db::{delete_credential, get_credential, has_credential, save_credential};
use crate::openvpn::cli::{AuthRequest, SessionInfo};
use crate::openvpn::stats::ThroughputMonitor;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfileItem {
    pub path: PathBuf,
    pub name: String,
    pub has_credential: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalField {
    Username,
    Password,
    OtpCode,
}

#[derive(Debug, Clone)]
pub struct ModalState {
    pub profile_name: String,
    pub username: String,
    pub password: String,
    pub otp_code: String,
    pub append_otp_to_password: bool,
    pub focused_field: ModalField,
    pub show_password: bool,
    pub is_connect_flow: bool,
}

#[derive(Debug, Clone)]
pub struct AuthChallengeModal {
    pub auth_req_id: String,
    pub session_path: String,
    pub status_description: String,
    pub auth_url: Option<String>,
    pub code_input: String,
    pub is_submitting: bool,
    pub error_message: Option<String>,
}

pub struct App {
    pub paths: AppPaths,
    pub key: SymmetricKey,
    pub profiles: Vec<ProfileItem>,
    pub selected_index: usize,
    pub active_sessions: Vec<SessionInfo>,
    pub pending_auths: Vec<AuthRequest>,
    pub submitting_auth_req_id: Option<String>,
    pub throughput: ThroughputMonitor,
    pub status_message: Option<(String, DateTime<Utc>, bool)>,
    pub is_connecting: bool,
    pub modal: Option<ModalState>,
    pub auth_modal: Option<AuthChallengeModal>,
    pub should_quit: bool,
}

impl App {
    pub fn new(paths: AppPaths, key: SymmetricKey) -> Self {
        Self {
            paths,
            key,
            profiles: Vec::new(),
            selected_index: 0,
            active_sessions: Vec::new(),
            pending_auths: Vec::new(),
            submitting_auth_req_id: None,
            throughput: ThroughputMonitor::new(),
            status_message: None,
            is_connecting: false,
            modal: None,
            auth_modal: None,
            should_quit: false,
        }
    }

    pub fn refresh_profiles(&mut self, conn: &Connection) -> Result<()> {
        let files = self.paths.list_config_files()?;
        let mut items = Vec::new();

        for path in files {
            let name = get_profile_name(&path);
            let has_cred = has_credential(conn, &name).unwrap_or(false);
            items.push(ProfileItem {
                path,
                name,
                has_credential: has_cred,
            });
        }

        self.profiles = items;
        if self.selected_index >= self.profiles.len() && !self.profiles.is_empty() {
            self.selected_index = self.profiles.len() - 1;
        }

        Ok(())
    }

    pub fn update_sessions(&mut self, sessions: Vec<SessionInfo>) {
        self.active_sessions = sessions;
    }

    pub fn update_pending_auths(&mut self, auths: Vec<AuthRequest>) {
        self.pending_auths = auths;

        // If we are currently submitting an auth request, check whether it has resolved
        if let Some(submitting_id) = self.submitting_auth_req_id.clone() {
            let still_pending = self
                .pending_auths
                .iter()
                .any(|a| a.auth_req_id == submitting_id);
            if !still_pending {
                // Request is no longer in pending queue -> finished successfully!
                self.submitting_auth_req_id = None;
                if let Some(ref modal) = self.auth_modal {
                    if modal.auth_req_id == submitting_id && modal.is_submitting {
                        self.auth_modal = None;
                        self.set_status("2FA authentication succeeded", false);
                    }
                }
            }
        }

        // Auto-open modal ONLY if user is not currently submitting, and no modal is active
        if self.submitting_auth_req_id.is_none()
            && self.auth_modal.is_none()
            && self.modal.is_none()
        {
            if let Some(req) = self.pending_auths.first() {
                self.open_auth_challenge_modal(req.clone());
            }
        }
    }

    pub fn open_auth_challenge_modal(&mut self, req: AuthRequest) {
        self.auth_modal = Some(AuthChallengeModal {
            auth_req_id: req.auth_req_id,
            session_path: req.session_path,
            status_description: req.auth_status,
            auth_url: req.auth_url,
            code_input: String::new(),
            is_submitting: false,
            error_message: None,
        });
    }

    pub fn close_auth_challenge_modal(&mut self) {
        self.auth_modal = None;
        self.submitting_auth_req_id = None;
    }

    pub fn update_stats_from_bytes(&mut self, rx: u64, tx: u64) {
        self.throughput.update(rx, tx);
    }

    pub fn update_network_stats(&mut self) {
        if let Some(session) = self.current_active_session() {
            if let Some((rx, tx)) = ThroughputMonitor::read_interface_bytes(&session.device) {
                self.throughput.update(rx, tx);
                return;
            }
        }
        self.throughput.tick_idle();
    }

    pub fn current_active_session(&self) -> Option<&SessionInfo> {
        self.active_sessions
            .iter()
            .find(|s| s.is_connected())
            .or_else(|| self.active_sessions.first())
    }

    pub fn session_for_selected_profile(&self) -> Option<&SessionInfo> {
        if let Some(profile) = self.selected_profile() {
            if let Some(s) = self
                .active_sessions
                .iter()
                .find(|s| s.matches_profile(&profile.name, &profile.path))
            {
                return Some(s);
            }
        }
        self.current_active_session()
    }

    pub fn is_profile_active(&self, profile: &ProfileItem) -> bool {
        self.active_sessions
            .iter()
            .any(|s| s.matches_profile(&profile.name, &profile.path) && s.is_connected())
    }

    pub fn selected_profile(&self) -> Option<&ProfileItem> {
        self.profiles.get(self.selected_index)
    }

    pub fn next_profile(&mut self) {
        if !self.profiles.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.profiles.len();
        }
    }

    pub fn prev_profile(&mut self) {
        if !self.profiles.is_empty() {
            if self.selected_index == 0 {
                self.selected_index = self.profiles.len() - 1;
            } else {
                self.selected_index -= 1;
            }
        }
    }

    pub fn open_credential_modal(&mut self, conn: &Connection, is_connect_flow: bool) {
        if let Some(profile) = self.selected_profile() {
            let existing_cred = get_credential(conn, &profile.name, &self.key)
                .ok()
                .flatten();
            let (username, password) = match existing_cred {
                Some(c) => (c.username, c.password),
                None => (String::new(), String::new()),
            };

            self.modal = Some(ModalState {
                profile_name: profile.name.clone(),
                username,
                password,
                otp_code: String::new(),
                append_otp_to_password: false,
                focused_field: ModalField::Username,
                show_password: false,
                is_connect_flow,
            });
        }
    }

    pub fn close_modal(&mut self) {
        self.modal = None;
    }

    /// Saves credentials to DB and returns (username, effective_password, optional_otp_to_pipe)
    pub fn save_modal_credentials(
        &mut self,
        conn: &Connection,
    ) -> Result<(String, String, Option<String>)> {
        let (username, base_password, otp, profile_name, append_otp) =
            if let Some(modal) = &self.modal {
                (
                    modal.username.trim().to_string(),
                    modal.password.trim().to_string(),
                    modal.otp_code.trim().to_string(),
                    modal.profile_name.clone(),
                    modal.append_otp_to_password,
                )
            } else {
                anyhow::bail!("No active modal to save");
            };

        // Save the base username and password to database (encrypted)
        save_credential(conn, &profile_name, &username, &base_password, &self.key)
            .with_context(|| format!("Failed saving credentials for {}", profile_name))?;

        let _ = self.refresh_profiles(conn);
        self.set_status(format!("Credentials saved for {}", profile_name), false);

        let (effective_password, otp_to_pipe) = if append_otp && !otp.is_empty() {
            (format!("{}{}", base_password, otp), None)
        } else if !otp.is_empty() {
            (base_password, Some(otp))
        } else {
            (base_password, None)
        };

        Ok((username, effective_password, otp_to_pipe))
    }

    pub fn delete_selected_credential(&mut self, conn: &Connection) -> Result<()> {
        if let Some(profile) = self.selected_profile() {
            let name = profile.name.clone();
            let has_cred = profile.has_credential;
            if has_cred {
                delete_credential(conn, &name)?;
                self.refresh_profiles(conn)?;
                self.set_status(format!("Deleted credentials for {}", name), false);
            } else {
                self.set_status(format!("No saved credentials for {}", name), false);
            }
        }
        Ok(())
    }

    pub fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_message = Some((msg.into(), Utc::now(), is_error));
    }

    pub fn clear_expired_status(&mut self) {
        if let Some((_, timestamp, _)) = &self.status_message {
            if Utc::now().signed_duration_since(*timestamp).num_seconds() > 6 {
                self.status_message = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_app_navigation() {
        let dummy_paths = AppPaths {
            base_dir: PathBuf::from("/tmp"),
            configs_dir: PathBuf::from("/tmp/configs"),
            db_path: PathBuf::from("/tmp/test.db"),
            key_path: PathBuf::from("/tmp/.key"),
        };
        let mut app = App::new(dummy_paths, [0u8; 32]);
        app.profiles = vec![
            ProfileItem {
                path: PathBuf::from("/tmp/configs/vpn1.ovpn"),
                name: "vpn1.ovpn".into(),
                has_credential: false,
            },
            ProfileItem {
                path: PathBuf::from("/tmp/configs/vpn2.ovpn"),
                name: "vpn2.ovpn".into(),
                has_credential: true,
            },
        ];

        assert_eq!(app.selected_index, 0);
        app.next_profile();
        assert_eq!(app.selected_index, 1);
        app.next_profile();
        assert_eq!(app.selected_index, 0);
        app.prev_profile();
        assert_eq!(app.selected_index, 1);
    }

    #[test]
    fn test_is_profile_active() {
        let dummy_paths = AppPaths {
            base_dir: PathBuf::from("/tmp"),
            configs_dir: PathBuf::from("/tmp/configs"),
            db_path: PathBuf::from("/tmp/test.db"),
            key_path: PathBuf::from("/tmp/.key"),
        };
        let mut app = App::new(dummy_paths, [0u8; 32]);
        let p = ProfileItem {
            path: PathBuf::from("/home/dizba/.config/ovpn3-tui/configs/aws-sam.ovpn"),
            name: "aws-sam.ovpn".into(),
            has_credential: true,
        };

        assert!(!app.is_profile_active(&p));

        app.active_sessions.push(SessionInfo {
            path: "/net/openvpn/v3/sessions/1".into(),
            config_name: "/home/dizba/.config/ovpn3-tui/configs/aws-sam.ovpn".into(),
            device: "tun0".into(),
            status: "Connection, Client connected".into(),
            ..Default::default()
        });

        assert!(app.is_profile_active(&p));
    }

    #[test]
    fn test_auth_submission_state_does_not_reopen_modal() {
        let dummy_paths = AppPaths {
            base_dir: PathBuf::from("/tmp"),
            configs_dir: PathBuf::from("/tmp/configs"),
            db_path: PathBuf::from("/tmp/test.db"),
            key_path: PathBuf::from("/tmp/.key"),
        };
        let mut app = App::new(dummy_paths, [0u8; 32]);

        let req = AuthRequest {
            session_path: "/session/1".into(),
            auth_req_id: "req-99".into(),
            auth_status: "Dynamic challenge".into(),
            auth_url: None,
        };

        // Initially when pending auth arrives, modal opens
        app.update_pending_auths(vec![req.clone()]);
        assert!(app.auth_modal.is_some());

        // User enters code and clicks Enter:
        let modal = app.auth_modal.as_mut().unwrap();
        modal.is_submitting = true;
        app.submitting_auth_req_id = Some("req-99".into());

        // Background poll occurs while OpenVPN 3 is processing OTP:
        app.update_pending_auths(vec![req.clone()]);

        // Modal should remain in submitting state, NOT reset or opened again
        let modal_after = app.auth_modal.as_ref().unwrap();
        assert!(modal_after.is_submitting);
        assert_eq!(modal_after.auth_req_id, "req-99");

        // When OpenVPN 3 finishes and request leaves pending queue:
        app.update_pending_auths(vec![]);
        assert!(app.auth_modal.is_none());
        assert!(app.submitting_auth_req_id.is_none());
    }
}
