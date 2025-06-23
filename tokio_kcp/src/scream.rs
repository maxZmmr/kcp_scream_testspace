use std::time::{Duration, Instant};

pub struct ScreamCongestionControl {
    target_bitrate: u64,
    rtt: Duration,
    last_update: Instant,
}

impl ScreamCongestionControl {
    pub fn new() -> Self {
        Self {
            target_bitrate: 1_000_000, // 1 Mbps
            rtt: Duration::from_millis(100), 
            last_update: Instant::now(),
        }
    }

    pub fn on_ack(&mut self, rtt: Duration) {
        // Hier kommt die Kernlogik von SCReAM hin.
        // Vereinfachtes Beispiel:
        // - Aktualisiere die RTT-Schätzung.
        // - Wenn die Latenz niedrig ist, erhöhe die Rate.
        // - Wenn Paketverlust erkannt wird (in einer anderen Funktion), verringere die Rate.
        
        self.rtt = rtt; // Hier wäre ein geglätteter Durchschnitt besser (wie in TCP).

        // Beispielhafte Logik: Wenn die Latenz unter einem Schwellenwert liegt,
        // erhöhe die Bitrate leicht.
        if self.rtt < Duration::from_millis(150) {
            self.target_bitrate += 20_000; // Erhöhe um 20 kbps
        }
    }

    /// Diese Funktion wird aufgerufen, wenn ein Paketverlust erkannt wird.
    pub fn on_packet_loss(&mut self) {
        // Reduziere die Bitrate als Reaktion auf den Verlust.
        // SCReAM hat hierfür spezifische Formeln. Ein gängiger Ansatz ist
        // eine multiplikative Verringerung.
        self.target_bitrate = (self.target_bitrate as f64 * 0.8) as u64;
    }
    
    /// Gibt die aktuelle Ziel-Bitrate zurück.
    pub fn get_target_bitrate(&self) -> u64 {
        self.target_bitrate
    }

    /// Gibt die erlaubte Fenstergröße (cwnd) basierend auf der Bitrate zurück.
    pub fn get_cwnd(&self) -> u32 {
        // Formel: cwnd = (bitrate_in_bytes_per_sec * rtt_in_sec)
        // Dies ist die Bandwidth-Delay Product (BDP) Berechnung.
        let bitrate_bps = self.get_target_bitrate();
        let rtt_sec = self.rtt.as_secs_f64();
        let bytes_per_sec = bitrate_bps / 8;
        
        (bytes_per_sec as f64 * rtt_sec) as u32
    }
}