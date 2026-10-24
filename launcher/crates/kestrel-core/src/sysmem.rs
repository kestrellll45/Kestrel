//! Physical memory detection and RAM presets.

/// Total physical memory in MiB, if it can be determined.
pub fn total_mib() -> Option<u64> {
    imp::total_bytes().map(|b| b / (1024 * 1024))
}

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    pub fn total_bytes() -> Option<u64> {
        // SAFETY: MEMORYSTATUSEX is plain data; zeroed is a valid initial
        // state and dwLength is set as the API requires. The pointer is to a
        // live stack value for the duration of the call.
        unsafe {
            let mut s: MEMORYSTATUSEX = std::mem::zeroed();
            s.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
            if GlobalMemoryStatusEx(&mut s) != 0 {
                Some(s.ullTotalPhys)
            } else {
                None
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn total_bytes() -> Option<u64> {
        let s = std::fs::read_to_string("/proc/meminfo").ok()?;
        let line = s.lines().find(|l| l.starts_with("MemTotal:"))?;
        let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb * 1024)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RamPreset {
    pub name: &'static str,
    pub max_mib: u64,
}

/// Presets appropriate for 1.8.9. More heap is not faster: 1.8.9 with a
/// PvP client rarely benefits from more than ~3-4 GiB and large heaps lengthen
/// GC pauses. Presets never exceed half of physical memory.
pub fn presets(total_mib: Option<u64>) -> Vec<RamPreset> {
    let cap = total_mib.map(|t| t / 2).unwrap_or(4096).max(1024);
    [
        ("Low (1 GB)", 1024),
        ("Balanced (2 GB)", 2048),
        ("High (3 GB)", 3072),
        ("Max useful (4 GB)", 4096),
    ]
    .into_iter()
    .filter(|(_, m)| *m <= cap)
    .map(|(name, max_mib)| RamPreset { name, max_mib })
    .collect()
}

/// Sensible default heap: 2 GiB when there is ≥ 6 GiB RAM, else 1 GiB.
pub fn default_max_mib(total_mib: Option<u64>) -> u64 {
    match total_mib {
        Some(t) if t >= 6 * 1024 => 2048,
        Some(_) => 1024,
        None => 2048,
    }
}

/// Clamp a user-chosen heap to [512 MiB, min(16 GiB, 75% of RAM)].
pub fn clamp_max_mib(requested: u64, total_mib: Option<u64>) -> u64 {
    let upper = total_mib
        .map(|t| t * 3 / 4)
        .unwrap_or(16 * 1024)
        .clamp(512, 16 * 1024);
    requested.clamp(512, upper)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_respect_memory() {
        assert_eq!(presets(Some(4096)).len(), 2);
        assert_eq!(presets(Some(32768)).len(), 4);
        assert_eq!(presets(Some(1024)).len(), 1);
        assert_eq!(default_max_mib(Some(16384)), 2048);
        assert_eq!(clamp_max_mib(100_000, Some(8192)), 6144);
        assert_eq!(clamp_max_mib(1, Some(8192)), 512);
    }

    #[test]
    fn detects_memory_on_this_host() {
        assert!(total_mib().unwrap_or(1) > 0);
    }
}
