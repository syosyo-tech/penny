use std::{
    io::{BufRead, BufReader},
    net::{IpAddr, TcpStream, UdpSocket},
    sync::mpsc::Sender,
};

pub(crate) enum NetworkEvent {
    // 別スレッドのネットワーク処理からUIスレッドへ渡すイベント。
    Connected {
        stream: TcpStream,
        description: String,
    },
    Message(String),
    Disconnected,
    Failed(String),
}

// 相手から届く行単位のメッセージを読み、UIスレッドへ通知する。
pub(crate) fn receive_messages(stream: TcpStream, sender: Sender<NetworkEvent>) {
    let reader = BufReader::new(stream);

    for result in reader.lines() {
        match result {
            Ok(message) => {
                if sender.send(NetworkEvent::Message(message)).is_err() {
                    return;
                }
            }

            Err(error) => {
                let _ = sender.send(NetworkEvent::Failed(format!(
                    "メッセージを受信できませんでした: {error}"
                )));
                return;
            }
        }
    }

    let _ = sender.send(NetworkEvent::Disconnected);
}

// LAN内で使う自分のIPアドレスを推定する。
pub(crate) fn get_local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;

    // 実際の通信は発生せず、macOSが選ぶ送信元IPを取得するために使う。
    socket.connect("8.8.8.8:80").ok()?;

    socket.local_addr().ok().map(|address| address.ip())
}
