use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{App, Mode, Screen},
    APP_NAME,
};

// 現在の画面状態に合わせて描画関数を切り替える。
pub(crate) fn draw(frame: &mut Frame, app: &App) {
    match app.screen {
        Screen::Menu => draw_menu(frame, app),

        Screen::NameInput(mode) => {
            let title = match mode {
                Mode::Host => "Create a host",
                Mode::Guest => "Join as a guest",
            };

            draw_input_screen(
                frame,
                title,
                "Enter your name",
                &app.input,
                &app.error_message,
                "Enter: Continue   Esc: Back",
            );
        }

        Screen::GuestIpInput => {
            draw_input_screen(
                frame,
                "Join as a guest",
                "Enter host IP address",
                &app.input,
                &app.error_message,
                "Example: 192.168.1.10",
            );
        }

        Screen::GuestPortInput => {
            draw_input_screen(
                frame,
                "Join as a guest",
                "Enter host port",
                &app.input,
                &app.error_message,
                "Enter the port shown on the host screen",
            );
        }

        Screen::WaitingForGuest => draw_waiting(frame, app),
        Screen::Connecting => draw_connecting(frame, app),
        Screen::Chat => draw_chat(frame, app),
        Screen::Error => draw_error(frame, app),
    }
}

// 最初のメニュー画面を描画する。
fn draw_menu(frame: &mut Frame, app: &App) {
    let area = centered_rect(70, 75, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Length(2),
            Constraint::Length(8),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);

    let logo = Paragraph::new(vec![
        Line::from("██████╗ ███████╗███╗   ██╗███╗   ██╗██╗   ██╗"),
        Line::from("██╔══██╗██╔════╝████╗  ██║████╗  ██║╚██╗ ██╔╝"),
        Line::from("██████╔╝█████╗  ██╔██╗ ██║██╔██╗ ██║ ╚████╔╝ "),
        Line::from("██╔═══╝ ██╔══╝  ██║╚██╗██║██║╚██╗██║  ╚██╔╝  "),
        Line::from("██║     ███████╗██║ ╚████║██║ ╚████║   ██║   "),
        Line::from("╚═╝     ╚══════╝╚═╝  ╚═══╝╚═╝  ╚═══╝   ╚═╝   "),
    ])
    .alignment(Alignment::Center)
    .style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    );

    frame.render_widget(logo, chunks[0]);

    let subtitle = Paragraph::new("LAN terminal chat")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(subtitle, chunks[1]);

    let options = ["Create a host", "Join as a guest"];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let selected = index == app.selected_menu;

            let prefix = if selected { "❯" } else { " " };

            let style = if selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!("{prefix} {}. ", index + 1), style),
                Span::styled(*option, style),
            ]))
        })
        .collect();

    let menu = List::new(items).block(Block::default().title(" Menu ").borders(Borders::ALL));

    frame.render_widget(menu, chunks[2]);

    let help = Paragraph::new("↑/↓ or j/k: Select   Enter: Confirm   q/Q: Quit")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(help, chunks[4]);
}

// 名前、IPアドレス、ポート番号などの入力画面を共通描画する。
fn draw_input_screen(
    frame: &mut Frame,
    title: &str,
    label: &str,
    input: &str,
    error: &str,
    help: &str,
) {
    let area = centered_rect(65, 45, frame.area());

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(area);

    let heading = Paragraph::new(title)
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::default()
                .title(format!(" {APP_NAME} "))
                .borders(Borders::ALL),
        );

    frame.render_widget(heading, chunks[0]);

    frame.render_widget(
        Paragraph::new(label).style(Style::default().fg(Color::White)),
        chunks[1],
    );

    let shown_input = format!("{input}█");

    let input_box = Paragraph::new(shown_input)
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::ALL));

    frame.render_widget(input_box, chunks[2]);

    if !error.is_empty() {
        frame.render_widget(
            Paragraph::new(error).style(Style::default().fg(Color::Red)),
            chunks[3],
        );
    }

    frame.render_widget(
        Paragraph::new(help)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        chunks[4],
    );
}

// ホスト側で接続待ち中のIPアドレスとポートを表示する。
fn draw_waiting(frame: &mut Frame, app: &App) {
    let area = centered_rect(70, 55, frame.area());

    let port = app
        .host_port
        .map(|port| port.to_string())
        .unwrap_or_else(|| "-".to_string());

    let content = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            "Waiting for a guest...",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Name        : ", Style::default().fg(Color::DarkGray)),
            Span::raw(&app.name),
        ]),
        Line::from(vec![
            Span::styled("IP address  : ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.host_ip, Style::default().fg(Color::Green)),
        ]),
        Line::from(vec![
            Span::styled("Port        : ", Style::default().fg(Color::DarkGray)),
            Span::styled(port, Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
        Line::from("Enter this IP address and port on the guest Mac."),
        Line::from(""),
        Line::from("Esc: Cancel"),
    ];

    let panel = Paragraph::new(content).alignment(Alignment::Center).block(
        Block::default()
            .title(format!(" {APP_NAME} HOST "))
            .borders(Borders::ALL),
    );

    frame.render_widget(panel, area);
}

// ゲスト側で接続処理中の状態を表示する。
fn draw_connecting(frame: &mut Frame, app: &App) {
    let area = centered_rect(65, 35, frame.area());

    let panel = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            &app.status,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Esc: Cancel"),
    ])
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .title(format!(" {APP_NAME} "))
            .borders(Borders::ALL),
    );

    frame.render_widget(panel, area);
}

// チャット画面のヘッダー、メッセージ一覧、入力欄を描画する。
fn draw_chat(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(5),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Name: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                &app.name,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Peer: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.peer_address, Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Status: ", Style::default().fg(Color::DarkGray)),
            Span::styled(&app.status, Style::default().fg(Color::Green)),
        ]),
    ])
    .block(
        Block::default()
            .title(format!(" {APP_NAME} CHAT "))
            .borders(Borders::ALL),
    );

    frame.render_widget(header, chunks[0]);

    let available_lines = chunks[1].height.saturating_sub(2) as usize;
    let start = app.messages.len().saturating_sub(available_lines);

    let message_items: Vec<ListItem> = app.messages[start..]
        .iter()
        .map(|message| {
            let is_own_message = is_own_message(message, &app.name);

            let color = if is_own_message {
                Color::Cyan
            } else {
                Color::Green
            };

            ListItem::new(Line::from(Span::styled(
                message,
                Style::default().fg(color),
            )))
        })
        .collect();

    let messages =
        List::new(message_items).block(Block::default().title(" Messages ").borders(Borders::ALL));

    frame.render_widget(messages, chunks[1]);

    let input = Paragraph::new(format!("{}█", app.input))
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().title(" Message ").borders(Borders::ALL))
        .wrap(Wrap { trim: false });

    frame.render_widget(input, chunks[2]);

    let help = Paragraph::new("Enter: Send   /quit or Esc: Leave   Ctrl+C: Quit")
        .alignment(Alignment::Center)
        .style(Style::default().fg(Color::DarkGray));

    frame.render_widget(help, chunks[3]);
}

fn is_own_message(message: &str, name: &str) -> bool {
    let protocol_prefix = format!("{name}:");
    let display_prefix = format!("{name}：");

    message.starts_with(&protocol_prefix) || message.starts_with(&display_prefix)
}

// エラー内容を表示し、メニューへ戻れるようにする。
fn draw_error(frame: &mut Frame, app: &App) {
    let area = centered_rect(65, 35, frame.area());

    frame.render_widget(Clear, area);

    let panel = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "エラー",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(app.error_message.as_str()),
        Line::from(""),
        Line::from("EnterまたはEscでメニューに戻ります。"),
    ])
    .alignment(Alignment::Center)
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .title(format!(" {APP_NAME} "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red)),
    );

    frame.render_widget(panel, area);
}

// 指定した割合の矩形を画面中央に作る。
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}
