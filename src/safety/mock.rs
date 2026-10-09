use super::DriveScanner;
use crate::core::{BusType, DriveInfo};
use crate::error::Result;

#[derive(Default)]
pub struct MockDriveScanner;

impl MockDriveScanner {
    pub fn new() -> Self {
        Self
    }
}

impl DriveScanner for MockDriveScanner {
    fn scan_drives(&self) -> Result<Vec<DriveInfo>> {
        // На macOS/Linux предоставляем тестовые виртуальные накопители для разработки и отладки UI
        let list = vec![
            // Системный диск
            DriveInfo {
            letter: 'C',
            root_path: "C:\\".to_string(),
            volume_name: "Windows-System".to_string(),
            file_system: "NTFS".to_string(),
            total_bytes: 512 * 1024 * 1024 * 1024,
            free_bytes: 180 * 1024 * 1024 * 1024,
            sector_size: 4096,
            is_removable: false,
            is_system: true,
            bus_type: BusType::Nvme,
            model: "Samsung SSD 980 PRO 500GB".to_string(),
        },
        // Пользовательский жесткий диск
        DriveInfo {
            letter: 'D',
            root_path: "D:\\".to_string(),
            volume_name: "Data_HDD".to_string(),
            file_system: "NTFS".to_string(),
            total_bytes: 2048 * 1024 * 1024 * 1024,
            free_bytes: 850 * 1024 * 1024 * 1024,
            sector_size: 4096,
            is_removable: false,
            is_system: false,
            bus_type: BusType::Sata,
            model: "WDC WD20EZAZ 2TB".to_string(),
        },
        // Съемная флешка 1 (USB 3.0)
        DriveInfo {
            letter: 'E',
            root_path: "E:\\".to_string(),
            volume_name: "KINGSTON".to_string(),
            file_system: "exFAT".to_string(),
            total_bytes: 32 * 1024 * 1024 * 1024,
            free_bytes: 31 * 1024 * 1024 * 1024,
            sector_size: 512,
            is_removable: true,
            is_system: false,
            bus_type: BusType::Usb,
            model: "Kingston DataTraveler 3.0 32GB".to_string(),
        },
        // Съемная флешка 2 (USB 2.0)
        DriveInfo {
            letter: 'F',
            root_path: "F:\\".to_string(),
            volume_name: "SANDISK".to_string(),
            file_system: "FAT32".to_string(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            free_bytes: 12 * 1024 * 1024 * 1024,
            sector_size: 512,
            is_removable: true,
            is_system: false,
            bus_type: BusType::Usb,
            model: "SanDisk Cruzer Blade 16GB".to_string(),
        },
        ];

        Ok(list)
    }

    fn get_system_drive_letter(&self) -> char {
        'C'
    }
}
