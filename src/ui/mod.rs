pub mod modal;
pub mod monitor;
pub mod profiles;

use crate::app::App;
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph};

pub fn render_ui(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Main layout: Header, Content, Status Banner, Footer
    let constraints = if app.status_message.is_some() {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Body
            Constraint::Length(1), // Status notification
            Constraint::Length(1), // Footer keybindings
        ]
    } else {
        vec![
            Constraint::Length(3), // Header
            Constraint::Min(10),   // Body
            Constraint::Length(1), // Footer keybindings
        ]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    render_header(frame, app, chunks[0]);

    // Split Body into Left (Profiles) and Right (Monitor & Sparkline)
    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(45), // Profiles list
            Constraint::Percentage(55), // Active session & sparkline
        ])
        .split(chunks[1]);

    profiles::render_profiles(frame, app, body_chunks[0]);
    monitor::render_monitor(frame, app, body_chunks[1]);

    if app.status_message.is_some() {
        render_status_banner(frame, app, chunks[2]);
        render_footer(frame, chunks[3]);
    } else {
        render_footer(frame, chunks[2]);
    }

    // Modal popups if active
    if app.modal.is_some() {
        modal::render_credential_modal(frame, app);
    } else if app.auth_modal.is_some() {
        modal::render_auth_challenge_modal(frame, app);
    }
}

fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let has_active_session = app.current_active_session().is_some();
    let has_pending_auth = !app.pending_auths.is_empty();

    let (badge, badge_color) = if has_pending_auth {
        ("⚠ 2FA AUTH REQUIRED (Press [a])", Color::LightMagenta)
    } else if has_active_session {
        ("● CONNECTED", Color::LightGreen)
    } else if app.is_connecting {
        ("◌ CONNECTING...", Color::LightYellow)
    } else {
        ("○ DISCONNECTED", Color::DarkGray)
    };

    let title_line = Line::from(vec![
        Span::styled(
            " OVPN3-TUI ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "— OpenVPN 3 Client Manager  ",
            Style::default().fg(Color::Gray),
        ),
        Span::styled(
            badge,
            Style::default()
                .fg(badge_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if has_pending_auth {
            Color::LightMagenta
        } else {
            Color::Blue
        }));

    let paragraph = Paragraph::new(title_line).block(block);
    frame.render_widget(paragraph, area);
}

fn render_status_banner(frame: &mut Frame, app: &App, area: Rect) {
    if let Some((msg, _, is_err)) = &app.status_message {
        let (prefix, color) = if *is_err {
            ("✖ ERROR: ", Color::LightRed)
        } else {
            ("✔ INFO: ", Color::LightGreen)
        };

        let line = Line::from(vec![
            Span::styled(
                prefix,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(msg, Style::default().fg(Color::White)),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }
}

fn render_footer(frame: &mut Frame, area: Rect) {
    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);

    let line = Line::from(vec![
        Span::styled(" [↑/↓/j/k]", key_style),
        Span::styled(" Select  ", desc_style),
        Span::styled("[c]", key_style),
        Span::styled(" Connect  ", desc_style),
        Span::styled("[d]", key_style),
        Span::styled(" Disconnect  ", desc_style),
        Span::styled("[a]", key_style),
        Span::styled(" 2FA Code  ", desc_style),
        Span::styled("[e]", key_style),
        Span::styled(" Edit Creds  ", desc_style),
        Span::styled("[x]", key_style),
        Span::styled(" Del Creds  ", desc_style),
        Span::styled("[r]", key_style),
        Span::styled(" Refresh  ", desc_style),
        Span::styled("[q]", key_style),
        Span::styled(" Quit", desc_style),
    ]);

    frame.render_widget(Paragraph::new(line), area);
}
