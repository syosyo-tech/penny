use std::{
    io::Write,
    mem::MaybeUninit,
    net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream},
    ptr,
    sync::mpsc::{Receiver, Sender},
    thread,
    time::Duration,
};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::network::{get_local_ip, receive_messages, NetworkEvent};

const CONNECT_TIMEOUT_SECONDS: u64 = 5;
const MAX_NAME_LENGTH: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    // 接続を待ち受ける側。
    Host,
    // ホストへ接続する側。
    Guest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Screen {
    // 現在表示している画面を表す。
    Menu,
    NameInput(Mode),
    GuestIpInput,
    GuestPortInput,
    WaitingForGuest,
    Connecting,
    Chat,
    Error,
}

// アプリ全体の状態を保持する。
pub(crate) struct App {
    pub(crate) screen: Screen,
    pub(crate) selected_menu: usize,

    pub(crate) input: String,
    pub(crate) name: String,
    guest_ip: String,

    pub(crate) host_ip: String,
    pub(crate) host_port: Option<u16>,
    pub(crate) peer_address: String,
    is_host: bool,

    pub(crate) status: String,
    pub(crate) error_message: String,
    pub(crate) messages: Vec<String>,

    stream: Option<TcpStream>,

    network_sender: Sender<NetworkEvent>,
    network_receiver: Receiver<NetworkEvent>,

    pub(crate) should_quit: bool,
}

impl App {
    // 初期状態はメニュー画面から始める。
    pub(crate) fn new(
        network_sender: Sender<NetworkEvent>,
        network_receiver: Receiver<NetworkEvent>,
    ) -> Self {
        Self {
            screen: Screen::Menu,
            selected_menu: 0,

            input: String::new(),
            name: String::new(),
            guest_ip: String::new(),

            host_ip: String::new(),
            host_port: None,
            peer_address: String::new(),
            is_host: false,

            status: String::new(),
            error_message: String::new(),
            messages: Vec::new(),

            stream: None,

            network_sender,
            network_receiver,

            should_quit: false,
        }
    }

    // 現在の画面に応じてキー入力の処理先を切り替える。
    pub(crate) fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        match self.screen.clone() {
            Screen::Menu => self.handle_menu_key(key),
            Screen::NameInput(mode) => self.handle_name_input_key(key, mode),
            Screen::GuestIpInput => self.handle_guest_ip_key(key),
            Screen::GuestPortInput => self.handle_guest_port_key(key),
            Screen::WaitingForGuest => self.handle_waiting_key(key),
            Screen::Connecting => self.handle_connecting_key(key),
            Screen::Chat => self.handle_chat_key(key),
            Screen::Error => self.handle_error_key(key),
        }
    }

    // メニュー画面ではホスト作成、ゲスト参加、終了を選択する。
    fn handle_menu_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_menu = self.selected_menu.saturating_sub(1);
            }

            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_menu = (self.selected_menu + 1).min(1);
            }

            KeyCode::Char('1') => {
                self.begin_name_input(Mode::Host);
            }

            KeyCode::Char('2') => {
                self.begin_name_input(Mode::Guest);
            }

            KeyCode::Enter => {
                let mode = if self.selected_menu == 0 {
                    Mode::Host
                } else {
                    Mode::Guest
                };

                self.begin_name_input(mode);
            }

            KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                self.should_quit = true;
            }

            _ => {}
        }
    }

    // 名前入力に進む前に入力欄とエラー表示を初期化する。
    fn begin_name_input(&mut self, mode: Mode) {
        self.input.clear();
        self.error_message.clear();
        self.screen = Screen::NameInput(mode);
    }

    // 名前を検証し、ホストまたはゲストの次の画面へ進める。
    fn handle_name_input_key(&mut self, key: KeyEvent, mode: Mode) {
        match key.code {
            KeyCode::Enter => {
                let name = self.input.trim().to_string();

                if name.is_empty() {
                    self.error_message = "名前を空欄にはできません。".to_string();
                    return;
                }

                if name.chars().count() > MAX_NAME_LENGTH {
                    self.error_message =
                        format!("名前は{MAX_NAME_LENGTH}文字以内で入力してください。");
                    return;
                }

                if name.contains(':') {
                    self.error_message = "名前には「:」を使用できません。".to_string();
                    return;
                }

                self.name = name;
                self.input.clear();
                self.error_message.clear();

                match mode {
                    Mode::Host => self.create_host(),
                    Mode::Guest => {
                        self.screen = Screen::GuestIpInput;
                    }
                }
            }

            KeyCode::Esc => {
                self.return_to_menu();
            }

            _ => self.edit_input(key),
        }
    }

    // ゲスト側で接続先のIPアドレスを入力する。
    fn handle_guest_ip_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let value = self.input.trim();

                match value.parse::<IpAddr>() {
                    Ok(_) => {
                        self.guest_ip = value.to_string();
                        self.input.clear();
                        self.error_message.clear();
                        self.screen = Screen::GuestPortInput;
                    }
                    Err(_) => {
                        self.error_message = "入力されたIPアドレスが正しくありません。".to_string();
                    }
                }
            }

            KeyCode::Esc => {
                self.input.clear();
                self.error_message.clear();
                self.screen = Screen::NameInput(Mode::Guest);
            }

            _ => self.edit_input(key),
        }
    }

    // ゲスト側で接続先のポート番号を入力する。
    fn handle_guest_port_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                let value = self.input.trim();

                match value.parse::<u16>() {
                    Ok(port) if port > 0 => {
                        self.error_message.clear();
                        self.connect_to_host(port);
                    }
                    _ => {
                        self.error_message =
                            "ポート番号は1〜65535の範囲で入力してください。".to_string();
                    }
                }
            }

            KeyCode::Esc => {
                self.input.clear();
                self.error_message.clear();
                self.screen = Screen::GuestIpInput;
            }

            _ => self.edit_input(key),
        }
    }

    fn handle_waiting_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            self.return_to_menu();
        }
    }

    fn handle_connecting_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Esc | KeyCode::Char('q')) {
            self.return_to_menu();
        }
    }

    fn handle_chat_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.send_current_message();
            }

            KeyCode::Esc => {
                self.disconnect();
                self.return_to_menu();
            }

            _ => self.edit_input(key),
        }
    }

    fn handle_error_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Enter | KeyCode::Esc | KeyCode::Char('q')) {
            self.return_to_menu();
        }
    }

    // 文字入力とBackspaceを共通処理にまとめる。
    fn edit_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char(character) => {
                self.input.push(character);
                self.error_message.clear();
            }

            KeyCode::Backspace => {
                self.input.pop();
                self.error_message.clear();
            }

            _ => {}
        }
    }

    // 空きポートでTCPリスナーを作り、ゲストからの接続を別スレッドで待つ。
    fn create_host(&mut self) {
        let listener = match TcpListener::bind("0.0.0.0:0") {
            Ok(listener) => listener,
            Err(error) => {
                self.show_error(format!("ホストを作成できませんでした: {error}"));
                return;
            }
        };

        let port = match listener.local_addr() {
            Ok(address) => address.port(),
            Err(error) => {
                self.show_error(format!("使用ポートを取得できませんでした: {error}"));
                return;
            }
        };

        self.host_ip = get_local_ip()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string());

        self.host_port = Some(port);
        self.is_host = true;
        self.status = "Waiting for a guest...".to_string();
        self.screen = Screen::WaitingForGuest;

        let sender = self.network_sender.clone();

        thread::spawn(move || match listener.accept() {
            Ok((stream, remote_address)) => {
                let description = format!("Guest connected from {remote_address}");

                let _ = sender.send(NetworkEvent::Connected {
                    stream,
                    description,
                    peer_address: remote_address.to_string(),
                });
            }

            Err(error) => {
                let _ = sender.send(NetworkEvent::Failed(format!(
                    "ゲストからの接続を受け付けられませんでした: {error}"
                )));
            }
        });
    }

    // ゲスト側からホストのIPアドレスとポートへ接続する。
    fn connect_to_host(&mut self, port: u16) {
        let ip = match self.guest_ip.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => {
                self.show_error("入力されたIPアドレスが正しくありません。".to_string());
                return;
            }
        };

        let address = SocketAddr::new(ip, port);

        self.input.clear();
        self.is_host = false;
        self.status = format!("Connecting to {address}...");
        self.screen = Screen::Connecting;

        let sender = self.network_sender.clone();

        thread::spawn(move || {
            let timeout = Duration::from_secs(CONNECT_TIMEOUT_SECONDS);

            match TcpStream::connect_timeout(&address, timeout) {
                Ok(stream) => {
                    let _ = sender.send(NetworkEvent::Connected {
                        stream,
                        description: format!("Connected to {address}"),
                        peer_address: address.to_string(),
                    });
                }

                Err(error) => {
                    let _ = sender.send(NetworkEvent::Failed(format!(
                        "ホストに接続できませんでした: {error}"
                    )));
                }
            }
        });
    }

    // ネットワークスレッドから届いたイベントをUI状態へ反映する。
    pub(crate) fn process_network_events(&mut self) {
        while let Ok(event) = self.network_receiver.try_recv() {
            match event {
                NetworkEvent::Connected {
                    stream,
                    description,
                    peer_address,
                } => {
                    self.start_chat(stream, description, peer_address);
                }

                NetworkEvent::Message(message) => {
                    self.messages.push(format_chat_message(&message));
                }

                NetworkEvent::Disconnected => {
                    if self.screen == Screen::Chat {
                        self.stream = None;

                        if self.is_host {
                            self.status = "The other user disconnected.".to_string();
                        } else {
                            self.should_quit = true;
                        }
                    }
                }

                NetworkEvent::Failed(message) => {
                    if matches!(self.screen, Screen::WaitingForGuest | Screen::Connecting) {
                        self.show_error(message);
                    }
                }
            }
        }
    }

    // 接続済みストリームを保存し、受信用スレッドを開始する。
    fn start_chat(&mut self, stream: TcpStream, description: String, peer_address: String) {
        let receive_stream = match stream.try_clone() {
            Ok(stream) => stream,
            Err(error) => {
                self.show_error(format!("通信接続を準備できませんでした: {error}"));
                return;
            }
        };

        self.stream = Some(stream);
        self.messages.clear();
        self.input.clear();
        self.peer_address = peer_address;
        self.status = description;
        self.screen = Screen::Chat;

        let sender = self.network_sender.clone();

        thread::spawn(move || {
            receive_messages(receive_stream, sender);
        });
    }

    // 入力中のメッセージを相手へ送信し、自分の画面にも追加する。
    fn send_current_message(&mut self) {
        let message = self.input.trim().to_string();

        if message.is_empty() {
            return;
        }

        if message == "/quit" {
            self.disconnect();
            self.return_to_menu();
            return;
        }

        let formatted_message = format!("{}:{}", self.name, message);

        let Some(stream) = self.stream.as_mut() else {
            self.show_error("通信接続が切断されています。".to_string());
            return;
        };

        if let Err(error) = writeln!(stream, "{formatted_message}") {
            self.show_error(format!("メッセージを送信できませんでした: {error}"));
            return;
        }

        if let Err(error) = stream.flush() {
            self.show_error(format!("メッセージを送信できませんでした: {error}"));
            return;
        }

        self.messages.push(format_chat_message(&formatted_message));
        self.input.clear();
    }

    // 接続中のTCPストリームがあれば両方向を閉じる。
    pub(crate) fn disconnect(&mut self) {
        if let Some(stream) = self.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
    }

    // チャットや入力途中の状態をリセットしてメニューへ戻る。
    fn return_to_menu(&mut self) {
        self.disconnect();

        self.screen = Screen::Menu;
        self.selected_menu = 0;

        self.input.clear();
        self.name.clear();
        self.guest_ip.clear();

        self.host_ip.clear();
        self.host_port = None;
        self.peer_address.clear();
        self.is_host = false;

        self.status.clear();
        self.error_message.clear();
        self.messages.clear();
    }

    // エラー画面に切り替える前に接続と入力状態を片付ける。
    fn show_error(&mut self, message: String) {
        self.disconnect();

        self.error_message = message;
        self.input.clear();
        self.screen = Screen::Error;
    }
}

fn format_chat_message(message: &str) -> String {
    let timestamp = current_timestamp();

    match message.split_once(':') {
        Some((name, body)) => format!("{name}：{body} [{timestamp}]"),
        None => format!("{message} [{timestamp}]"),
    }
}

fn current_timestamp() -> String {
    unsafe {
        let now = libc::time(ptr::null_mut());
        let mut local_time = MaybeUninit::<libc::tm>::uninit();

        if libc::localtime_r(&now, local_time.as_mut_ptr()).is_null() {
            return "--:--:--".to_string();
        }

        let local_time = local_time.assume_init();

        format!(
            "{:02}:{:02}:{:02}",
            local_time.tm_hour, local_time.tm_min, local_time.tm_sec
        )
    }
}
