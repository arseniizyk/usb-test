use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use crate::core::TestMode;
use crate::error::{Result, UsbTestError};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TestRecord {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub drive_letter: char,
    pub drive_model: String,
    pub volume_name: String,
    pub file_system: String,
    pub total_capacity_bytes: u64,
    pub tested_bytes: u64,
    pub mode: TestMode,
    pub avg_write_speed_mbps: f64,
    pub avg_read_speed_mbps: f64,
    pub healthy_blocks: u64,
    pub warning_blocks: u64,
    pub slow_blocks: u64,
    pub critical_blocks: u64,
    pub bad_blocks: u64,
    pub is_fake_capacity: bool,
    pub elapsed_seconds: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestComparison {
    pub id_a: String,
    pub id_b: String,
    pub drive_model: String,
    pub write_speed_a: f64,
    pub write_speed_b: f64,
    pub write_speed_delta_mbps: f64,
    pub write_speed_delta_percent: f64,
    pub read_speed_a: f64,
    pub read_speed_b: f64,
    pub read_speed_delta_mbps: f64,
    pub read_speed_delta_percent: f64,
    pub bad_blocks_a: u64,
    pub bad_blocks_b: u64,
    pub bad_blocks_delta: i64,
    pub degradation_detected: bool,
}

impl TestComparison {
    pub fn compare(a: &TestRecord, b: &TestRecord) -> Self {
        let write_speed_delta_mbps = b.avg_write_speed_mbps - a.avg_write_speed_mbps;
        let write_speed_delta_percent = if a.avg_write_speed_mbps > 0.001 {
            (write_speed_delta_mbps / a.avg_write_speed_mbps) * 100.0
        } else {
            0.0
        };

        let read_speed_delta_mbps = b.avg_read_speed_mbps - a.avg_read_speed_mbps;
        let read_speed_delta_percent = if a.avg_read_speed_mbps > 0.001 {
            (read_speed_delta_mbps / a.avg_read_speed_mbps) * 100.0
        } else {
            0.0
        };

        let bad_blocks_delta = b.bad_blocks as i64 - a.bad_blocks as i64;
        let degradation_detected = bad_blocks_delta > 0
            || write_speed_delta_percent < -25.0
            || read_speed_delta_percent < -25.0;

        Self {
            id_a: a.id.clone(),
            id_b: b.id.clone(),
            drive_model: b.drive_model.clone(),
            write_speed_a: a.avg_write_speed_mbps,
            write_speed_b: b.avg_write_speed_mbps,
            write_speed_delta_mbps,
            write_speed_delta_percent,
            read_speed_a: a.avg_read_speed_mbps,
            read_speed_b: b.avg_read_speed_mbps,
            read_speed_delta_mbps,
            read_speed_delta_percent,
            bad_blocks_a: a.bad_blocks,
            bad_blocks_b: b.bad_blocks,
            bad_blocks_delta,
            degradation_detected,
        }
    }
}

pub struct HistoryManager {
    file_path: PathBuf,
}

impl HistoryManager {
    pub fn new_default() -> Self {
        let base_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("USB Test");
        Self {
            file_path: base_dir.join("history.json"),
        }
    }

    pub fn with_path(path: PathBuf) -> Self {
        Self { file_path: path }
    }

    /// Безопасная загрузка истории тестов с обработкой поврежденных файлов (защита от краша nlohmann::json)
    pub fn load(&self) -> Result<Vec<TestRecord>> {
        if !self.file_path.exists() {
            return Ok(Vec::new());
        }

        let content = match std::fs::read_to_string(&self.file_path) {
            Ok(c) => c,
            Err(e) => {
                return Err(UsbTestError::HistoryError(format!(
                    "Не удалось прочитать файл истории {:?}: {}",
                    self.file_path, e
                )));
            }
        };

        if content.trim().is_empty() {
            return Ok(Vec::new());
        }

        match serde_json::from_str::<Vec<TestRecord>>(&content) {
            Ok(records) => Ok(records),
            Err(e) => {
                // Файл поврежден: создаем резервную копию вместо падения
                let backup_path = self.file_path.with_extension(format!(
                    "corrupted_{}.json",
                    Utc::now().format("%Y%m%d_%H%M%S")
                ));
                let _ = std::fs::rename(&self.file_path, &backup_path);

                Err(UsbTestError::HistoryError(format!(
                    "Файл истории поврежден ({}), создана резервная копия {:?}",
                    e, backup_path
                )))
            }
        }
    }

    /// Атомарное сохранение истории во избежание повреждения при аварийном завершении
    pub fn save(&self, records: &[TestRecord]) -> Result<()> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json_data = serde_json::to_string_pretty(records)?;
        let temp_path = self.file_path.with_extension("tmp");

        // Записываем во временный файл
        std::fs::write(&temp_path, json_data)?;

        // Атомарно заменяем рабочий файл
        std::fs::rename(&temp_path, &self.file_path)?;
        Ok(())
    }

    /// Добавить запись о новом тесте
    pub fn append(&self, record: TestRecord) -> Result<()> {
        let mut records = self.load().unwrap_or_default();
        records.push(record);
        self.save(&records)
    }

    /// Экспорт истории в указанный файл
    pub fn export_to_file(&self, destination: &Path) -> Result<()> {
        let records = self.load()?;
        let json_data = serde_json::to_string_pretty(&records)?;
        std::fs::write(destination, json_data)?;
        Ok(())
    }

    /// Импорт истории из внешнего файла с валидацией схемы
    pub fn import_from_file(&self, source: &Path) -> Result<usize> {
        let content = std::fs::read_to_string(source)?;
        let imported: Vec<TestRecord> = serde_json::from_str(&content)
            .map_err(|e| UsbTestError::HistoryError(format!("Некорректный формат файла импорта: {}", e)))?;

        let count = imported.len();
        let mut current = self.load().unwrap_or_default();

        for item in imported {
            if !current.iter().any(|r| r.id == item.id) {
                current.push(item);
            }
        }

        self.save(&current)?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn make_sample_record(id: &str, write_speed: f64, bad_blocks: u64) -> TestRecord {
        TestRecord {
            id: id.to_string(),
            timestamp: Utc::now(),
            drive_letter: 'E',
            drive_model: "FlashDrive 3.0".into(),
            volume_name: "USB_DISK".into(),
            file_system: "FAT32".into(),
            total_capacity_bytes: 32 * 1024 * 1024 * 1024,
            tested_bytes: 1024 * 1024 * 1024,
            mode: TestMode::WriteAndVerify,
            avg_write_speed_mbps: write_speed,
            avg_read_speed_mbps: 80.0,
            healthy_blocks: 1000,
            warning_blocks: 5,
            slow_blocks: 2,
            critical_blocks: 0,
            bad_blocks,
            is_fake_capacity: false,
            elapsed_seconds: 30.0,
        }
    }

    #[test]
    fn test_atomic_history_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.json");
        let manager = HistoryManager::with_path(path.clone());

        assert!(manager.load().unwrap().is_empty());

        let rec = make_sample_record("rec1", 45.0, 0);
        manager.append(rec.clone()).unwrap();

        let loaded = manager.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].id, "rec1");
    }

    #[test]
    fn test_corrupted_history_does_not_crash() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("history.json");
        std::fs::write(&path, "INVALID JSON CONTENT {{{").unwrap();

        let manager = HistoryManager::with_path(path.clone());
        let res = manager.load();
        assert!(res.is_err());
        // Проверяем, что исходный файл был переименован в backup, а программа не упала
        assert!(!path.exists());
    }

    #[test]
    fn test_comparison_delta() {
        let a = make_sample_record("test1", 50.0, 0);
        let b = make_sample_record("test2", 25.0, 2);

        let comp = TestComparison::compare(&a, &b);
        assert_eq!(comp.write_speed_delta_mbps, -25.0);
        assert_eq!(comp.write_speed_delta_percent, -50.0);
        assert_eq!(comp.bad_blocks_delta, 2);
        assert!(comp.degradation_detected);
    }
}
