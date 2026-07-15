use std::{
    io::{ErrorKind, Read},
    net::{IpAddr, UdpSocket},
    sync::mpsc::Sender,
    thread,
    time::Duration,
};

use crate::tls::SharedTlsStream;

pub(crate) enum NetworkEvent {
    // 別スレッドのネットワーク処理からUIスレッドへ渡すイベント。
    Connected {
        stream: SharedTlsStream,
        description: String,
        peer_address: String,
    },
    Message(String),
    Disconnected,
    Failed(String),
}

// タイムアウト直後に間を置かずロックを取り直すと、OSのMutexは公平性を保証しないため
// 書き込み側が長時間ロックを取得できなくなることがある(ロックコンボイ)。
// リトライ前に短く眠ることで、書き込み側に確実にロックを取る隙を与える。
const RETRY_BACKOFF: Duration = Duration::from_millis(5);

// 相手から届く行単位のメッセージを読み、UIスレッドへ通知する。
// 読み取りソケットにはタイムアウトが設定されているため、データが来ていない間は
// WouldBlock/TimedOutが定期的に返ってくる。その間だけ書き込み側がMutexを取得できるので、
// エラー扱いにはせずそのままリトライする。
pub(crate) fn receive_messages(mut stream: SharedTlsStream, sender: Sender<NetworkEvent>) {
    let mut buffer = Vec::new();
    let mut read_chunk = [0_u8; 1024];

    loop {
        match stream.read(&mut read_chunk) {
            Ok(0) => {
                let _ = sender.send(NetworkEvent::Disconnected);
                return;
            }

            Ok(byte_count) => {
                buffer.extend_from_slice(&read_chunk[..byte_count]);

                while let Some(newline_index) = buffer.iter().position(|&byte| byte == b'\n') {
                    let line_bytes: Vec<u8> = buffer.drain(..=newline_index).collect();
                    let line = String::from_utf8_lossy(&line_bytes)
                        .trim_end_matches(['\r', '\n'])
                        .to_string();

                    if sender.send(NetworkEvent::Message(line)).is_err() {
                        return;
                    }
                }
            }

            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                thread::sleep(RETRY_BACKOFF);
                continue;
            }

            Err(error) => {
                let _ = sender.send(NetworkEvent::Failed(format!(
                    "メッセージを受信できませんでした: {error}"
                )));
                return;
            }
        }
    }
}

// LAN内で使う自分のIPアドレスを推定する。
pub(crate) fn get_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;

    // 実際の通信は発生せず、macOSが選ぶ送信元IPを取得するために使う。
    socket.connect("8.8.8.8:80").ok()?;

    socket.local_addr().ok().map(|address| address.ip())
}
