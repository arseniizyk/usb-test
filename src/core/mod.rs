use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BusType {
    Usb,
    Sata,
    Nvme,
    Scsi,
    Sd,
    Unknown,
}

impl std::fmt::Display for BusType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BusType::Usb => write!(f, "USB"),
            BusType::Sata => write!(f, "SATA"),
            BusType::Nvme => write!(f, "NVMe"),
            BusType::Scsi => write!(f, "SCSI"),
            BusType::Sd => write!(f, "SD/MMC"),
            BusType::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DriveInfo {
    pub letter: char,
    pub root_path: String,
    pub volume_name: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub sector_size: u32,
    pub is_removable: bool,
    pub is_system: bool,
    pub bus_type: BusType,
    pub model: String,
}

impl DriveInfo {
    pub fn formatted_total_size(&self) -> String {
        format_bytes(self.total_bytes)
    }

    pub fn formatted_free_size(&self) -> String {
        format_bytes(self.free_bytes)
    }
}

pub fn format_bytes(bytes: u64) -> String {
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    const TIB: u64 = 1024 * GIB;

    if bytes >= TIB {
        format!("{:.2} TB", bytes as f64 / TIB as f64)
    } else if bytes >= GIB {
        format!("{:.2} GB", bytes as f64 / GIB as f64)
    } else if bytes >= MIB {
        format!("{:.2} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.2} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockStatus {
    Untested,
    Healthy,  // < 50ms
    Warning,  // 50 - 200ms
    Slow,     // 200 - 500ms
    Critical, // > 500ms
    Bad,      // Read/Write failed or Checksum mismatch
}

impl BlockStatus {
    pub fn from_latency(latency_ms: u64) -> Self {
        if latency_ms < 50 {
            BlockStatus::Healthy
        } else if latency_ms < 200 {
            BlockStatus::Warning
        } else if latency_ms < 500 {
            BlockStatus::Slow
        } else {
            BlockStatus::Critical
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestMode {
    /// Полный тест записи тестового контейнера с последующей верификацией чтения
    WriteAndVerify,
    /// Только верификация существующего контейнера (безопасно для существующих файлов)
    ReadVerifyOnly,
    /// Быстрый тест на поддельную емкость (Fake Capacity Detection)
    FakeCapacityCheck,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestConfig {
    pub drive_letter: char,
    pub mode: TestMode,
    pub chunk_size: usize,
    pub target_bytes: u64,
    pub test_file_name: String,
    pub clean_up_after_test: bool,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            drive_letter: 'E',
            mode: TestMode::WriteAndVerify,
            chunk_size: 256 * 1024, // 256 KiB оптимально для флеш-накопителей
            target_bytes: 0,
            test_file_name: ".usb_test_data.tmp".to_string(),
            clean_up_after_test: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TestPhase {
    Idle,
    Writing,
    Reading,
    Verifying,
    Analyzing,
    Completed,
    Aborted,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TestStats {
    pub total_bytes: u64,
    pub total_blocks: usize,
    pub processed_blocks: usize,
    pub bytes_written: u64,
    pub bytes_read: u64,
    pub avg_write_speed_mbps: f64,
    pub avg_read_speed_mbps: f64,
    pub current_speed_mbps: f64,
    pub elapsed_secs: f64,
    pub healthy_count: u64,
    pub warning_count: u64,
    pub slow_count: u64,
    pub critical_count: u64,
    pub bad_count: u64,
    pub is_fake_capacity: bool,
    pub real_capacity_estimate: Option<u64>,
}

#[derive(Debug, Clone)]
pub enum TestProgressEvent {
    Started {
        phase: TestPhase,
        total_blocks: usize,
        total_bytes: u64,
    },
    PhaseChanged(TestPhase),
    BlockCompleted {
        block_index: usize,
        status: BlockStatus,
        latency_ms: u64,
        speed_mbps: f64,
    },
    SpeedUpdate {
        current_mbps: f64,
        avg_mbps: f64,
    },
    LogMessage(String),
    Finished(TestStats),
    Failed(String),
}
