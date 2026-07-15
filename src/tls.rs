use std::{
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{Arc, Mutex},
    time::Duration,
};

use rustls::{
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    crypto::CryptoProvider,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection,
    SignatureScheme, StreamOwned,
};
use sha2::{Digest, Sha256};

// TLSで使う自己署名証明書と秘密鍵をその場で生成する。ホストの起動ごとに使い捨てる。
pub(crate) fn generate_self_signed_cert(
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>), String> {
    let certified_key = rcgen::generate_simple_self_signed(vec!["penny".to_string()])
        .map_err(|error| format!("証明書を生成できませんでした: {error}"))?;

    let cert_der = certified_key.cert.der().clone();
    let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
        certified_key.signing_key.serialize_der(),
    ));

    Ok((cert_der, key_der))
}

// フィンガープリントとして使うバイト数。SHA-256(32バイト)全体だと画面幅に収まらず
// 手入力も現実的でないため、人間が読み書きしやすい長さに切り詰める。
// このアプリの脅威モデル（その場限りの1回の接続を横取りしようとする攻撃者が、
// 声などの別経路を盗聴せずに一致する証明書を用意する）に対しては、
// 8バイト(64bit)の総当たりは1回の接続試行の中では現実的に不可能なため十分な強度がある。
pub(crate) const FINGERPRINT_BYTE_LENGTH: usize = 8;

// 読み取りソケットがデータなしでブロックし続ける最大時間。この間隔ごとに
// 書き込み側がMutexを取得できるチャンスが巡ってくる。
const READ_POLL_TIMEOUT: Duration = Duration::from_millis(100);

// 証明書のフィンガープリント(SHA-256の先頭部分)を「AA:BB:CC...」の形式で返す。
pub(crate) fn fingerprint_of(cert: &CertificateDer<'_>) -> String {
    let digest = Sha256::digest(cert.as_ref());

    digest[..FINGERPRINT_BYTE_LENGTH]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

// ユーザーが入力したフィンガープリントを比較用に正規化する（区切り文字を除去し大文字化）。
pub(crate) fn normalize_fingerprint(input: &str) -> String {
    input
        .chars()
        .filter(|character| character.is_ascii_hexdigit())
        .collect::<String>()
        .to_uppercase()
}

// ホスト用のTLS設定を、生成した自己署名証明書から組み立てる。
pub(crate) fn build_server_config(
    cert: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<ServerConfig>, String> {
    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key)
        .map_err(|error| format!("TLS設定を作成できませんでした: {error}"))?;

    Ok(Arc::new(config))
}

// ゲストが入力したフィンガープリントとだけ照合し、CA検証は行わないTLSクライアント設定を組み立てる。
pub(crate) fn build_client_config(expected_fingerprint: &str) -> Arc<ClientConfig> {
    let provider = CryptoProvider::get_default()
        .expect("暗号プロバイダが初期化されていません")
        .clone();

    let verifier = Arc::new(PinnedFingerprintVerifier {
        expected_fingerprint: normalize_fingerprint(expected_fingerprint),
        provider,
    });

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    Arc::new(config)
}

// 受け入れた生TCP接続をTLSサーバとして扱い、ハンドシェイクを完了させてから返す。
pub(crate) fn accept_tls(
    config: Arc<ServerConfig>,
    tcp_stream: TcpStream,
) -> Result<SharedTlsStream, String> {
    let connection = ServerConnection::new(config)
        .map_err(|error| format!("TLS接続を初期化できませんでした: {error}"))?;

    let mut stream = StreamOwned::new(connection, tcp_stream);

    stream
        .conn
        .complete_io(&mut stream.sock)
        .map_err(|error| format!("TLSハンドシェイクに失敗しました: {error}"))?;

    set_read_poll_timeout(&stream.sock)?;

    Ok(SharedTlsStream::new(TlsStream::Server(stream)))
}

// 接続済みの生TCPソケットでTLSクライアントとしてハンドシェイクを行う。
// ここでの証明書検証がフィンガープリント不一致で失敗すればハンドシェイク自体が失敗する。
pub(crate) fn connect_tls(
    config: Arc<ClientConfig>,
    tcp_stream: TcpStream,
) -> Result<SharedTlsStream, String> {
    let server_name = ServerName::try_from("penny")
        .expect("固定のサーバ名リテラルは常に有効です")
        .to_owned();

    let connection = ClientConnection::new(config, server_name)
        .map_err(|error| format!("TLS接続を初期化できませんでした: {error}"))?;

    let mut stream = StreamOwned::new(connection, tcp_stream);

    stream
        .conn
        .complete_io(&mut stream.sock)
        .map_err(|error| format!("TLSハンドシェイクに失敗しました: {error}"))?;

    set_read_poll_timeout(&stream.sock)?;

    Ok(SharedTlsStream::new(TlsStream::Client(stream)))
}

// 読み取り用スレッドがブロッキングreadで無期限にMutexを握り続けないよう、
// データが来ていない間は定期的にWouldBlock/TimedOutで制御を返すタイムアウトを設定する。
// ハンドシェイク自体は完全なブロッキングソケットで完了させたいので、完了後にのみ設定する。
fn set_read_poll_timeout(tcp_stream: &TcpStream) -> Result<(), String> {
    tcp_stream
        .set_read_timeout(Some(READ_POLL_TIMEOUT))
        .map_err(|error| format!("読み取りタイムアウトを設定できませんでした: {error}"))
}

// CAチェーンの検証は行わず、相手の証明書のフィンガープリントが期待値と一致するかだけを見る。
// 署名自体の妥当性検証（相手が本当にその証明書の秘密鍵を持っているか）はプロバイダの実装に委譲する。
#[derive(Debug)]
struct PinnedFingerprintVerifier {
    expected_fingerprint: String,
    provider: Arc<CryptoProvider>,
}

impl ServerCertVerifier for PinnedFingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let actual_fingerprint = normalize_fingerprint(&fingerprint_of(end_entity));

        if actual_fingerprint == self.expected_fingerprint {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "相手のフィンガープリントが一致しませんでした。".to_string(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

// ホスト・ゲストどちらの役割でも扱えるようにするためのラッパー。
enum TlsStream {
    Server(StreamOwned<ServerConnection, TcpStream>),
    Client(StreamOwned<ClientConnection, TcpStream>),
}

impl TlsStream {
    fn tcp_stream(&self) -> &TcpStream {
        match self {
            TlsStream::Server(stream) => &stream.sock,
            TlsStream::Client(stream) => &stream.sock,
        }
    }
}

impl Read for TlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            TlsStream::Server(stream) => stream.read(buf),
            TlsStream::Client(stream) => stream.read(buf),
        }
    }
}

impl Write for TlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            TlsStream::Server(stream) => stream.write(buf),
            TlsStream::Client(stream) => stream.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            TlsStream::Server(stream) => stream.flush(),
            TlsStream::Client(stream) => stream.flush(),
        }
    }
}

// 読み取り用スレッドと書き込み側で1本のTLS接続を共有するためのハンドル。
// rustlsの暗号状態はTcpStreamのようにfdを複製して独立させることができないため、
// Arc<Mutex<..>>で共有し、読み書きのたびにロックを取る。
#[derive(Clone)]
pub(crate) struct SharedTlsStream(Arc<Mutex<TlsStream>>);

impl SharedTlsStream {
    fn new(stream: TlsStream) -> Self {
        Self(Arc::new(Mutex::new(stream)))
    }

    // 接続中のTCPソケットを両方向とも閉じる。
    pub(crate) fn shutdown(&self) {
        if let Ok(stream) = self.0.lock() {
            let _ = stream.tcp_stream().shutdown(Shutdown::Both);
        }
    }
}

impl Read for SharedTlsStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .read(buf)
    }
}

impl Write for SharedTlsStream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .flush()
    }
}

