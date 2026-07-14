mod app;
mod network;
mod ui;

use std::{io, sync::mpsc, time::Duration};

use app::App;
use crossterm::{
    cursor::Show,
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

pub(crate) const APP_NAME: &str = "PENNY";
const EVENT_POLL_INTERVAL_MS: u64 = 50;

// エラーが起きた場合は内容を表示して終了する。
fn main() {
    if let Err(error) = run() {
        eprintln!("エラー: {error}");
        std::process::exit(1);
    }
}

// ターミナルをTUI用の状態に切り替え、終了時に元の表示へ戻す。
fn run() -> io::Result<()> {
    enable_raw_mode()?;

    let mut stdout = io::stdout();

    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_application(&mut terminal);

    disable_raw_mode()?;

    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
        Show
    )?;

    terminal.show_cursor()?;

    result
}

// 画面描画、キー入力、ネットワークイベント処理を繰り返すメインループ。
fn run_application(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let (network_sender, network_receiver) = mpsc::channel();
    let mut app = App::new(network_sender, network_receiver);

    loop {
        app.process_network_events();

        terminal.draw(|frame| ui::draw(frame, &app))?;

        if app.should_quit {
            break;
        }

        if event::poll(Duration::from_millis(EVENT_POLL_INTERVAL_MS))? {
            let event = event::read()?;

            match event {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.handle_key(key);
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }

    app.disconnect();

    Ok(())
}
