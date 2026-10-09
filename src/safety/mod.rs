use crate::core::DriveInfo;
use crate::error::{Result, UsbTestError};

#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub mod mock;

pub trait DriveScanner: Send + Sync {
    /// Сканирует и возвращает список доступных накопителей
    fn scan_drives(&self) -> Result<Vec<DriveInfo>>;

    /// Сканирует только съемные безопасные накопители (фильтрация DRIVE_REMOVABLE и исключение C:)
    fn scan_removable_drives(&self) -> Result<Vec<DriveInfo>> {
        let all = self.scan_drives()?;
        Ok(all.into_iter().filter(|d| d.is_removable && !d.is_system).collect())
    }

    /// Получить букву системного диска
    fn get_system_drive_letter(&self) -> char;
}

/// Получить экземпляр сканера для текущей платформы
pub fn get_platform_scanner() -> Box<dyn DriveScanner> {
    #[cfg(windows)]
    {
        Box::new(windows::WindowsDriveScanner::new())
    }
    #[cfg(not(windows))]
    {
        Box::new(mock::MockDriveScanner::new())
    }
}

/// Строгая проверка безопасности накопителя перед запуском тестирования
pub fn validate_drive_for_testing(drive: &DriveInfo) -> Result<()> {
    if drive.is_system {
        return Err(UsbTestError::SystemDriveProtected(format!(
            "{}: ({})",
            drive.letter, drive.volume_name
        )));
    }

    if !drive.is_removable {
        return Err(UsbTestError::NonRemovableDrive(format!(
            "{}: ({}, Bus: {})",
            drive.letter, drive.volume_name, drive.bus_type
        )));
    }

    if drive.total_bytes == 0 {
        return Err(UsbTestError::DriveNotFound(format!(
            "Диск {}: пуст или не готов к чтению/записи",
            drive.letter
        )));
    }

    Ok(())
}

/// Двухэтапная проверка подтверждения форматирования
/// Исключает случайное форматирование системных дисков и требует точного совпадения буквы диска
pub fn verify_format_confirmation(drive: &DriveInfo, typed_confirmation: &str) -> Result<()> {
    // Никогда не разрешать форматирование системного диска
    if drive.is_system {
        return Err(UsbTestError::SystemDriveProtected(format!(
            "{}: - Системный диск никогда не может быть отформатирован через USB Test!",
            drive.letter
        )));
    }

    if !drive.is_removable {
        return Err(UsbTestError::NonRemovableDrive(format!(
            "{}: - Форматирование несъемных дисков запрещено!",
            drive.letter
        )));
    }

    let expected = format!("{}:", drive.letter.to_ascii_uppercase());
    let trimmed = typed_confirmation.trim().to_ascii_uppercase();

    if trimmed == expected || trimmed == drive.letter.to_ascii_uppercase().to_string() {
        Ok(())
    } else {
        Err(UsbTestError::Cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::BusType;

    fn test_drive(letter: char, is_removable: bool, is_system: bool) -> DriveInfo {
        DriveInfo {
            letter,
            root_path: format!("{}:\\", letter),
            volume_name: "TestDrive".into(),
            file_system: "exFAT".into(),
            total_bytes: 16 * 1024 * 1024 * 1024,
            free_bytes: 15 * 1024 * 1024 * 1024,
            sector_size: 512,
            is_removable,
            is_system,
            bus_type: BusType::Usb,
            model: "SanDisk Ultra".into(),
        }
    }

    #[test]
    fn test_rejects_system_drive() {
        let sys_drive = test_drive('C', false, true);
        assert!(validate_drive_for_testing(&sys_drive).is_err());
        assert!(verify_format_confirmation(&sys_drive, "C").is_err());
    }

    #[test]
    fn test_rejects_fixed_drive() {
        let fixed_drive = test_drive('D', false, false);
        assert!(validate_drive_for_testing(&fixed_drive).is_err());
    }

    #[test]
    fn test_accepts_removable_usb() {
        let usb_drive = test_drive('E', true, false);
        assert!(validate_drive_for_testing(&usb_drive).is_ok());
        assert!(verify_format_confirmation(&usb_drive, "E").is_ok());
        assert!(verify_format_confirmation(&usb_drive, "e:").is_ok());
        assert!(verify_format_confirmation(&usb_drive, "wrong").is_err());
    }
}
