use crate::app::App;
use crate::openvpn::stats::{format_bytes, format_rate};
use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Sparkline};

pub fn render_monitor(frame: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Session details
            Constraint::Min(8),     // Network Throughput Sparklines
        ])
        .split(area);

    render_session_details(frame, app, chunks[0]);
    render_throughput(frame, app, chunks[1]);
}

fn render_session_details(frame: &mut Frame, app: &App, area: Rect) {
    let session = app.session_for_selected_profile();

    let lines = if let Some(s) = session {
        let mut l = vec![
            Line::from(vec![
                Span::styled("Status     : ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    if s.status.is_empty() {
                        "Active"
                    } else {
                        &s.status
                    },
                    Style::default()
                        .fg(Color::LightGreen)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Profile    : ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    &s.config_name,
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        ];

        if !s.connected_to.is_empty() {
            l.push(Line::from(vec![
                Span::styled("Endpoint   : ", Style::default().fg(Color::DarkGray)),
                Span::styled(&s.connected_to, Style::default().fg(Color::LightCyan)),
            ]));
        }

        l.extend(vec![
            Line::from(vec![
                Span::styled("Interface  : ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    if s.device.is_empty() {
                        "tun"
                    } else {
                        &s.device
                    },
                    Style::default().fg(Color::Yellow),
                ),
            ]),
            Line::from(vec![
                Span::styled("Session ID : ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    if s.path.len() > 38 {
                        format!("...{}", &s.path[s.path.len() - 35..])
                    } else {
                        s.path.clone()
                    },
                    Style::default().fg(Color::Cyan),
                ),
            ]),
            Line::from(vec![
                Span::styled("PID / Owner: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "{} / {}",
                        s.pid.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
                        s.owner
                    ),
                    Style::default().fg(Color::Gray),
                ),
            ]),
            Line::from(vec![
                Span::styled("Total Xfer : ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!(
                        "↓ {}  |  ↑ {}",
                        format_bytes(app.throughput.total_rx_bytes),
                        format_bytes(app.throughput.total_tx_bytes)
                    ),
                    Style::default().fg(Color::White),
                ),
            ]),
        ]);

        l
    } else {
        vec![
            Line::from(vec![
                Span::styled("Status     : ", Style::default().fg(Color::DarkGray)),
                Span::styled("Disconnected", Style::default().fg(Color::DarkGray)),
            ]),
            Line::from(vec![Span::styled(
                "No active OpenVPN 3 session running.",
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )]),
            Line::from(vec![Span::styled(
                "Select a profile on the left and press [c] to connect.",
                Style::default().fg(Color::DarkGray),
            )]),
        ]
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if session.is_some() {
            Color::Green
        } else {
            Color::DarkGray
        }))
        .title(Span::styled(
            " Active Session Details ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let paragraph = Paragraph::new(lines).block(block);
    frame.render_widget(paragraph, area);
}

fn render_throughput(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Blue))
        .title(Span::styled(
            " Network Throughput (Sparkline) ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    let sparkline_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // DL Label
            Constraint::Length(2), // DL Sparkline
            Constraint::Length(1), // Spacer
            Constraint::Length(1), // UL Label
            Constraint::Length(2), // UL Sparkline
            Constraint::Min(0),    // Remaining
        ])
        .split(inner_area);

    let rx_rate_str = format_rate(app.throughput.current_rx_rate);
    let tx_rate_str = format_rate(app.throughput.current_tx_rate);

    // Download Label
    let dl_label = Line::from(vec![
        Span::styled(
            " ↓ Download Rate: ",
            Style::default()
                .fg(Color::LightGreen)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            rx_rate_str,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(dl_label), sparkline_layout[0]);

    // Download Sparkline
    let rx_data = app.throughput.rx_history_slice();
    let rx_sparkline = Sparkline::default()
        .data(&rx_data)
        .style(Style::default().fg(Color::LightGreen));
    frame.render_widget(rx_sparkline, sparkline_layout[1]);

    // Upload Label
    let ul_label = Line::from(vec![
        Span::styled(
            " ↑ Upload Rate  : ",
            Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            tx_rate_str,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(ul_label), sparkline_layout[3]);

    // Upload Sparkline
    let tx_data = app.throughput.tx_history_slice();
    let tx_sparkline = Sparkline::default()
        .data(&tx_data)
        .style(Style::default().fg(Color::LightMagenta));
    frame.render_widget(tx_sparkline, sparkline_layout[4]);
}
