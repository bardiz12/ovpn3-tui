pub mod app;
pub mod config;
pub mod crypto;
pub mod db;
pub mod openvpn;
pub mod ui;

use anyhow::Result;
use app::{App, ModalField};
use config::AppPaths;
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use openvpn::cli::{
    AuthRequest, SessionInfo, check_openvpn3_binary, check_pending_auth, disconnect_session,
    list_sessions, provide_auth_response, query_session_stats, start_session,
};
use openvpn::stats::ThroughputMonitor;
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use std::io::{IsTerminal, stdout};
use std::time::Duration;
use tokio::sync::mpsc;

enum AsyncMessage {
    SessionsUpdated(Vec<SessionInfo>),
    StatsUpdated(u64, u64),
    AuthPendingUpdated(Vec<AuthRequest>),
    ConnectFinished(Result<String, String>),
    DisconnectFinished(Result<String, String>),
    AuthResponseFinished(Result<String, String>),
}

fn print_help(paths: &AppPaths) {
    println!("ovpn3-tui — OpenVPN 3 Terminal User Interface");
    println!();
    println!("USAGE:");
    println!("    ovpn3-tui [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       Print help information");
    println!("    -v, --version    Print version information");
    println!();
    println!("DIRECTORIES & FILES:");
    println!("    Configs Directory: {:?}", paths.configs_dir);
    println!("    Database:          {:?}", paths.db_path);
    println!("    Encryption Key:    {:?}", paths.key_path);
    println!();
    println!("KEYBINDINGS:");
    println!("    ↑, k, Down, j     Navigate profile list");
    println!("    c, Enter          Connect to selected profile");
    println!("    d                 Disconnect active session");
    println!("    a                 Open 2FA / Authenticator code challenge prompt");
    println!("    e                 Edit/Save encrypted credentials & optional OTP");
    println!("    x                 Delete saved credentials for selected profile");
    println!("    r                 Refresh profiles, sessions, and auth challenges");
    println!("    q, Esc, Ctrl+C    Quit application");
}

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Setup paths and check command-line arguments
    let paths = AppPaths::default_paths()?;

    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help(&paths);
        return Ok(());
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-v") {
        println!("ovpn3-tui {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    // Check if openvpn3 binary is installed and executable
    if let Err(err) = check_openvpn3_binary() {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }

    if !std::io::stdin().is_terminal() {
        eprintln!("Error: ovpn3-tui requires an interactive terminal (TTY) to run.");
        std::process::exit(1);
    }

    // Ensure config directories, crypto key, and database are ready
    paths.ensure_dirs()?;
    let key = crypto::load_or_generate_key(&paths.key_path)?;
    let conn = db::init_db(&paths.db_path)?;

    // 2. Initialize application state
    let mut app = App::new(paths, key);
    let _ = app.refresh_profiles(&conn);

    // Initial check for active sessions and pending auth
    if let Ok(sessions) = list_sessions().await {
        app.update_sessions(sessions);
        if let Some(session) = app.current_active_session() {
            // Read directly from kernel interface statistics first (no process spawn)
            if let Some((rx, tx)) = ThroughputMonitor::read_interface_bytes(&session.device) {
                app.update_stats_from_bytes(rx, tx);
            } else if let Some((rx, tx)) = query_session_stats(&session.path).await {
                app.update_stats_from_bytes(rx, tx);
            }
        }
    }
    if let Ok(auths) = check_pending_auth().await {
        app.update_pending_auths(auths);
    }

    // 3. Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Channel for background async tasks to send results back to main event loop
    let (tx, mut rx) = mpsc::unbounded_channel::<AsyncMessage>();

    // 4. Main Event Loop
    let mut stats_ticker = tokio::time::interval(Duration::from_millis(1000));
    let mut tick_rate = tokio::time::interval(Duration::from_millis(50));

    loop {
        // Render UI
        terminal.draw(|f| ui::render_ui(f, &app))?;

        if app.should_quit {
            break;
        }

        tokio::select! {
            // Receive async background messages (connect/disconnect/session updates/auth updates/stats)
            Some(msg) = rx.recv() => {
                match msg {
                    AsyncMessage::SessionsUpdated(sessions) => {
                        app.update_sessions(sessions);
                    }
                    AsyncMessage::StatsUpdated(rx_bytes, tx_bytes) => {
                        app.update_stats_from_bytes(rx_bytes, tx_bytes);
                    }
                    AsyncMessage::AuthPendingUpdated(auths) => {
                        app.update_pending_auths(auths);
                    }
                    AsyncMessage::ConnectFinished(res) => {
                        app.is_connecting = false;
                        match res {
                            Ok(msg) => {
                                app.set_status(format!("Connected: {}", msg), false);
                            }
                            Err(err) => {
                                app.set_status(err, true);
                            }
                        }
                        // Refresh session list & auth requests immediately after connect attempt
                        if let Ok(sessions) = list_sessions().await {
                            app.update_sessions(sessions);
                        }
                        if let Ok(auths) = check_pending_auth().await {
                            app.update_pending_auths(auths);
                        }
                    }
                    AsyncMessage::DisconnectFinished(res) => {
                        match res {
                            Ok(msg) => {
                                app.set_status(format!("Disconnected: {}", msg), false);
                                app.throughput.reset();
                            }
                            Err(err) => {
                                app.set_status(err, true);
                            }
                        }
                        if let Ok(sessions) = list_sessions().await {
                            app.update_sessions(sessions);
                        }
                        if let Ok(auths) = check_pending_auth().await {
                            app.update_pending_auths(auths);
                        }
                    }
                    AsyncMessage::AuthResponseFinished(res) => {
                        app.submitting_auth_req_id = None;
                        match res {
                            Ok(msg) => {
                                app.auth_modal = None;
                                app.set_status(format!("2FA verified: {}", msg), false);
                            }
                            Err(err) => {
                                if let Some(ref mut auth) = app.auth_modal {
                                    auth.is_submitting = false;
                                    auth.error_message = Some(err.clone());
                                    auth.code_input.clear();
                                }
                                app.set_status(format!("2FA Verification failed: {}", err), true);
                            }
                        }
                        if let Ok(auths) = check_pending_auth().await {
                            app.update_pending_auths(auths);
                        }
                        if let Ok(sessions) = list_sessions().await {
                            app.update_sessions(sessions);
                        }
                    }
                }
            }

            // High-frequency UI tick (keyboard event polling, status expiration)
            _ = tick_rate.tick() => {
                app.clear_expired_status();

                // Poll terminal events with zero wait since we are in async select
                if event::poll(Duration::from_millis(0))? {
                    if let Event::Key(key) = event::read()? {
                        if key.kind == KeyEventKind::Press {
                            handle_key_event(&mut app, key.code, key.modifiers, &conn, &tx).await;
                        }
                    }
                }
            }

            // 1-second interval ticker for network throughput calculation, session sync & auth checks
            _ = stats_ticker.tick() => {
                // Read stats directly from Linux kernel files (sysfs/procfs) without spawning subprocesses
                if let Some(session) = app.current_active_session() {
                    if let Some((rx, tx)) = ThroughputMonitor::read_interface_bytes(&session.device) {
                        app.update_stats_from_bytes(rx, tx);
                    } else {
                        // Fallback to openvpn3 CLI in background only if device file is not yet available
                        let session_path = session.path.clone();
                        let tx_stats = tx.clone();

                        tokio::spawn(async move {
                            if let Some((rx, tx)) = query_session_stats(&session_path).await {
                                let _ = tx_stats.send(AsyncMessage::StatsUpdated(rx, tx));
                            }
                        });
                    }
                } else {
                    app.throughput.tick_idle();
                }

                // Poll openvpn3 sessions & pending auth requests in background if not actively connecting
                if !app.is_connecting {
                    let tx_sessions = tx.clone();
                    tokio::spawn(async move {
                        if let Ok(sessions) = list_sessions().await {
                            let _ = tx_sessions.send(AsyncMessage::SessionsUpdated(sessions));
                        }
                    });

                    // Only poll auth if we aren't currently waiting for a submission response
                    if app.submitting_auth_req_id.is_none() {
                        let tx_auth = tx.clone();
                        tokio::spawn(async move {
                            if let Ok(auths) = check_pending_auth().await {
                                let _ = tx_auth.send(AsyncMessage::AuthPendingUpdated(auths));
                            }
                        });
                    }
                }
            }
        }
    }

    // 5. Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

async fn handle_key_event(
    app: &mut App,
    code: KeyCode,
    modifiers: KeyModifiers,
    conn: &rusqlite::Connection,
    tx: &mpsc::UnboundedSender<AsyncMessage>,
) {
    // 1. If 2FA challenge modal is active, route keys to 2FA modal
    if let Some(ref mut auth_modal) = app.auth_modal {
        match code {
            KeyCode::Esc => {
                app.auth_modal = None;
                app.submitting_auth_req_id = None;
            }
            KeyCode::Char('o') if auth_modal.auth_url.is_some() && !auth_modal.is_submitting => {
                if let Some(ref url) = auth_modal.auth_url {
                    let url_clone = url.clone();
                    tokio::spawn(async move {
                        let _ = tokio::process::Command::new("xdg-open")
                            .arg(&url_clone)
                            .spawn();
                    });
                    app.set_status("Opening auth URL in browser...", false);
                }
            }
            KeyCode::Char(c) if !auth_modal.is_submitting => {
                auth_modal.code_input.push(c);
            }
            KeyCode::Backspace if !auth_modal.is_submitting => {
                auth_modal.code_input.pop();
            }
            KeyCode::Enter => {
                if auth_modal.is_submitting {
                    return; // Ignore Enter while already awaiting OpenVPN 3 response
                }
                let code_val = auth_modal.code_input.trim().to_string();
                if code_val.is_empty() {
                    auth_modal.error_message =
                        Some("Please enter the Authenticator code".to_string());
                    return;
                }

                let auth_req_id = auth_modal.auth_req_id.clone();
                auth_modal.is_submitting = true;
                auth_modal.error_message = None;
                app.submitting_auth_req_id = Some(auth_req_id.clone());
                app.set_status(
                    format!(
                        "Submitting Authenticator code for request {}...",
                        auth_req_id
                    ),
                    false,
                );

                let tx_clone = tx.clone();
                tokio::spawn(async move {
                    let res = provide_auth_response(&auth_req_id, &code_val).await;
                    match res {
                        Ok(msg) => {
                            let _ = tx_clone.send(AsyncMessage::AuthResponseFinished(Ok(msg)));
                        }
                        Err(e) => {
                            let _ = tx_clone
                                .send(AsyncMessage::AuthResponseFinished(Err(e.to_string())));
                        }
                    }
                });
            }
            _ => {}
        }
        return;
    }

    // 2. If credentials modal dialog is open, route keys to modal
    if let Some(ref mut modal) = app.modal {
        match code {
            KeyCode::Esc => {
                app.modal = None;
            }
            KeyCode::Tab => {
                modal.focused_field = match modal.focused_field {
                    ModalField::Username => ModalField::Password,
                    ModalField::Password => ModalField::OtpCode,
                    ModalField::OtpCode => ModalField::Username,
                };
            }
            KeyCode::BackTab => {
                modal.focused_field = match modal.focused_field {
                    ModalField::Username => ModalField::OtpCode,
                    ModalField::Password => ModalField::Username,
                    ModalField::OtpCode => ModalField::Password,
                };
            }
            KeyCode::F(2) => {
                modal.show_password = !modal.show_password;
            }
            KeyCode::F(3) => {
                modal.append_otp_to_password = !modal.append_otp_to_password;
            }
            KeyCode::Char(c) => match modal.focused_field {
                ModalField::Username => modal.username.push(c),
                ModalField::Password => modal.password.push(c),
                ModalField::OtpCode => modal.otp_code.push(c),
            },
            KeyCode::Backspace => match modal.focused_field {
                ModalField::Username => {
                    modal.username.pop();
                }
                ModalField::Password => {
                    modal.password.pop();
                }
                ModalField::OtpCode => {
                    modal.otp_code.pop();
                }
            },
            KeyCode::Enter => {
                let is_connect_flow = modal.is_connect_flow;
                match app.save_modal_credentials(conn) {
                    Ok((username, effective_password, otp_to_pipe)) => {
                        let selected_profile_path = app
                            .selected_profile()
                            .map(|p| p.path.to_string_lossy().to_string());

                        app.modal = None;

                        if is_connect_flow {
                            if let Some(path_str) = selected_profile_path {
                                trigger_connect(
                                    app,
                                    path_str,
                                    Some(username),
                                    Some(effective_password),
                                    otp_to_pipe,
                                    tx,
                                );
                            }
                        }
                    }
                    Err(e) => {
                        app.set_status(format!("Error saving credentials: {}", e), true);
                    }
                }
            }
            _ => {}
        }
        return;
    }

    // 3. Main screen keybindings
    match code {
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            app.prev_profile();
        }
        KeyCode::Down | KeyCode::Char('j') => {
            app.next_profile();
        }
        KeyCode::Char('a') => {
            // Open 2FA Challenge prompt if any pending authentication exists
            if let Some(req) = app.pending_auths.first().cloned() {
                app.open_auth_challenge_modal(req);
            } else {
                // Query fresh to see if an auth request just arrived
                match check_pending_auth().await {
                    Ok(auths) if !auths.is_empty() => {
                        let req = auths[0].clone();
                        app.update_pending_auths(auths);
                        app.open_auth_challenge_modal(req);
                    }
                    _ => {
                        app.set_status("No pending 2FA authentication requests", false);
                    }
                }
            }
        }
        KeyCode::Char('r') => {
            if let Err(e) = app.refresh_profiles(conn) {
                app.set_status(format!("Failed to refresh profiles: {}", e), true);
            } else {
                app.set_status("Profiles & sessions refreshed", false);
            }
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                if let Ok(sessions) = list_sessions().await {
                    let _ = tx_clone.send(AsyncMessage::SessionsUpdated(sessions));
                }
                if let Ok(auths) = check_pending_auth().await {
                    let _ = tx_clone.send(AsyncMessage::AuthPendingUpdated(auths));
                }
            });
        }
        KeyCode::Char('e') => {
            if app.selected_profile().is_some() {
                app.open_credential_modal(conn, false);
            } else {
                app.set_status("No profile selected", true);
            }
        }
        KeyCode::Char('x') => {
            if let Err(e) = app.delete_selected_credential(conn) {
                app.set_status(format!("Error deleting credentials: {}", e), true);
            }
        }
        KeyCode::Char('c') | KeyCode::Enter => {
            if let Some(profile) = app.selected_profile() {
                if app.is_profile_active(profile) {
                    app.set_status(
                        format!("Profile '{}' is already connected", profile.name),
                        false,
                    );
                    return;
                }

                if app.is_connecting {
                    app.set_status("Connection attempt already in progress...", true);
                    return;
                }

                // Check if profile has saved credentials in SQLite
                match db::get_credential(conn, &profile.name, &app.key) {
                    Ok(Some(cred)) => {
                        let path_str = profile.path.to_string_lossy().to_string();
                        trigger_connect(
                            app,
                            path_str,
                            Some(cred.username),
                            Some(cred.password),
                            None,
                            tx,
                        );
                    }
                    Ok(None) => {
                        // Prompt user via modal before connecting
                        app.open_credential_modal(conn, true);
                    }
                    Err(e) => {
                        app.set_status(format!("Error retrieving credentials: {}", e), true);
                    }
                }
            } else {
                app.set_status("No profile selected to connect", true);
            }
        }
        KeyCode::Char('d') => {
            if let Some(session) = app.current_active_session() {
                let session_path = session.path.clone();
                let tx_clone = tx.clone();
                app.set_status(
                    format!("Disconnecting session {}...", session.config_name),
                    false,
                );

                tokio::spawn(async move {
                    let res = disconnect_session(&session_path).await;
                    match res {
                        Ok(msg) => {
                            let _ = tx_clone.send(AsyncMessage::DisconnectFinished(Ok(msg)));
                        }
                        Err(e) => {
                            let _ =
                                tx_clone.send(AsyncMessage::DisconnectFinished(Err(e.to_string())));
                        }
                    }
                });
            } else {
                app.set_status("No active session to disconnect", false);
            }
        }
        _ => {}
    }
}

fn trigger_connect(
    app: &mut App,
    config_path: String,
    username: Option<String>,
    password: Option<String>,
    otp_code: Option<String>,
    tx: &mpsc::UnboundedSender<AsyncMessage>,
) {
    app.is_connecting = true;
    app.set_status(format!("Starting connection for {}...", config_path), false);

    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let res = start_session(
            &config_path,
            username.as_deref(),
            password.as_deref(),
            otp_code.as_deref(),
        )
        .await;

        match res {
            Ok(msg) => {
                let _ = tx_clone.send(AsyncMessage::ConnectFinished(Ok(msg)));
            }
            Err(e) => {
                let _ = tx_clone.send(AsyncMessage::ConnectFinished(Err(e.to_string())));
            }
        }
    });
}
