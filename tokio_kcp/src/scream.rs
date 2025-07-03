use std::{cmp::min, collections::HashMap, convert::TryInto, time::{Duration, Instant, UNIX_EPOCH}};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::SystemTime;

// scream feedback header to seperate KCP and SCReAMv2 ACK's
pub const SCREAM_FEEDBACK_HEADER: u32 = 0x5C4D4642; // "SCMFB" in hex

const BASE_RTT_WINDOW: Duration = Duration::from_secs(10);
const QDELAY_TARGET_LO: f32 = 0.06; 
const MIN_REF_WND: u32 = 2000;     
const BYTES_IN_FLIGHT_HEAD_ROOM: f32 = 1.5;
const BETA_LOSS: f32 = 0.7;
const BETA_ECN: f32 = 0.8;
const MSS: u64 = 1000;
const POST_CONGESTION_DELAY_RTT: f32 = 4.0;
const MUL_INCREASE_FACTOR: f32 = 0.02;
const PACKET_PACING_HEADROOM: f32 = 1.25;

#[derive(Debug, Clone, Copy)]
pub struct FeedbackPacketInfo {
    pub seq_number: u32,
    pub reception_time_ms: u64,
}

#[derive(Debug)]
struct PacketInfo {
    timestamp: Instant,
    size: usize,
    acked_by_kcp: bool,
}

#[derive(Debug)]
pub struct ScreamCongestionControl {
    s_rtt: f32,
    rtt_var: f32,
    base_rtt: Duration,
    min_rtt_in_window: Duration,
    base_rtt_update_time: Instant,
    qdelay: Duration,
    qdelay_avg: f32,
    qdelay_target: f32,

    // ref_wnd and bytes in flight
    ref_wnd: f32,
    ref_wnd_i: f32, 
    bytes_in_flight: u32,
    max_bytes_in_flight: u32,
    max_bytes_in_flight_prev: u32,
    
    // variables for window control
    bytes_newly_acked: u32,
    bytes_newly_acked_ce: u32, // Für ECN
    loss_occured_in_rtt: bool,
    last_congestion_detected_time: Instant,
    last_ref_wnd_i_update_time: Instant,
    last_periodic_update_time: Instant,
    
    // packet-tracking
    packets_in_flight: HashMap<u32, PacketInfo>,

    // logging and small helpers
    first_rtt_measurement: bool,
    loss_for_log: bool,

    // for packet feedback
    received_packets_for_feedback: Vec<FeedbackPacketInfo>,
    last_feedback_time: Instant,
}

impl ScreamCongestionControl {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            s_rtt: 0.0,
            rtt_var: 0.0,
            base_rtt: Duration::from_secs(10), 
            min_rtt_in_window: Duration::from_secs(10),
            base_rtt_update_time: now,
            qdelay: Duration::ZERO,
            qdelay_avg: 0.0,
            qdelay_target: QDELAY_TARGET_LO,

            ref_wnd: 2.0 * MSS as f32, 
            ref_wnd_i: 2.0 * MSS as f32,
            bytes_in_flight: 0,
            max_bytes_in_flight: 0,
            max_bytes_in_flight_prev: 0,

            bytes_newly_acked: 0,
            bytes_newly_acked_ce: 0,
            loss_occured_in_rtt: false,
            last_congestion_detected_time: now,
            last_ref_wnd_i_update_time: now,
            last_periodic_update_time: now,

            packets_in_flight: HashMap::new(),

            first_rtt_measurement: true,
            loss_for_log: false,   
            
            received_packets_for_feedback: Vec::new(),
            last_feedback_time: Instant::now(),             
        }
    }


    fn decrease_window(&mut self, now: Instant, is_loss: bool, is_ce: bool) {
        let mut congestion_event = false;
        let mut reduction_factor: f32 = 1.0;

        if self.qdelay_avg > self.qdelay_target / 2.0 {
            // only reduce every 1 RTT -> prevent overreaction
            if now.saturating_duration_since(self.last_congestion_detected_time).as_secs_f32() > self.s_rtt {
                let backoff = (self.qdelay_avg - self.qdelay_target / 2.0) / (self.qdelay_target / 2.0);
                reduction_factor = reduction_factor.min(1.0 - backoff.clamp(0.0, 1.0) * 0.5);
                congestion_event = true;
            }
        }

        // loss or ECN based reduction
        if is_loss {
            reduction_factor = reduction_factor.min(BETA_LOSS);
            congestion_event = true;
        } else if is_ce {
            reduction_factor = reduction_factor.min(BETA_ECN);
            congestion_event = true;
        }

        if congestion_event {
            // renew ref_wnd when enough time has passed since last renewal (typically 10 rtt's)
            if now.saturating_duration_since(self.last_ref_wnd_i_update_time).as_secs_f32() > 10.0 * self.s_rtt {
                self.ref_wnd_i = self.ref_wnd;
                self.last_ref_wnd_i_update_time = now;
            }

            self.ref_wnd *= reduction_factor;
            self.ref_wnd = self.ref_wnd.max(MIN_REF_WND as f32);
            self.last_congestion_detected_time = now;
        }
    }

    fn increase_window(&mut self) {
        if self.bytes_newly_acked == 0 {
            return;
        }


        // scaling factor -> throttle up slowly after congestion event
        let post_congestion_scale = (self.last_congestion_detected_time.elapsed().as_secs_f32()
            / (POST_CONGESTION_DELAY_RTT * self.s_rtt.max(0.01))).clamp(0.0, 1.0);
        
        let additive_increase = self.bytes_newly_acked as f32 * (MSS as f32 / self.ref_wnd.max(MSS as f32));
        let multiplicative_increase = self.ref_wnd * MUL_INCREASE_FACTOR * (self.bytes_newly_acked as f32 / self.ref_wnd.max(1.0));

        let mut increment = additive_increase + multiplicative_increase * post_congestion_scale;

        // smaller increase when ref_wnd is near the red_wnd_i (last inflection point)
        if self.ref_wnd > self.ref_wnd_i {
            let scale = ((self.ref_wnd - self.ref_wnd_i) / self.ref_wnd_i).clamp(0.0, 4.0);
            increment *= (1.0 - (scale / 4.0).powi(2)).max(0.25);
        }

        // apply the increment
        let max_allowed_wnd = (self.max_bytes_in_flight_prev as f32 * BYTES_IN_FLIGHT_HEAD_ROOM).max(self.ref_wnd);
        if self.ref_wnd + increment <= max_allowed_wnd  {
            self.ref_wnd += increment;
        } else {
            self.ref_wnd = max_allowed_wnd;
        }
    }


    pub fn on_packet_sent(&mut self, seq_number: u32, size: usize) {
        let now = Instant::now();
        let info = PacketInfo{ timestamp: now, size: size, acked_by_kcp: false };
        self.packets_in_flight.insert(seq_number, info);
        self.bytes_in_flight += size as u32;
        self.max_bytes_in_flight = self.max_bytes_in_flight.max(self.bytes_in_flight);
    }

     pub fn on_packet_received(&mut self, seq_number: u32, reception_time: Instant) {
        let reception_time_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        self.received_packets_for_feedback.push(FeedbackPacketInfo { 
            seq_number: seq_number,
            reception_time_ms: reception_time_ms,
        });
    }

    pub fn create_feedback_packet(&mut self) -> Option<Vec<u8>>  {
        if self.received_packets_for_feedback.is_empty() {
            return None;
        }

        // 12 bytes per entry -> 4 for sn and 8 for timestamp
        let mut feedback_data = Vec::with_capacity(self.received_packets_for_feedback.len() * 12);

        for info in &self.received_packets_for_feedback {
            feedback_data.extend_from_slice((&info.seq_number.to_le_bytes()));
            feedback_data.extend_from_slice(&info.reception_time_ms.to_le_bytes());
        }

        self.received_packets_for_feedback.clear();
        self.last_feedback_time = Instant::now();
        Some(feedback_data)
    }

    // when an SCReAMv2 feedback header packet is delivered
    pub fn on_feedback(&mut self, data: &[u8], feedback_arrival_time: Instant) {
        for chunk in data.chunks_exact(12) {
            let seq_number = u32::from_le_bytes(chunk[0..4].try_into().unwrap());
            //let _reception_time_ms = u64::from_le_bytes(chunk[4..12].try_into().unwrap());
            self.on_ack_scream(seq_number, feedback_arrival_time);
        }
    }

    pub fn on_rtt(&mut self) {
        self.increase_window();
        self.decrease_window(Instant::now(), false, false);

        self.max_bytes_in_flight_prev = self.max_bytes_in_flight;
        self.max_bytes_in_flight = self.bytes_in_flight; 
        
        // reset values
        self.bytes_newly_acked = 0;
        self.bytes_newly_acked_ce = 0;
        self.loss_occured_in_rtt = false;
        self.last_periodic_update_time = Instant::now();
    }

    // gets called everytime there is an KCP ACK 
    pub fn on_ack_kcp(&mut self, seq_number: u32) {
        if let Some(info) = self.packets_in_flight.get_mut(&seq_number) {
            if !info.acked_by_kcp {
                self.bytes_in_flight = self.bytes_in_flight.saturating_sub(info.size as u32);
                info.acked_by_kcp = true;
            }
        }
    }

    // everytime a SCReAMv2 feedback packet arrives
    pub fn on_ack_scream(&mut self, seq_number: u32, ack_timestamp: Instant) {
        if let Some(info) = self.packets_in_flight.remove(&seq_number) {
            if !info.acked_by_kcp {
                // remove from bytes in flight
                self.bytes_in_flight = self.bytes_in_flight.saturating_sub(info.size as u32);
            }

            // add ACK'ed bytes to the list for this rtt
            self.bytes_newly_acked += info.size as u32;

            let latest_rtt = ack_timestamp.saturating_duration_since(info.timestamp);
            if latest_rtt.is_zero() { return; }
            

            if self.first_rtt_measurement {
                self.s_rtt = latest_rtt.as_secs_f32();
                self.base_rtt = latest_rtt; // set first base_rtt for first 10 seconds
                self.rtt_var = self.s_rtt / 2.0;
                self.first_rtt_measurement = false;
            } else {
                let alpha = 0.125;
                let beta = 0.25;
                let rtt_now_secs = latest_rtt.as_secs_f32();
                self.rtt_var = (1.0 - beta) * self.rtt_var + beta * (self.s_rtt - rtt_now_secs).abs();
                self.s_rtt = (1.0 - alpha) * self.s_rtt + alpha * rtt_now_secs;

                println!("{}", s_rtt)
            }

            // update base_rtt every 10 seconds
            self.min_rtt_in_window = min(self.min_rtt_in_window, latest_rtt);
            if self.base_rtt_update_time.elapsed() >= BASE_RTT_WINDOW {
                self.base_rtt = self.min_rtt_in_window;       
                self.min_rtt_in_window = Duration::from_secs(10);
                self.base_rtt_update_time = Instant::now();
            }
            self.qdelay = latest_rtt.saturating_sub(self.base_rtt);
            let qdelay_sample = self.qdelay.as_secs_f32();
            let q_alpha = 0.1;
            self.qdelay_avg = (1.0 - q_alpha) * self.qdelay_avg + q_alpha * qdelay_sample;
        }
    }

    pub fn on_packet_loss(&mut self, seq_number: u32) {
        // remove bytes in flight
        if let Some(info) = self.packets_in_flight.remove(&seq_number) {
            self.bytes_in_flight = self.bytes_in_flight.saturating_sub(info.size as u32);
            self.loss_occured_in_rtt = true;
            self.loss_for_log = true; 
            self.decrease_window(Instant::now(), true, false);
        } else {
            print!("MAYDAY MAYDAY, lost packet not removed from bytes in flight: {}", self.bytes_in_flight);
        }
    }
    

    pub fn log_data(&mut self) {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let packet_loss_indicator = if self.loss_for_log { 1 } else { 0 };
        self.loss_for_log = false;

        let log_line = format!(
            "{},{},{},{},{},{},{},{},{},{}\n",
            timestamp,
            (self.s_rtt * 1000.0) as u128, 
            (self.base_rtt.as_secs_f32() * 1000.0) as u128, 
            (self.qdelay.as_secs_f32() * 1000.0) as u128, 
            (self.qdelay_avg * 1000.0) as u128,
            self.get_target_bitrate() / 1000.0, 
            self.ref_wnd, 
            self.bytes_in_flight,
            self.max_bytes_in_flight,
            packet_loss_indicator,
        );

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("scream_log.csv")
        {
            if file.metadata().unwrap().len() == 0 {
                let _ = file.write_all(b"timestamp_ms,s_rtt_ms,base_rtt_ms,qdelay_ms,qdelay_avg_ms,bitrate_kbps,cwnd_bytes,bytes_in_flight,max_bytes_in_flight,packet_loss\n");
            }
            let _ = file.write_all(log_line.as_bytes());
        }
    }


    pub fn get_last_feedback_time(&self) -> Instant {
        return self.last_feedback_time
    }

    pub fn get_target_bitrate(&self) -> f32 {
        if self.s_rtt <= 0.0 { return 500_000.0; }
        (self.ref_wnd * 8.0 / self.s_rtt).clamp(500_000.0, 10_000_000.0)
    }  

    pub fn get_pacing_rate(&self) -> f32 {
        self.get_target_bitrate() * PACKET_PACING_HEADROOM
    } 

    pub fn get_ref_wnd(&self) -> f32 {
        self.ref_wnd
    }

    pub fn get_last_periodic_update_time(&self) -> Instant {
        self.last_periodic_update_time
    }

    pub fn get_s_rtt(&self) -> f32 {
        self.s_rtt
    }
}