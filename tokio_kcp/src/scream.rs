
use std::{cmp::min, collections::HashMap, time::{Duration, Instant}};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::SystemTime;


const BASE_RTT_WINDOW: Duration = Duration::from_secs(10);
const QDELAY_TARGET_LO: Duration = Duration::from_millis(100);
const QDELAY_TARGET_HI: Duration = Duration::from_millis(400);



#[derive(Debug)]
struct PacketInfo {
    timestamp: Instant,
    size: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ScreamState {
    Normal,
    Reduce,
    Accelerate,
}

#[derive(Debug)]
pub struct ScreamCongestionControl {
    target_bitrate: u64,
    rtt: Duration,
    base_rtt: Duration,
    min_rtt_in_window: Duration,
    base_rtt_update_time: Instant,

    qdelay_target: Duration,
    state: ScreamState,
    last_rtt_update_time: Instant,
    mult_bitrate_reduction: f64, 
    mult_bitrate_increase: f64,
    r_target: u64, //estimated bandwidth in bps
    last_congestion_event_time: Instant,

    target_delay: Duration,
    last_update: Instant,
    packets_in_flight: HashMap<u32, PacketInfo>,
    packet_loss_occured: bool,

}

impl ScreamCongestionControl {
    pub fn new() -> Self {
        Self {
            target_bitrate: 1_000_000, // start at 1 Mbps
            rtt: Duration::from_millis(100), 
            base_rtt: Duration::from_secs(10),
            min_rtt_in_window: Duration::from_secs(10),
            base_rtt_update_time: Instant::now() + BASE_RTT_WINDOW,

            qdelay_target: QDELAY_TARGET_LO,
            state: ScreamState::Normal,
            last_rtt_update_time: Instant::now(),
            mult_bitrate_reduction: 0.9,  // Typische Werte
            mult_bitrate_increase: 1.05,  // Typische Werte
            r_target: 1_000_000, 
            last_congestion_event_time: Instant::now(),

            target_delay: Duration::from_millis(100),
            last_update: Instant::now(),
            packets_in_flight: HashMap::new(),
            packet_loss_occured: false,
        }
    }


    pub fn on_packet_sent(&mut self, seq_number: u32, size: usize) {
        let info = PacketInfo{ timestamp: Instant::now(),size };
        self.packets_in_flight.insert(seq_number, info);
    }
    
    pub fn on_ack(&mut self, seq_number: u32) {
        if let Some(info) = self.packets_in_flight.remove(&seq_number) {
            
            // rtt and min roundtrip time over 10s
            let latest_rtt = info.timestamp.elapsed();

            if latest_rtt.as_nanos() == 0 {
                return;
            }
            
            self.rtt = latest_rtt;
            self.last_rtt_update_time = Instant::now();
            self.min_rtt_in_window = min(self.min_rtt_in_window, latest_rtt);
            let now = Instant::now();
            let time_since_last_base_rtt_update = now.duration_since(self.last_rtt_update_time)



            if now >= self.base_rtt_update_time {
                self.base_rtt = self.min_rtt_in_window;

                self.min_rtt_in_window = Duration::from_secs(10);
                self.base_rtt_update_time = now + BASE_RTT_WINDOW;
            }

            let queuing_delay = self.rtt.saturating_sub(self.base_rtt);

            let delay_diff = queuing_delay.as_secs_f64() - self.target_delay.as_secs_f64();
            if delay_diff > 0.0 {
                let reduction_factor = 1.0 - (delay_diff * 2.0).min(0.5);
                self.target_bitrate = ((self.target_bitrate as f64) * reduction_factor) as u64;
            } else {
                self.target_bitrate += 250_000; // increase by 250 kbps
            }

            self.target_bitrate = self.target_bitrate.clamp(500_000, 10_000_000); // clamp between  500kbps and 10 Mbps
        }
    }

    pub fn on_packet_loss(&mut self) {
        self.target_bitrate = (self.target_bitrate as f64 * 0.7) as u64;
        self.target_bitrate = self.target_bitrate.clamp(500_000, 10_000_000); // clamp between  500kbps and 10 Mbps
        self.packet_loss_occured = true;
    }
    


    pub fn get_cwnd(&mut self) -> u32 {
        let bitrate_bps = self.target_bitrate;
        let rtt_sec = self.rtt.as_secs_f64();
        let bytes_per_sec = bitrate_bps / 8;

        let cwnd = (bytes_per_sec as f64 * rtt_sec) as u32;


        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let packet_loss_indicator = if self.packet_loss_occured { 1 } else { 0 };
        let log_line = format!(
            "{},{},{},{}, {}\n",
            timestamp,
            self.rtt.as_millis(),
            self.target_bitrate / 1000,
            cwnd,
            packet_loss_indicator,
        );
        self.packet_loss_occured = false;

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("scream_log.csv")
        {
            if file.metadata().unwrap().len() == 0 {
                let _ = file.write(b"timestamp_ms,rtt_ms,bitrate_kbps,cwnd_bytes,packet_loss\n");
            }   
            let _ = file.write_all(log_line.as_bytes());
        }





        // println!("[SCReAM] RTT: {:?}, Target Bitrate: {} kbps, Calculated CWND: {}", self.rtt, self.target_bitrate / 1000, cwnd);
        cwnd
    }
}