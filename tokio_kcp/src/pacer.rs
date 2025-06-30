use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::{mpsc, watch};
use tokio::time::{self, Duration};
use log::{error, info};

pub struct PacketPacer {
    pub(crate) packet_tx: mpsc::Sender<Vec<u8>>,
}

impl PacketPacer {
    pub fn new(
        socket: Arc<UdpSocket>,
        target_addr: SocketAddr,
        mut pacing_rate_rx: watch::Receiver<f32>,

    ) -> Self {
        let (packet_tx, mut packet_rx) = mpsc::channel::<Vec<u8>>(256);

        tokio::spawn(async move {
            let mut pacing_rate_rx = pacing_rate_rx.clone();
            let mut pacing_rate = *pacing_rate_rx.borrow();
            let mut interval = Self::calculate_interval(pacing_rate);
            let mut timer = time::interval(interval);
            timer.tick().await;

            loop {
                tokio::select! {
                    biased;
                    Ok(()) = pacing_rate_rx.changed() => {
                        pacing_rate = *pacing_rate_rx.borrow_and_update();
                        interval = Self::calculate_interval(pacing_rate);
                        timer.reset(); 
                        info!("Pacing rate updated to {} bps, interval is now {:?}.", pacing_rate, interval);
                    }

                    
                    _ = timer.tick() => {
                        match packet_rx.try_recv() {
                            Ok(packet) => {
                                if let Err(e) = socket.send_to(&packet, target_addr).await {
                                    error!("UDP send_to failed: {}", e);
                                }
                            }
                            Err(mpsc::error::TryRecvError::Empty) => {},
                            Err(mpsc::error::TryRecvError::Disconnected) => {
                                info!("Packet channel disconnected, pacer task is shutting down.");
                                break;
                            }
                        }
                    }
                }
            }
        });

        Self { packet_tx }
    }

    pub async fn send(&self, packet: Vec<u8>) -> Result<(), mpsc::error::SendError<Vec<u8>>> {
        self.packet_tx.send(packet).await
    }

    fn calculate_interval(pacing_rate_bps: f32) -> Duration {
        if pacing_rate_bps < 1.0 {
            return Duration::from_secs(1);
        }

        //                                             MSS = 1000
        let packets_per_second = pacing_rate_bps / (1000.0 * 8.0);
        if packets_per_second < 1.0 {
            return Duration::from_secs(1);
        }

        let interval_seconds = 1.0 / packets_per_second;
        Duration::from_secs_f32(interval_seconds)
    }
}