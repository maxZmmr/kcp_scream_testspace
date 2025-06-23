// /scream_kcp_lab/kcp_tester/src/main.rs

use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio_kcp::{KcpConfig, KcpListener, KcpStream};

#[tokio::main]
async fn main() -> std::io::Result<()> {

    tokio::spawn(async {
        run_server().await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_secs(1)).await;

    run_client().await?;

    Ok(())
}

async fn run_server() -> std::io::Result<()> {
    let config = KcpConfig::default();
    let mut listener = KcpListener::bind(config, "127.0.0.1:22333").await?;
    println!("Server lauscht auf 127.0.0.1:22333");

    let (mut stream, addr) = listener.accept().await?;
    println!("Server accepted connection from address: {}", addr);

    let mut buf = [0u8; 4096];
    loop {
        match stream.recv(&mut buf).await {
            Ok(n) => {
                let msg = std::str::from_utf8(&buf[..n]).unwrap();
                println!("Server recieved: '{}'", msg);
            }
            Err(e) => {
                eprintln!("Server error during receiving: {}", e);
                break;
            }
        }
    }
    Ok(())
}

async fn run_client() -> std::io::Result<()> {
    let config = KcpConfig::default();
    let server_addr: SocketAddr = "127.0.0.1:22333".parse().unwrap();
    
    let client_socket = UdpSocket::bind("127.0.0.1:0").await?;
    let client_addr = client_socket.local_addr()?;
    println!("Client runs on: {}", client_addr);

    let mut stream = KcpStream::connect(&config, server_addr).await?;
    println!("Client connected with: {}", server_addr);

    for i in 0..100 {
        let msg = format!("message: {}", i);
        println!("Client seding: '{}'", msg);
        stream.send(msg.as_bytes()).await?;
        
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }

    Ok(())
}