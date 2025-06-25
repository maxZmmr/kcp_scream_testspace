use std::{cmp::min, collections::HashMap, time::{Duration, Instant}};
use std::fs::OpenOptions;
use std::io::Write;
use std::time::SystemTime;


const BASE_RTT_WINDOW: Duration = Duration::from_secs(10);
const QDELAY_TARGET_LO: f32 = 0.06; // 0.1 seconds
const QDELAY_TARGET_HI: f32 = 0.4;
const MIN_REF_WND: u32 = 3000;
const BYTES_IN_FLIGHT_HEAD_ROOM: f32 = 2.0;
const BETA_LOSS: f32 = 0.7;
const BETA_ECN: f32 = 0.8;
const MSS: u64 = 1000;
const REF_WND_OVERHEAD: f32 = 1.5;
const POST_CONGESTION_DELAY_RTT: u32 = 100;
const MUL_INCREASE_FACTOR: f32 = 0.02;
const IS_L4S: bool = false;
const VIRTUAL_RTT: f32 = 0.025;
const PACKET_PACING_HEADROOM: f32 = 1.5;



#[derive(Debug)]
struct PacketInfo {
    timestamp: Instant,
    size: usize,
}

#[derive(Debug)]
pub struct ScreamCongestionControl {
    rtt: Duration,
    base_rtt: Duration,
    min_rtt_in_window: Duration,
    base_rtt_update_time: Instant,
    
    last_rtt_update_time: Instant,
    mult_bitrate_reduction: f64, 
    mult_bitrate_increase: f64,
    r_target: u64, //estimated bandwidth in bps
    
    last_update: Instant,
    packets_in_flight: HashMap<u32, PacketInfo>,
    //bytes_in_flight: u64,
    //qdelay_target: Duration,
    //target_bitrate: u64,

    last_periodic_update_time: Instant,
    loss_occured_in_rtt: bool,
    loss_occured_for_csv_file: bool,
    first_rtt_measurement: bool,
    rtt_var: f32,                           // rtt variation (rfc 6298) [s]

    // variables for ref_wnd
    ref_wnd: f32 ,                          // The reference congestion window in bytes [byte].
    bytes_in_flight: u32,                    // Number of unacknowledged bytes in the network [byte].
    max_bytes_in_flight: u32,                 // Maximum observed bytes in flight in the current RTT [byte].
    max_bytes_in_flight_prev: u32,              // The maximum number of bytes in flight in previous round trip [byte]
    s_rtt: f32,                               // Smoothed round-trip time in seconds. [s]
    qdelay: Duration,                       
    qdelay_avg: f32,                          // Filtered, averaged queue delay in seconds. [s]
    qdelay_target: f32,                       // Target delay for the queue in seconds. [s]
    last_congestion_detected_time: Instant,       // Timestamp of the last overload event. [s]
    last_ref_wnd_i_update_time : Instant,       // Timestamp of the last time ref_wnd_i was updated [s]
    l4s_alpha: f32,                           // Scaling factor for L4S ECN feedback.
    is_l4s_active: bool,                       // Flag indicating whether L4S is actively used.
    ref_wnd_i: f32,                            // Inflection point of the CWND, value of the CWND at the last overload event.
    ref_wnd_ratio: f32,                       // Ratio between MSS and ref_wnd capped to not exceed 1.0 (min(1.0, MSS / ref_wnd))
    bytes_newly_acked: u32,                    // Number of bytes newly ACKed, reset to 0 when congestion window is updated [byte]
    bytes_newly_acked_ce: u32,                  // Number of bytes newly ACKed and CE marked, reset to 0 when reference window is updated [byte]
}

impl ScreamCongestionControl {
    pub fn new() -> Self {
        Self {
            rtt: Duration::from_millis(100), 
            base_rtt: Duration::from_secs(10),
            min_rtt_in_window: Duration::from_secs(10),
            base_rtt_update_time: Instant::now(),

            last_rtt_update_time: Instant::now(),
            mult_bitrate_reduction: 0.9,  
            mult_bitrate_increase: 1.05,  
            r_target: 1_000_000, 
            
            last_update: Instant::now(),
            packets_in_flight: HashMap::new(),

            last_periodic_update_time: Instant::now(),
            loss_occured_in_rtt: false,
            loss_occured_for_csv_file: false,
            first_rtt_measurement: true,
            rtt_var: 0.0,
            
            ref_wnd: 4.0 * MSS as f32,
            last_ref_wnd_i_update_time: Instant::now(),
            bytes_in_flight: 0,                    
            max_bytes_in_flight: 0,  
            max_bytes_in_flight_prev: 0,               
            s_rtt: 0.0,       
            qdelay: Duration::from_secs(1),                        
            qdelay_avg: 0.0,                          
            qdelay_target: QDELAY_TARGET_LO,                       
            last_congestion_detected_time: Instant::now(),
            l4s_alpha: 0.1,                                      
            is_l4s_active: false,                      
            ref_wnd_i: 0.0,  
            ref_wnd_ratio: 0.0,   
            bytes_newly_acked: 0,
            bytes_newly_acked_ce: 0,                      
        }
    }


    fn decrease_window(&mut self, now: Instant, is_loss: bool, is_ce: bool) {

        // ref window reduction
        let mut is_loss_t = false;
        let mut is_ce_t = false;
        let mut is_virtual_ce_t = false;
        let mut l4s_alpha_v_t = 0.0;
        if self.last_congestion_detected_time.elapsed().as_secs_f32() >= VIRTUAL_RTT.min(self.s_rtt) {
            if is_loss { is_loss_t = true };
            if is_ce { is_ce_t = true };


            if self.qdelay > Duration::from_secs_f32(self.qdelay_target / 2.0) {
                // It is expected that l4s_alpha is below a given value,
                let l4_alpha_lim_t = 2.0 / self.get_target_bitrate() * MSS as f32 * 8.0 / self.s_rtt;
                if self.l4s_alpha < l4_alpha_lim_t || !self.is_l4s_active {
                    // L4S does not seem to be active
                    l4s_alpha_v_t = f32::min(1.0, f32::max(0.0,
                            (self.qdelay_avg - self.qdelay_target / 2.0) / (self.qdelay_target / 2.0)));
                    is_virtual_ce_t = true;
                }
            }    
        }
        
        if is_loss_t || is_ce_t || is_virtual_ce_t {
            if self.last_ref_wnd_i_update_time.elapsed() > Duration::from_secs_f32(10.0 * self.s_rtt) {
                // Update ref_wnd_i, no more often than every 10 RTTs
                // Additional median filtering over more congestion epochs
                // may improve accuracy of ref_wnd_i
                self.last_ref_wnd_i_update_time = now; 
                self.ref_wnd_i = self.ref_wnd;
            }
        }

        //     Either loss, ECN mark or increased qdelay is detected
        if is_loss_t {
            // loss is detected
            self.ref_wnd = self.ref_wnd * BETA_LOSS;
        }

        if is_ce_t {
            if IS_L4S {
                // L4S mode
                let mut backoff_t = self.l4s_alpha / 2.0;

                // Increase stability for very small ref_wnd
                backoff_t *= f32::max(0.5, 1.0 - self.ref_wnd_ratio);

                if self.last_congestion_detected_time.elapsed() > Duration::from_secs_f32(100.0 * f32::max(VIRTUAL_RTT, self.s_rtt)) {
                    // A long time (>100 RTTs) since last congested because
                    // link throughput exceeds max video bitrate.
                    // There is a certain risk that ref_wnd has increased way above
                    // bytes in flight, so we reduce it here to get it better on
                    // track and thus the congestion episode is shortened
                    self.ref_wnd = f32::min(self.ref_wnd, self.max_bytes_in_flight_prev as f32);

                    // Also, we back off a little extra if needed
                    // because alpha is quite likely very low
                    // This can in some cases be an over-reaction
                    // but as this function should kick in relatively seldom
                    // it should not be to too big concern
                    backoff_t = f32::max(backoff_t, 0.25);

                    // In addition, bump up l4sAlpha to a more credible value
                    // This may over react but it is better than
                    // excessive queue delay
                    self.l4s_alpha = 0.25;
                }   
                self.ref_wnd = (1.0 - backoff_t) * self.ref_wnd;
            } else {
                // Classic ECN mode
                self.ref_wnd = self.ref_wnd * BETA_ECN;
            }
        }

        if is_virtual_ce_t {
            let backoff_t = l4s_alpha_v_t  / 2.0;
            self.ref_wnd = (1.0 - backoff_t) * self.ref_wnd;
        }
        self.ref_wnd = f32::max(MIN_REF_WND as f32, self.ref_wnd);

        if is_loss_t || is_ce_t || is_virtual_ce_t {
            self.last_congestion_detected_time = now;
        }
    }

    fn increase_window(&mut self) {
        // Delay factor for multiplicative reference window increase
        // after congestion

        let post_congestion_scale_t = f32::max(0.0, f32::min(1.0,
        (self.last_congestion_detected_time.elapsed().as_secs_f32()) /
        (POST_CONGESTION_DELAY_RTT as f32 * f32::max(VIRTUAL_RTT, self.s_rtt))));

        // Scale factor for ref_wnd update
        let ref_wnd_scale_factor_t = 1.0 + (MUL_INCREASE_FACTOR  * self.ref_wnd) / MSS as f32;

        // Calculate bytes acked that are not CE marked
        // For the case that only accumulated number of CE marked packets is
        // reported by the feedback, it is necessary to make an approximation
        // of bytes_newly_acked_ce based on average data unit size.
        let bytes_newly_acked_minus_ce_t = self.bytes_newly_acked - self.bytes_newly_acked_ce;

        let mut increment_t = bytes_newly_acked_minus_ce_t as f32 * self.ref_wnd_ratio;
        
        // Reduce increment for small RTTs
        let mut tmp_t = f32::min(1.0, self.s_rtt / VIRTUAL_RTT);
        increment_t *= tmp_t * tmp_t;

        // Apply limit to reference window growth when close to last
        // known max value before congestion
        let mut scl_t = (self.ref_wnd - self.ref_wnd_i) / self.ref_wnd_i;
        scl_t *= 4.0;
        scl_t = scl_t * scl_t;
        scl_t = f32::max(0.1, f32::min(1.0, scl_t));
        if !self.is_l4s_active {
            increment_t *= scl_t;
        }


        // Limit on CWND growth speed further for small CWND
        // This is complemented with a corresponding restriction on CWND
        // reduction
        increment_t *= f32::max(0.5,1.0 - self.ref_wnd_ratio);

        // Scale up increment with multiplicative increase
        // Limit multiplicative increase when congestion occured
        // recently and when reference window is close to the last
        // known max value
        tmp_t = ref_wnd_scale_factor_t;
        if tmp_t > 1.0 {
            tmp_t = 1.0 + (tmp_t - 1.0) * post_congestion_scale_t * scl_t;
        }
        increment_t *= tmp_t;

        // Increase ref_wnd only if bytes in flight is large enough
        // Quite a lot of slack is allowed here to avoid that bitrate
        // locks to low values.
        let max_allowed_t = MSS + u64::max(self.max_bytes_in_flight as u64,
        self.max_bytes_in_flight_prev as u64) * BYTES_IN_FLIGHT_HEAD_ROOM as u64;
        let ref_wnd_t = self.ref_wnd + increment_t;
        if ref_wnd_t <= max_allowed_t as f32 {
            self.ref_wnd = ref_wnd_t;
        }
    }

    pub fn update_ref_window(&mut self, is_loss: bool, is_ce: bool) {
        let now = Instant::now();
        self.increase_window();
        self.decrease_window(now, is_loss, is_ce);
    }

    pub fn on_packet_sent(&mut self, seq_number: u32, size: usize) {
        let info = PacketInfo{ timestamp: Instant::now(), size };
        self.bytes_in_flight += size as u32;
        self.packets_in_flight.insert(seq_number, info);
        self.max_bytes_in_flight = self.max_bytes_in_flight.max(self.bytes_in_flight);
    }

    pub fn on_rtt(&mut self) {
        self.max_bytes_in_flight_prev = self.max_bytes_in_flight;
        self.max_bytes_in_flight = self.bytes_in_flight; 

        let is_ce = false; // just for testing 
        self.update_ref_window(self.loss_occured_in_rtt, is_ce);
        
        self.bytes_newly_acked = 0;
        self.bytes_newly_acked_ce = 0;
        self.loss_occured_in_rtt = false;
        self.last_periodic_update_time = Instant::now();
    }

    // everytime a packet gets ACK'ed the congestion_window gets increased and decreased (only after one s_rtt)
    pub fn on_ack(&mut self, seq_number: u32, size: usize) {
        if let Some(info) = self.packets_in_flight.remove(&seq_number) {

            // remove from bytes in flight
            self.bytes_in_flight -= size as u32;

            self.bytes_newly_acked += size as u32;
            
            let latest_rtt = info.timestamp.elapsed();
            if latest_rtt.as_nanos() == 0 { return; }
            self.rtt = latest_rtt;

            if self.first_rtt_measurement {
                self.s_rtt = latest_rtt.as_secs_f32();
                self.rtt_var = (latest_rtt / 2).as_secs_f32();
                self.qdelay_avg = self.qdelay.as_secs_f32();
            } else {
                let alpha = 0.125;
                let beta = 0.25;
                let qdelay_avg_g = 0.25;
                self.rtt_var = (1.0 - beta) * self.rtt_var + beta * (self.s_rtt - latest_rtt.as_secs_f32()).abs();
                self.s_rtt = (1.0 - alpha) * self.s_rtt + alpha * latest_rtt.as_secs_f32();
                self.qdelay_avg = (1.0 - qdelay_avg_g) * self.qdelay_avg + qdelay_avg_g * self.qdelay.as_secs_f32();
            }

            // update base_rtt every 10 seconds
            if self.base_rtt_update_time.elapsed() >= BASE_RTT_WINDOW {
                self.base_rtt_update_time = Instant::now();
                self.base_rtt = self.min_rtt_in_window;       
            }
            self.min_rtt_in_window = min(self.min_rtt_in_window, latest_rtt);
        }
    }

    pub fn on_packet_loss(&mut self, seq_number: u32) {
        // remove bytes in flight
        if let Some(info) = self.packets_in_flight.remove(&seq_number) {
            self.bytes_in_flight = self.bytes_in_flight.saturating_sub(info.size as u32);  
        } else {
            print!("MAYDAY MAYDAY, lost packet not removed from bytes in flight: {}", self.bytes_in_flight);
        }
    }
    

    pub fn log_data(&mut self) {
        // output to csv file
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let packet_loss_indicator = if self.loss_occured_for_csv_file { 1 } else { 0 };
        let log_line = format!(
            "{},{},{},{}, {}\n",
            timestamp,
            self.rtt.as_millis(),
            self.get_target_bitrate() / 1000.0,
            self.ref_wnd,
            packet_loss_indicator,
        );
        self.loss_occured_for_csv_file = false;

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
    }


    pub fn get_target_bitrate(&self) -> f32 {
        ((self.ref_wnd * 8.0) / self.s_rtt).clamp(500_000.0, 10_000_000.0)
    }  

    pub fn get_pacing_rate(&self) -> f32 {
        let target_bitrate = self.get_target_bitrate();
        target_bitrate * PACKET_PACING_HEADROOM
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