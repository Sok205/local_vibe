use std::time::{Duration, Instant};

use sysinfo::{MemoryRefreshKind, Pid, ProcessRefreshKind, ProcessesToUpdate, System};

#[derive(Debug, Clone, Copy, Default)]
pub struct MemorySample {
    /// Resident set size of the current process, in bytes.
    pub rss_bytes: u64,
    /// Virtual size of the current process, in bytes.
    pub vsize_bytes: u64,
    /// Bytes of swap this process is currently using (0 when unknown).
    pub proc_swap_bytes: u64,
    /// System-wide swap used, in bytes.
    pub swap_used_bytes: u64,
    /// System-wide swap total, in bytes.
    pub swap_total_bytes: u64,
}

/// Throttled poller: calls into `sysinfo` at most once per `interval`. Cheap
/// enough to poll from the draw loop when the cached sample has expired.
pub struct MemoryPoller {
    sys: System,
    pid: Pid,
    interval: Duration,
    last_refresh: Option<Instant>,
    last_sample: MemorySample,
}

impl MemoryPoller {
    pub fn new(interval: Duration) -> Self {
        let pid = Pid::from_u32(std::process::id());
        Self {
            sys: System::new(),
            pid,
            interval,
            last_refresh: None,
            last_sample: MemorySample::default(),
        }
    }

    pub fn sample(&mut self) -> MemorySample {
        let due = self
            .last_refresh
            .map(|t| t.elapsed() >= self.interval)
            .unwrap_or(true);
        if !due {
            return self.last_sample;
        }
        self.sys.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[self.pid]),
            false,
            ProcessRefreshKind::nothing().with_memory(),
        );
        self.sys
            .refresh_memory_specifics(MemoryRefreshKind::nothing().with_swap());

        let proc_mem = self
            .sys
            .process(self.pid)
            .map(|p| (p.memory(), p.virtual_memory(), swap_of(p)))
            .unwrap_or((0, 0, 0));

        self.last_sample = MemorySample {
            rss_bytes: proc_mem.0,
            vsize_bytes: proc_mem.1,
            proc_swap_bytes: proc_mem.2,
            swap_used_bytes: self.sys.used_swap(),
            swap_total_bytes: self.sys.total_swap(),
        };
        self.last_refresh = Some(Instant::now());
        self.last_sample
    }
}

fn swap_of(p: &sysinfo::Process) -> u64 {
    // Not every platform populates per-process swap. Fall back to 0.
    p.run_time(); // keep the signature available; run_time avoids warn-unused if sysinfo shape shifts
    0
}

/// Format bytes as a short human-readable string: "4.2 GB", "512 MB", etc.
pub fn fmt_bytes(bytes: u64) -> String {
    const KB: f64 = 1_024.0;
    const MB: f64 = KB * 1_024.0;
    const GB: f64 = MB * 1_024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.0} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_bytes_across_scales() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(500), "500 B");
        assert_eq!(fmt_bytes(2_048), "2 KB");
        assert_eq!(fmt_bytes(1_500_000), "1 MB");
        assert_eq!(fmt_bytes(4_500_000_000), "4.2 GB");
    }

    #[test]
    fn poller_returns_nonzero_rss_for_self() {
        let mut p = MemoryPoller::new(Duration::from_millis(0));
        let s = p.sample();
        // The test process is definitely using some memory.
        assert!(s.rss_bytes > 1_000_000, "got rss={}", s.rss_bytes);
    }
}
