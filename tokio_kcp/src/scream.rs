
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


impl ScreamState {

    fn on_ack(self, congestion_control: &mut ScreamCongestionControl, latest_rtt: Duration) -> ScreamState {

        let queuing_delay = congestion_control.rtt.saturating_sub(congestion_control.base_rtt);

        match self {
            ScreamState::Normal => {
                if queuing_delay > QDELAY_TARGET_HI {
                    return ScreamState::Reduce;
                }
                // TODO: increase every 50_000 every 100ms -> not when on_ack gets called


                congestion_control.target_bitrate += 20_000;
            },
            ScreamState::Reduce => {
                if congestion_control.last_congestion_event_time.elapsed() < Duration::from_secs(5) {
                    congestion_control.target_bitrate = (congestion_control.target_bitrate as f64 * 0.95) as u64;
                } else if queuing_delay < QDELAY_TARGET_LO {
                    return ScreamState::Normal
                }
            }
            ScreamState::Accelerate => {
                // return self
            },
        }
        self
    }

    fn on_packet_loss(self, congestion_control: &mut ScreamCongestionControl) -> ScreamState {
        congestion_control.target_bitrate = (congestion_control.target_bitrate as f64 * 0.7) as u64;
        congestion_control.last_congestion_event_time = Instant::now();
        ScreamState::Reduce
    }

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
            base_rtt_update_time: Instant::now(),

            qdelay_target: QDELAY_TARGET_LO,
            state: ScreamState::Normal,
            last_rtt_update_time: Instant::now(),
            mult_bitrate_reduction: 0.9,  
            mult_bitrate_increase: 1.05,  
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
            
            let latest_rtt = info.timestamp.elapsed();
            if latest_rtt.as_nanos() == 0 { return; }
            self.rtt = latest_rtt;

            // update base_rtt every 10 seconds
            if self.base_rtt_update_time.elapsed() >= BASE_RTT_WINDOW {
                self.base_rtt_update_time = Instant::now();
                self.base_rtt = self.min_rtt_in_window;       
            }
            self.min_rtt_in_window = min(self.min_rtt_in_window, latest_rtt);
            let queuing_delay = self.rtt.saturating_sub(self.base_rtt);

            self.state = self.state.on_ack(self, latest_rtt);
            self.target_bitrate = self.target_bitrate.clamp(500_000, 10_000_000);
        }
    }

    pub fn on_packet_loss(&mut self) {
        self.state = self.state.on_packet_loss(self);
        self.target_bitrate = self.target_bitrate.clamp(500_000, 10_000_000);
    }
    


    pub fn get_cwnd(&mut self) -> u32 {
        let bitrate_bps = self.target_bitrate;
        let rtt_sec = self.rtt.as_secs_f64();
        let bytes_per_sec = bitrate_bps / 8;

        let cwnd = (bytes_per_sec as f64 * rtt_sec) as u32;


        // output to csv file
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