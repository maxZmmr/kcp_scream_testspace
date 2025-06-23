use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio_kcp::{KcpConfig, KcpListener, KcpStream};
use tokio::io::AsyncWriteExt;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    tokio::spawn(async {
        if let Err(e) = run_server().await {
            eprintln!("Server error: {}", e);
        }
    });

    tokio::time::sleep(Duration::from_secs(1)).await;

    if let Err(e) = run_client().await {
        eprintln!("Client error: {}", e);
    }

    tokio::time::sleep(Duration::from_secs(2)).await;

    Ok(())
}

async fn run_server() -> std::io::Result<()> {
    let mut config = KcpConfig::default();
    config.nodelay.nc = true; 
    let mut listener = KcpListener::bind(config, "0.0.0.0:22333").await?;
    println!("Server lauscht auf 0.0.0.0:22333");

    let (mut stream, addr) = listener.accept().await?;
    println!("Server: Verbindung von {} akzeptiert", addr);

    let mut buf = vec![0u8; 8192];
    let mut total_received_bytes = 0;
    let start_time = Instant::now();
    let mut last_stat_time = Instant::now();

    loop {
        match stream.recv(&mut buf).await {
            Ok(0) => {
                println!("Server: Verbindung von {} geschlossen.", addr);
                break;
            }
            Ok(n) => {
                total_received_bytes += n;
                if let Err(e) = stream.send(&buf[..n]).await {
                    eprintln!("Server send error: {}", e);
                    break;
                }

                if last_stat_time.elapsed() >= Duration::from_secs(2) {
                    let elapsed_secs = start_time.elapsed().as_secs_f64();
                    let throughput_kbps = (total_received_bytes as f64 * 8.0 / 1000.0) / elapsed_secs;
                    println!(
                        "Server: Aktueller Empfangsdurchsatz: {:.2} kbps",
                        throughput_kbps
                    );
                    last_stat_time = Instant::now();
                }
            }
            Err(e) => {
                eprintln!("Server recv error: {}", e);
                break;
            }
        }
    }
    Ok(())
}

async fn run_client() -> std::io::Result<()> {
    let mut config = KcpConfig::default();
    config.nodelay.nc = true;
    let server_addr: SocketAddr = "127.0.0.1:22333".parse().unwrap();

    println!("Client: Verbinde mit {}", server_addr);
    let mut stream = KcpStream::connect(&config, server_addr).await?;
    println!("Client: Verbunden.");

    let data_to_send = vec![1u8; 4096];
    let mut total_sent_bytes = 0;
    let start_time = Instant::now();

    println!("Client: Sende Daten für 15 Sekunden...");

    while start_time.elapsed() < Duration::from_secs(15) {
        match stream.send(&data_to_send).await {
            Ok(n) => {
                total_sent_bytes += n;
            }
            Err(e) => {
                eprintln!("Client send error: {}", e);
                break;
            }
        }
    }

    let elapsed_secs = start_time.elapsed().as_secs_f64();
    let throughput_kbps = (total_sent_bytes as f64 * 8.0 / 1000.0) / elapsed_secs;
    println!("\n----------------------------------------");
    println!("Client: Senden beendet.");
    println!("Gesendete Daten: {} bytes", total_sent_bytes);
    println!("Dauer: {:.2} Sekunden", elapsed_secs);
    println!("Durchschnittlicher Sendedurchsatz: {:.2} kbps", throughput_kbps);
    println!("----------------------------------------");

    stream.shutdown().await?;

    Ok(())
}