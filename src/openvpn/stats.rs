use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::time::Instant;

pub const MAX_HISTORY_POINTS: usize = 60;

#[derive(Debug, Clone)]
pub struct ThroughputMonitor {
    pub last_rx_bytes: u64,
    pub last_tx_bytes: u64,
    pub current_rx_rate: u64, // bytes / sec
    pub current_tx_rate: u64, // bytes / sec
    pub total_rx_bytes: u64,
    pub total_tx_bytes: u64,
    pub rx_history: VecDeque<u64>,
    pub tx_history: VecDeque<u64>,
    last_tick: Option<Instant>,
}

impl Default for ThroughputMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl ThroughputMonitor {
    pub fn new() -> Self {
        let mut rx_history = VecDeque::with_capacity(MAX_HISTORY_POINTS);
        let mut tx_history = VecDeque::with_capacity(MAX_HISTORY_POINTS);
        for _ in 0..MAX_HISTORY_POINTS {
            rx_history.push_back(0);
            tx_history.push_back(0);
        }

        Self {
            last_rx_bytes: 0,
            last_tx_bytes: 0,
            current_rx_rate: 0,
            current_tx_rate: 0,
            total_rx_bytes: 0,
            total_tx_bytes: 0,
            rx_history,
            tx_history,
            last_tick: None,
        }
    }

    pub fn reset(&mut self) {
        self.last_rx_bytes = 0;
        self.last_tx_bytes = 0;
        self.current_rx_rate = 0;
        self.current_tx_rate = 0;
        self.total_rx_bytes = 0;
        self.total_tx_bytes = 0;
        self.last_tick = None;
        self.rx_history.clear();
        self.tx_history.clear();
        for _ in 0..MAX_HISTORY_POINTS {
            self.rx_history.push_back(0);
            self.tx_history.push_back(0);
        }
    }

    /// Read raw statistics from Linux kernel for the given network interface (e.g. "tun0")
    pub fn read_interface_bytes(device: &str) -> Option<(u64, u64)> {
        if device.is_empty() {
            return None;
        }

        let base = Path::new("/sys/class/net").join(device).join("statistics");
        let rx_file = base.join("rx_bytes");
        let tx_file = base.join("tx_bytes");

        let rx = fs::read_to_string(rx_file)
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()?;
        let tx = fs::read_to_string(tx_file)
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()?;

        Some((rx, tx))
    }

    /// Update with newly observed raw total rx and tx bytes
    pub fn update(&mut self, current_rx: u64, current_tx: u64) {
        let now = Instant::now();

        if let Some(prev_time) = self.last_tick {
            let elapsed_secs = now.duration_since(prev_time).as_secs_f64();
            if elapsed_secs > 0.05 {
                let rx_diff = current_rx.saturating_sub(self.last_rx_bytes);
                let tx_diff = current_tx.saturating_sub(self.last_tx_bytes);

                self.current_rx_rate = (rx_diff as f64 / elapsed_secs) as u64;
                self.current_tx_rate = (tx_diff as f64 / elapsed_secs) as u64;

                self.rx_history.pop_front();
                self.rx_history.push_back(self.current_rx_rate);

                self.tx_history.pop_front();
                self.tx_history.push_back(self.current_tx_rate);
            }
        }

        self.last_rx_bytes = current_rx;
        self.last_tx_bytes = current_tx;
        self.total_rx_bytes = current_rx;
        self.total_tx_bytes = current_tx;
        self.last_tick = Some(now);
    }

    /// If no device exists (disconnected), push 0 rate to history
    pub fn tick_idle(&mut self) {
        self.current_rx_rate = 0;
        self.current_tx_rate = 0;
        self.rx_history.pop_front();
        self.rx_history.push_back(0);
        self.tx_history.pop_front();
        self.tx_history.push_back(0);
        self.last_tick = Some(Instant::now());
    }

    pub fn rx_history_slice(&self) -> Vec<u64> {
        self.rx_history.iter().copied().collect()
    }

    pub fn tx_history_slice(&self) -> Vec<u64> {
        self.tx_history.iter().copied().collect()
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes as f64;
    if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

pub fn format_rate(bytes_per_sec: u64) -> String {
    format!("{}/s", format_bytes(bytes_per_sec))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1536 * 1024 * 1024), "1.50 GB");
    }

    #[test]
    fn test_throughput_monitor_history() {
        let mut monitor = ThroughputMonitor::new();
        assert_eq!(monitor.rx_history.len(), MAX_HISTORY_POINTS);
        assert_eq!(monitor.tx_history.len(), MAX_HISTORY_POINTS);

        monitor.tick_idle();
        assert_eq!(monitor.rx_history.len(), MAX_HISTORY_POINTS);
        assert_eq!(monitor.current_rx_rate, 0);
    }
}
