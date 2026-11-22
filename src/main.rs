mod audio;
mod tls;

use std::net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result, ensure};
use audio::{AudioBuffer, BATCH_BYTES, VirtualMic};
use axum::Router;
use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use hyper::server::conn::http1;
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use qrcode::{QrCode, render::unicode};
use rand::RngCore;
use tokio::net::TcpListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::Semaphore;
use tokio_rustls::TlsAcceptor;

#[derive(Clone)]
struct App {
    origin: String,
    token: String,
    audio: Arc<Mutex<AudioBuffer>>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let first = args.next();
    if matches!(first.as_deref(), Some("-h" | "--help")) {
        println!(
            "WebMic: cargo run -- [LAN-IP] [PORT]\nStandard: automatisch erkannte IPv4, Port 8443"
        );
        return Ok(());
    }
    let ip = match first {
        Some(ip) => ip.parse::<IpAddr>().context("Ungültige LAN-IP")?,
        None => lan_ip().context("LAN-IP nicht ermittelbar. Starte cargo run -- <LAN-IP>")?,
    };
    let port = args
        .next()
        .map(|port| port.parse::<u16>())
        .transpose()?
        .unwrap_or(8443);
    ensure!(
        !ip.is_unspecified() && port != 0 && args.next().is_none(),
        "WebMic benötigt eine konkrete IP und einen Port ungleich 0"
    );
    let address = SocketAddr::new(ip, port);
    let listener = TcpListener::bind(address)
        .await
        .with_context(|| format!("HTTPS an {address} binden"))?;

    let cert_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".webmic");
    let tls = TlsAcceptor::from(tls::configure(ip, &cert_dir)?);
    let mic = VirtualMic::create()?;
    let audio = Arc::new(Mutex::new(AudioBuffer::default()));
    let mut random = [0_u8; 16];
    rand::rngs::OsRng
        .try_fill_bytes(&mut random)
        .context("Zufälligen Verbindungslink erstellen")?;
    let token: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    let origin = format!("https://{address}");
    let url = format!("{origin}/#{token}");
    let router = router(App {
        origin,
        token,
        audio: audio.clone(),
    });
    let mut terminate = signal(SignalKind::terminate())?;

    println!("\nWebMic · Im Browser des iPhones öffnen\n{url}\n");
    if let Ok(qr) = QrCode::new(&url) {
        println!("{}", qr.render::<unicode::Dense1x2>().build());
    }
    println!(
        "iPhone-Zertifikat: {}",
        cert_dir.join("webmic-ca.cer").display()
    );
    println!("Audioeingang: WebMic · Beenden: Strg+C");

    tokio::select! {
        result = serve(listener, tls, router) => result,
        result = mic.run(audio) => result,
        result = tokio::signal::ctrl_c() => result.context("Auf Strg+C warten"),
        _ = terminate.recv() => Ok(()),
    }
}

fn lan_ip() -> std::io::Result<IpAddr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
    // UDP connect selects a local route without sending a packet.
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9))?;
    Ok(socket.local_addr()?.ip())
}

fn router(app: App) -> Router {
    Router::new()
        .route("/", get(|| async { Html(include_str!("../web/index.html")) }))
        .route("/style.css", get(|| async { asset("text/css", include_str!("../web/style.css")) }))
        .route("/app.js", get(|| async { asset("text/javascript", include_str!("../web/app.js")) }))
        .route("/worklet.js", get(|| async { asset("text/javascript", include_str!("../web/worklet.js")) }))
        .route("/audio", post(receive_audio).delete(stop_audio))
        .layer(DefaultBodyLimit::max(BATCH_BYTES))
        .layer(axum::middleware::map_response(|mut response: axum::response::Response| async move {
            let headers = response.headers_mut();
            headers.insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
            headers.insert(header::REFERRER_POLICY, "no-referrer".parse().unwrap());
            headers.insert(header::CONTENT_SECURITY_POLICY, "default-src 'self'; object-src 'none'; frame-ancestors 'none'; base-uri 'none'".parse().unwrap());
            response
        }))
        .with_state(app)
}

fn asset(mime: &'static str, body: &'static str) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, mime)], body)
}

fn authorize<'a>(app: &App, headers: &'a HeaderMap) -> Result<&'a str, StatusCode> {
    let value = |key: &str| headers.get(key).and_then(|value| value.to_str().ok());
    if value("origin") != Some(app.origin.as_str())
        || value("x-webmic-token") != Some(app.token.as_str())
    {
        return Err(StatusCode::FORBIDDEN);
    }
    value("x-webmic-session")
        .filter(|session| {
            session.len() == 36
                && session
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        })
        .ok_or(StatusCode::BAD_REQUEST)
}

async fn receive_audio(State(app): State<App>, headers: HeaderMap, body: Bytes) -> StatusCode {
    let session = match authorize(&app, &headers) {
        Ok(session) => session,
        Err(status) => return status,
    };
    if body.len() != BATCH_BYTES {
        return StatusCode::BAD_REQUEST;
    }
    if app.audio.lock().unwrap().push(session, body) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::CONFLICT
    }
}

async fn stop_audio(State(app): State<App>, headers: HeaderMap) -> StatusCode {
    let session = match authorize(&app, &headers) {
        Ok(session) => session,
        Err(status) => return status,
    };
    app.audio.lock().unwrap().stop(session);
    StatusCode::NO_CONTENT
}

async fn serve(listener: TcpListener, tls: TlsAcceptor, router: Router) -> Result<()> {
    let connections = Arc::new(Semaphore::new(32));
    loop {
        let (socket, _) = listener.accept().await?;
        let Ok(permit) = connections.clone().try_acquire_owned() else {
            continue;
        };
        socket.set_nodelay(true)?;
        let tls = tls.clone();
        let service = TowerToHyperService::new(router.clone());
        tokio::spawn(async move {
            let _permit = permit;
            if let Ok(Ok(stream)) =
                tokio::time::timeout(Duration::from_secs(5), tls.accept(socket)).await
            {
                let _ = http1::Builder::new()
                    .timer(hyper_util::rt::TokioTimer::new())
                    .header_read_timeout(Duration::from_secs(15))
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Method, Request};
    use tower::ServiceExt;

    use super::*;

    #[tokio::test]
    async fn audio_endpoint_requires_link_and_allows_one_sender() {
        const ORIGIN: &str = "https://127.0.0.1:8443";
        const FIRST: &str = "11111111-1111-1111-1111-111111111111";
        const SECOND: &str = "22222222-2222-2222-2222-222222222222";
        let router = router(App {
            origin: ORIGIN.to_owned(),
            token: "secret".to_owned(),
            audio: Arc::new(Mutex::new(AudioBuffer::default())),
        });
        let request = |method, origin, token, session, size| {
            Request::builder()
                .method(method)
                .uri("/audio")
                .header("origin", origin)
                .header("x-webmic-token", token)
                .header("x-webmic-session", session)
                .body(Body::from(vec![0; size]))
                .unwrap()
        };
        for (method, origin, token, session, size, expected) in [
            (
                Method::POST,
                ORIGIN,
                "bad",
                FIRST,
                BATCH_BYTES,
                StatusCode::FORBIDDEN,
            ),
            (
                Method::POST,
                "https://other",
                "secret",
                FIRST,
                BATCH_BYTES,
                StatusCode::FORBIDDEN,
            ),
            (
                Method::POST,
                ORIGIN,
                "secret",
                FIRST,
                1,
                StatusCode::BAD_REQUEST,
            ),
            (
                Method::POST,
                ORIGIN,
                "secret",
                FIRST,
                BATCH_BYTES + 1,
                StatusCode::PAYLOAD_TOO_LARGE,
            ),
            (
                Method::POST,
                ORIGIN,
                "secret",
                FIRST,
                BATCH_BYTES,
                StatusCode::NO_CONTENT,
            ),
            (
                Method::POST,
                ORIGIN,
                "secret",
                SECOND,
                BATCH_BYTES,
                StatusCode::CONFLICT,
            ),
            (
                Method::DELETE,
                ORIGIN,
                "secret",
                FIRST,
                0,
                StatusCode::NO_CONTENT,
            ),
            (
                Method::POST,
                ORIGIN,
                "secret",
                SECOND,
                BATCH_BYTES,
                StatusCode::NO_CONTENT,
            ),
        ] {
            let response = router
                .clone()
                .oneshot(request(method, origin, token, session, size))
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }
}
