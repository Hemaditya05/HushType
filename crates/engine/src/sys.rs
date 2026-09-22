//! Hardware facts used to pick defaults and to guard model loading.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CpuInfo {
    pub logical_cores: usize,
    pub avx2: bool,
    pub avx512: bool,
    pub fma: bool,
}

pub fn cpu_info() -> CpuInfo {
    let logical_cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    #[cfg(target_arch = "x86_64")]
    let (avx2, avx512, fma) = (
        std::is_x86_feature_detected!("avx2"),
        std::is_x86_feature_detected!("avx512f"),
        std::is_x86_feature_detected!("fma"),
    );
    #[cfg(not(target_arch = "x86_64"))]
    let (avx2, avx512, fma) = (false, false, false);
    CpuInfo { logical_cores, avx2, avx512, fma }
}

/// Whether this build of the engine can run on the current CPU.
pub fn cpu_supported() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::is_x86_feature_detected!("avx2") && std::is_x86_feature_detected!("fma")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        true
    }
}

/// Inference threads: whisper.cpp scales with physical cores and
/// hyper-threads mostly add contention. Leaves headroom for the foreground app.
pub fn inference_threads() -> usize {
    if let Some(n) = std::env::var("HUSHTYPE_THREADS").ok().and_then(|s| s.parse::<usize>().ok()) {
        return n.clamp(1, 64);
    }
    let logical = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4);
    (logical / 2).clamp(1, 8)
}

/// GPU backends compiled into this build.
pub fn gpu_backends() -> Vec<&'static str> {
    let mut v = Vec::new();
    if cfg!(feature = "cuda") {
        v.push("CUDA");
    }
    if cfg!(feature = "vulkan") {
        v.push("Vulkan");
    }
    v
}

/// (total, available) physical memory in bytes.
#[cfg(windows)]
pub fn memory() -> Option<(u64, u64)> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    unsafe { GlobalMemoryStatusEx(&mut m).ok()? };
    Some((m.ullTotalPhys, m.ullAvailPhys))
}

#[cfg(not(windows))]
pub fn memory() -> Option<(u64, u64)> {
    None
}
