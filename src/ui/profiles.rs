use crate::app::App;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem};

pub fn render_profiles(frame: &mut Frame, app: &App, area: Rect) {
    let mut items = Vec::new();

    if app.profiles.is_empty() {
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "  No configs found in ~/.config/ovpn3-tui/configs/",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )])));
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "  Drop your .ovpn or .conf files there and press [r] to refresh.",
            Style::default().fg(Color::DarkGray),
        )])));
    } else {
        for (i, profile) in app.profiles.iter().enumerate() {
            let is_selected = i == app.selected_index;
            let is_active = app.is_profile_active(profile);

            let prefix = if is_selected { "▶ " } else { "  " };

            let (status_badge, status_color) = if is_active {
                ("[● Connected] ", Color::LightGreen)
            } else if app.is_connecting && is_selected {
                ("[◌ Connecting]", Color::LightYellow)
            } else {
                ("[○ Idle]      ", Color::DarkGray)
            };

            let (cred_badge, cred_color) = if profile.has_credential {
                ("[K] ", Color::Yellow)
            } else {
                ("[ ] ", Color::DarkGray)
            };

            let name_style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            let line = Line::from(vec![
                Span::styled(
                    prefix,
                    if is_selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    },
                ),
                Span::styled(
                    status_badge,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" ", Style::default()),
                Span::styled(cred_badge, Style::default().fg(cred_color)),
                Span::styled(&profile.name, name_style),
            ]);

            let item = ListItem::new(line);
            let item = if is_selected {
                item.style(Style::default().bg(Color::Rgb(30, 40, 55)))
            } else {
                item
            };

            items.push(item);
        }
    }

    let title = format!(" Config Profiles ({}) ", app.profiles.len());

    let list_widget = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Blue))
            .title(Span::styled(
                title,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )),
    );

    frame.render_widget(list_widget, area);
}
