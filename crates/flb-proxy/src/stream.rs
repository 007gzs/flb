use flb_core::{StreamConfig, StreamProtocol};
use flb_store::Store;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream, UdpSocket};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

pub async fn run_streams(store: Arc<Store>) {
    let mut generation = 0u64;
    let mut tasks: Vec<JoinHandle<()>> = Vec::new();
    loop {
        let current = store.generation();
        if current != generation {
            generation = current;
            for task in tasks.drain(..) {
                task.abort();
            }
            let streams = store.snapshot().streams.clone();
            info!(count = streams.len(), "reloading stream listeners");
            for stream in streams {
                tasks.push(tokio::spawn(run_one(stream)));
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn run_one(stream: StreamConfig) {
    match stream.protocol {
        StreamProtocol::Tcp => {
            if let Err(err) = run_tcp(&stream).await {
                warn!(id = %stream.id, error = %err, "tcp stream stopped");
            }
        }
        StreamProtocol::Udp => {
            if let Err(err) = run_udp(&stream).await {
                warn!(id = %stream.id, error = %err, "udp stream stopped");
            }
        }
    }
}

async fn run_tcp(stream: &StreamConfig) -> std::io::Result<()> {
    let bind = format!("0.0.0.0:{}", stream.listen_port);
    let listener = TcpListener::bind(&bind).await?;
    let target = format!("{}:{}", stream.target_ip, stream.target_port);
    info!(listen = %bind, target = %target, "tcp stream listening");
    loop {
        let (mut inbound, peer) = listener.accept().await?;
        let target = target.clone();
        tokio::spawn(async move {
            match TcpStream::connect(&target).await {
                Ok(mut outbound) => {
                    let _ = inbound.set_nodelay(true);
                    let _ = outbound.set_nodelay(true);
                    if let Err(err) = copy_bidirectional(&mut inbound, &mut outbound).await {
                        warn!(peer = %peer, error = %err, "tcp proxy error");
                    }
                }
                Err(err) => warn!(peer = %peer, error = %err, "tcp connect failed"),
            }
        });
    }
}

async fn run_udp(stream: &StreamConfig) -> std::io::Result<()> {
    let bind = format!("0.0.0.0:{}", stream.listen_port);
    let socket = Arc::new(UdpSocket::bind(&bind).await?);
    let target: SocketAddr = format!("{}:{}", stream.target_ip, stream.target_port)
        .parse()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?;
    info!(listen = %bind, target = %target, "udp stream listening");
    let sessions: Arc<Mutex<HashMap<SocketAddr, Arc<UdpSocket>>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let last_seen: Arc<Mutex<HashMap<SocketAddr, Instant>>> = Arc::new(Mutex::new(HashMap::new()));

    let gc_sessions = sessions.clone();
    let gc_seen = last_seen.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(30)).await;
            let mut seen = gc_seen.lock().await;
            let mut map = gc_sessions.lock().await;
            seen.retain(|addr, at| {
                if at.elapsed() > Duration::from_secs(60) {
                    map.remove(addr);
                    false
                } else {
                    true
                }
            });
        }
    });

    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let (n, src) = socket.recv_from(&mut buf).await?;
        last_seen.lock().await.insert(src, Instant::now());
        let mut map = sessions.lock().await;
        if let std::collections::hash_map::Entry::Vacant(entry) = map.entry(src) {
            let upstream = UdpSocket::bind("0.0.0.0:0").await?;
            upstream.connect(target).await?;
            let upstream = Arc::new(upstream);
            entry.insert(upstream.clone());
            let listen = socket.clone();
            let upstream_rx = upstream.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                while let Ok(n) = upstream_rx.recv(&mut buf).await {
                    if listen.send_to(&buf[..n], src).await.is_err() {
                        break;
                    }
                }
            });
        }
        if let Some(upstream) = map.get(&src)
            && let Err(err) = upstream.send(&buf[..n]).await
        {
            warn!(peer = %src, error = %err, "udp send failed");
        }
    }
}
