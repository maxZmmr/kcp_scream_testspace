use std::{cmp::min, time::{Duration, Instant}};

#[derive(Debug)]
pub struct ScreamCongestionControl {
    target_bitrate: u64,
    rtt: Duration,
    last_update: Instant,

    base_rtt: Duration,
    target_delay: Duration,
}

impl ScreamCongestionControl {
    pub fn new() -> Self {
        Self {
            target_bitrate: 1_000_000, // start at 1 Mbps
            rtt: Duration::from_millis(100), 
            last_update: Instant::now(),
            base_rtt: Duration::from_secs(10),
            target_delay: Duration::from_millis(80),
        }
    }

    pub fn on_ack(&mut self, latest_rtt: Duration) {
        if latest_rtt.as_nanos() == 0 {
            return;
        }
        self.rtt = latest_rtt;

        self.base_rtt = min(self.base_rtt, self.rtt); 
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

    pub fn on_packet_loss(&mut self) {
        self.target_bitrate = (self.target_bitrate as f64 * 0.7) as u64;
        self.target_bitrate = self.target_bitrate.clamp(500_000, 10_000_000); // clamp between  500kbps and 10 Mbps
    }
    
    pub fn get_target_bitrate(&self) -> u64 {
        self.target_bitrate
    }

    pub fn get_cwnd(&self) -> u32 {
        // cwnd = (bitrate_in_bytes_per_sec * rtt_in_sec

        let bitrate_bps = self.get_target_bitrate();
        let rtt_sec = self.rtt.as_secs_f64();
        let bytes_per_sec = bitrate_bps / 8;
        
        let cwnd = (bytes_per_sec as f64 * rtt_sec) as u32;
        println!("[SCReAM] RTT: {:?}, Target Bitrate: {} kbps, Calculated CWND: {}", self.rtt, self.target_bitrate / 1000, cwnd);
        cwnd
    }
}