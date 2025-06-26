use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::io::AsyncWriteExt; // Wichtig für stream.shutdown()
use tokio_kcp::{KcpConfig, KcpListener, KcpStream};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Server in einem Hintergrund-Task starten
    let server_handle = tokio::spawn(async {
        if let Err(e) = run_server().await {
            eprintln!("Server-Fehler: {}", e);
        }
    });

    // Dem Server einen Moment Zeit zum Starten geben
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Client im Vordergrund ausführen
    if let Err(e) = run_client().await {
        eprintln!("Client-Fehler: {}", e);
    }

    // Auf das saubere Beenden des Servers warten
    server_handle.await.expect("Server-Task konnte nicht beendet werden.");
    println!("Programm beendet.");

    Ok(())
}

async fn run_server() -> std::io::Result<()> {
    let mut config = KcpConfig::default();
    config.nodelay.nc = true;
    config.use_external_congestion_control = true;
    let mut listener = KcpListener::bind(config, "0.0.0.0:22333").await?;
    println!("Server lauscht auf 0.0.0.0:22333");

    let (mut stream, addr) = listener.accept().await?;
    println!("Server: Verbindung von {} akzeptiert", addr);

    let mut buf = vec![0u8; 8192];
    let mut total_received_bytes = 0;
    let mut last_stat_time = Instant::now();
    let start_time = Instant::now();

    loop {
        match stream.recv(&mut buf).await {
            Ok(0) => {
                println!("\nServer: Verbindung von {} sauber geschlossen.", addr);
                break;
            }
            Ok(n) => {
                total_received_bytes += n;
                if let Err(e) = stream.send(&buf[..n]).await {
                    eprintln!("Server Sende-Fehler: {}", e);
                    break;
                }
                if last_stat_time.elapsed() >= Duration::from_secs(2) {
                    let rate_kbps = (total_received_bytes as f64 * 8.0) / (last_stat_time.elapsed().as_secs_f64() * 1000.0);
                    println!("[Server] Empfangsdurchsatz der letzten 2s: {:.2} kbps", rate_kbps);
                    total_received_bytes = 0;
                    last_stat_time = Instant::now();
                }
            }
            Err(e) => {
                eprintln!("Server Empfangs-Fehler: {}", e);
                break;
            }
        }
    }
    let total_duration = start_time.elapsed().as_secs_f64();
    println!("Server-Task beendet nach {:.2} Sekunden.", total_duration);
    Ok(())
}

async fn run_client() -> std::io::Result<()> {
    let mut config = KcpConfig::default();
    config.nodelay.nc = true;
    config.use_external_congestion_control = true;
    let server_addr: SocketAddr = "127.0.0.1:22333".parse().unwrap();

    println!("Client: Verbinde mit {}", server_addr);
    let mut stream = KcpStream::connect(&config, server_addr).await?;
    println!("Client: Verbunden.");

    let data_to_send = vec![1u8; 4096];
    let mut recv_buf = vec![0u8; 8192];
    let mut total_sent_bytes: u64 = 0;
    let mut total_received_bytes: u64 = 0;
    let start_time = Instant::now();
    let test_duration = Duration::from_secs(30);

    println!("Client: Sende und empfange Daten für {} Sekunden...", test_duration.as_secs());

    while start_time.elapsed() < test_duration {
        // First, try to send data
        match stream.send(&data_to_send).await {
            Ok(n) => {
                total_sent_bytes += n as u64;
            }
            Err(e) => {
                eprintln!("Client Sende-Fehler: {}", e);
                tokio::time::sleep(Duration::from_secs(1)).await;
                break;
            }
        }

        // Then, try to receive data
        match stream.recv(&mut recv_buf).await {
            Ok(0) => {
                println!("Client: Server hat die Verbindung geschlossen.");
                break;
            }
            Ok(n) => {
                total_received_bytes += n as u64;
            }
            Err(e) => {
                eprintln!("Client Empfangs-Fehler: {}", e);
                break;
            }
        }
    }

    let elapsed_secs = start_time.elapsed().as_secs_f64();
    let send_throughput_kbps = (total_sent_bytes as f64 * 8.0 / 1000.0) / elapsed_secs;
    let recv_throughput_kbps = (total_received_bytes as f64 * 8.0 / 1000.0) / elapsed_secs;

    println!("\n----------------------------------------");
    println!("Client: Test beendet.");
    println!("Gesamtdauer: {:.2} Sekunden", elapsed_secs);
    println!("Gesendet: {} bytes | Avg. Rate: {:.2} kbps", total_sent_bytes, send_throughput_kbps);
    println!("Empfangen: {} bytes | Avg. Rate: {:.2} kbps", total_received_bytes, recv_throughput_kbps);
    println!("----------------------------------------");

    stream.shutdown().await?;

    Ok(())
}