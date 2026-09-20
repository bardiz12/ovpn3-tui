use crate::app::{App, ModalField};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

pub fn render_credential_modal(frame: &mut Frame, app: &App) {
    let modal = match &app.modal {
        Some(m) => m,
        None => return,
    };

    // Use adaptive sizing to ensure nothing is clipped even on standard 80x24 terminals
    let area = adaptive_modal_rect(72, 19, frame.area());
    frame.render_widget(Clear, area);

    let title = if modal.is_connect_flow {
        format!(" Connect to: {} ", modal.profile_name)
    } else {
        format!(" Edit Credentials: {} ", modal.profile_name)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Condense layout if terminal height is constrained
    let is_compact = inner_area.height < 16;
    let chunks = if is_compact {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Username input
                Constraint::Length(3), // Password input
                Constraint::Length(3), // OTP / Authenticator code input
                Constraint::Length(1), // Toggles
                Constraint::Min(1),    // Actions guide
            ])
            .split(inner_area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Info text
                Constraint::Length(3), // Username input
                Constraint::Length(3), // Password input
                Constraint::Length(3), // OTP / Authenticator code input
                Constraint::Length(2), // Toggles
                Constraint::Min(1),    // Actions guide
            ])
            .split(inner_area)
    };

    let (u_chunk, p_chunk, o_chunk, t_chunk, a_chunk) = if is_compact {
        (chunks[0], chunks[1], chunks[2], chunks[3], chunks[4])
    } else {
        let info_text = Paragraph::new(Line::from(vec![Span::styled(
            "Username & Password encrypted in SQLite. OTP is optional.",
            Style::default()
                .fg(Color::Gray)
                .add_modifier(Modifier::ITALIC),
        )]));
        frame.render_widget(info_text, chunks[0]);
        (chunks[1], chunks[2], chunks[3], chunks[4], chunks[5])
    };

    // Username input
    let u_focused = modal.focused_field == ModalField::Username;
    let u_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if u_focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(
            " Username ",
            if u_focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ));

    let u_cursor = if u_focused { "█" } else { "" };
    let u_text = format!("{}{}", modal.username, u_cursor);
    frame.render_widget(Paragraph::new(u_text).block(u_block), u_chunk);

    // Password input
    let p_focused = modal.focused_field == ModalField::Password;
    let p_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if p_focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(
            " Password ",
            if p_focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ));

    let p_display = if modal.show_password {
        modal.password.clone()
    } else {
        "*".repeat(modal.password.len())
    };
    let p_cursor = if p_focused { "█" } else { "" };
    let p_text = format!("{}{}", p_display, p_cursor);
    frame.render_widget(Paragraph::new(p_text).block(p_block), p_chunk);

    // OTP / Authenticator input
    let o_focused = modal.focused_field == ModalField::OtpCode;
    let o_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if o_focused {
            Color::Cyan
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(
            " Authenticator / OTP Code (Optional) ",
            if o_focused {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            },
        ));

    let o_cursor = if o_focused { "█" } else { "" };
    let o_text = format!("{}{}", modal.otp_code, o_cursor);
    frame.render_widget(Paragraph::new(o_text).block(o_block), o_chunk);

    // Toggle options line
    let show_pass_msg = if modal.show_password {
        "[F2] Hide Password"
    } else {
        "[F2] Show Password"
    };
    let append_otp_msg = if modal.append_otp_to_password {
        "[F3] OTP Mode: Append to Password [ON]"
    } else {
        "[F3] OTP Mode: Challenge Response [DEFAULT]"
    };

    let toggle_line = Line::from(vec![
        Span::styled(show_pass_msg, Style::default().fg(Color::DarkGray)),
        Span::styled("  |  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            append_otp_msg,
            if modal.append_otp_to_password {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            },
        ),
    ]);
    frame.render_widget(Paragraph::new(toggle_line), t_chunk);

    // Help instructions
    let instructions = vec![Line::from(vec![
        Span::styled(
            "[Tab]",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Next Field   ", Style::default().fg(Color::White)),
        Span::styled(
            "[Enter]",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Connect / Save   ", Style::default().fg(Color::White)),
        Span::styled(
            "[Esc]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" Cancel", Style::default().fg(Color::White)),
    ])];
    frame.render_widget(Paragraph::new(instructions), a_chunk);
}

pub fn render_auth_challenge_modal(frame: &mut Frame, app: &App) {
    let auth = match &app.auth_modal {
        Some(a) => a,
        None => return,
    };

    // Adaptive sizing for 2FA challenge modal
    let area = adaptive_modal_rect(70, 16, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(if auth.is_submitting {
            Color::LightYellow
        } else {
            Color::LightMagenta
        }))
        .title(Span::styled(
            if auth.is_submitting {
                " 2FA Authenticator Challenge [VERIFYING...] "
            } else {
                " 2FA Authenticator Challenge Required "
            },
            Style::default()
                .fg(if auth.is_submitting {
                    Color::LightYellow
                } else {
                    Color::LightMagenta
                })
                .add_modifier(Modifier::BOLD),
        ));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2), // Session / Request ID
            Constraint::Length(2), // Status / Error / URL / Submitting message
            Constraint::Length(3), // Input box for OTP
            Constraint::Min(1),    // Actions guide
        ])
        .split(inner_area);

    let session_short = if auth.session_path.len() > 36 {
        format!("...{}", &auth.session_path[auth.session_path.len() - 33..])
    } else {
        auth.session_path.clone()
    };

    let info_lines = vec![
        Line::from(vec![
            Span::styled("Auth Request ID : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &auth.auth_req_id,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  |  Session: ", Style::default().fg(Color::DarkGray)),
            Span::styled(session_short, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Challenge       : ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                if auth.status_description.is_empty() {
                    "Authenticator / TOTP code required"
                } else {
                    &auth.status_description
                },
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    frame.render_widget(Paragraph::new(info_lines), chunks[0]);

    // Status / Message row
    if auth.is_submitting {
        let submitting_line = vec![
            Line::from(vec![Span::styled(
                "⏳ Submitting code to OpenVPN 3... Please wait.",
                Style::default()
                    .fg(Color::LightYellow)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                "Verifying authentication credentials. Modal will close upon success.",
                Style::default().fg(Color::Gray),
            )]),
        ];
        frame.render_widget(Paragraph::new(submitting_line), chunks[1]);
    } else if let Some(ref err) = auth.error_message {
        let err_line = vec![
            Line::from(vec![Span::styled(
                format!("✖ {}", err),
                Style::default()
                    .fg(Color::LightRed)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![Span::styled(
                "Please re-enter your 6-digit Authenticator code below:",
                Style::default().fg(Color::Gray),
            )]),
        ];
        frame.render_widget(Paragraph::new(err_line), chunks[1]);
    } else if let Some(ref url) = auth.auth_url {
        let url_line = vec![
            Line::from(vec![
                Span::styled("Auth URL: ", Style::default().fg(Color::Yellow)),
                Span::styled(
                    url,
                    Style::default()
                        .fg(Color::LightBlue)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]),
            Line::from(vec![Span::styled(
                "Press [o] to open URL in web browser.",
                Style::default().fg(Color::Gray),
            )]),
        ];
        frame.render_widget(Paragraph::new(url_line), chunks[1]);
    } else {
        let hint_line = Line::from(vec![Span::styled(
            "Enter the 6-digit code from your Authenticator app (Google Authenticator, etc.):",
            Style::default().fg(Color::Gray),
        )]);
        frame.render_widget(Paragraph::new(hint_line), chunks[1]);
    }

    // Input box
    let input_border_color = if auth.is_submitting {
        Color::DarkGray
    } else if auth.error_message.is_some() {
        Color::LightRed
    } else {
        Color::LightGreen
    };

    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(input_border_color))
        .title(Span::styled(
            if auth.is_submitting {
                " Authenticator Code [Submitting...] "
            } else {
                " Enter Authenticator Code "
            },
            Style::default()
                .fg(input_border_color)
                .add_modifier(Modifier::BOLD),
        ));

    let display_input = if auth.is_submitting {
        format!("{}  (Waiting for OpenVPN 3)", auth.code_input)
    } else {
        format!("{}█", auth.code_input)
    };
    frame.render_widget(Paragraph::new(display_input).block(input_block), chunks[2]);

    let action_line = if auth.is_submitting {
        vec![Line::from(vec![
            Span::styled(
                "Waiting for response from openvpn3 cli... ",
                Style::default().fg(Color::DarkGray),
            ),
            Span::styled("[Esc]", Style::default().fg(Color::Red)),
            Span::styled(" Dismiss", Style::default().fg(Color::DarkGray)),
        ])]
    } else {
        vec![Line::from(vec![
            Span::styled(
                "[Enter]",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Submit Code   ", Style::default().fg(Color::White)),
            Span::styled(
                "[Esc]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Dismiss / Cancel", Style::default().fg(Color::White)),
        ])]
    };
    frame.render_widget(Paragraph::new(action_line), chunks[3]);
}

/// Computes centered, non-clipping modal rectangle bounded by available terminal area
pub fn adaptive_modal_rect(target_width: u16, target_height: u16, area: Rect) -> Rect {
    let width = target_width.min(area.width.saturating_sub(2)).max(20);
    let height = target_height.min(area.height.saturating_sub(2)).max(10);

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    Rect {
        x,
        y,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adaptive_modal_rect_large_screen() {
        let area = Rect::new(0, 0, 120, 40);
        let rect = adaptive_modal_rect(72, 19, area);
        assert_eq!(rect.width, 72);
        assert_eq!(rect.height, 19);
        assert_eq!(rect.x, (120 - 72) / 2);
        assert_eq!(rect.y, (40 - 19) / 2);
    }

    #[test]
    fn test_adaptive_modal_rect_small_screen() {
        let area = Rect::new(0, 0, 80, 24);
        let rect = adaptive_modal_rect(72, 19, area);
        assert_eq!(rect.width, 72);
        assert_eq!(rect.height, 19);
        assert!(rect.width <= area.width);
        assert!(rect.height <= area.height);
    }

    #[test]
    fn test_adaptive_modal_rect_very_small_screen() {
        let area = Rect::new(0, 0, 50, 14);
        let rect = adaptive_modal_rect(72, 19, area);
        assert_eq!(rect.width, 48);
        assert_eq!(rect.height, 12);
        assert!(rect.width <= area.width);
        assert!(rect.height <= area.height);
    }
}
