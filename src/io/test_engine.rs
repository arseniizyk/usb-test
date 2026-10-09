use crate::core::{
    BlockStatus, TestConfig, TestMode, TestPhase, TestProgressEvent, TestStats,
};
use crate::error::{Result, UsbTestError};
use crate::io::aligned_buffer::AlignedBuffer;
use crate::io::direct_file::DirectFile;
use crate::io::pattern::{fill_block_pattern, verify_block_pattern, VerificationResult};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::Instant;

pub struct TestEngine {
    config: TestConfig,
    cancel_flag: Arc<AtomicBool>,
    event_tx: Sender<TestProgressEvent>,
}

impl TestEngine {
    pub fn new(
        config: TestConfig,
        cancel_flag: Arc<AtomicBool>,
        event_tx: Sender<TestProgressEvent>,
    ) -> Self {
        Self {
            config,
            cancel_flag,
            event_tx,
        }
    }

    pub fn run(&mut self) -> Result<TestStats> {
        match self.config.mode {
            TestMode::WriteAndVerify => self.run_write_and_verify(),
            TestMode::ReadVerifyOnly => self.run_read_verify_only(),
            TestMode::FakeCapacityCheck => self.run_fake_capacity_check(),
        }
    }

    fn get_target_file_path(&self) -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(format!("{}:\\{}", self.config.drive_letter, self.config.test_file_name))
        }
        #[cfg(not(windows))]
        {
            std::env::temp_dir().join(&self.config.test_file_name)
        }
    }

    fn run_write_and_verify(&mut self) -> Result<TestStats> {
        let chunk_size = self.config.chunk_size;
        let target_bytes = self.config.target_bytes;
        assert!(chunk_size > 0 && chunk_size.is_multiple_of(4096));

        let total_blocks = (target_bytes / chunk_size as u64) as usize;
        let test_file_path = self.get_target_file_path();

        let mut stats = TestStats {
            total_bytes: target_bytes,
            total_blocks,
            ..Default::default()
        };

        let seed = rand::random::<u64>();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // ==========================================
        // ФАЗА 1: ПРЯМАЯ НЕБУФЕРИЗОВАННАЯ ЗАПИСЬ
        // ==========================================
        let _ = self.event_tx.send(TestProgressEvent::Started {
            phase: TestPhase::Writing,
            total_blocks,
            total_bytes: target_bytes,
        });

        let mut buffer = AlignedBuffer::new(chunk_size);
        let write_start_time = Instant::now();

        {
            let mut df = DirectFile::open_for_write(&test_file_path)?;

            for block_idx in 0..total_blocks {
                if self.cancel_flag.load(Ordering::Relaxed) {
                    let _ = self.event_tx.send(TestProgressEvent::PhaseChanged(TestPhase::Aborted));
                    return Err(UsbTestError::Cancelled);
                }

                fill_block_pattern(&mut buffer, block_idx as u64, seed, timestamp);

                let block_start = Instant::now();
                let offset = block_idx as u64 * chunk_size as u64;

                let write_res = df.write_block(offset, &buffer);
                let latency_ms = block_start.elapsed().as_millis() as u64;

                let status = match write_res {
                    Ok(_) => {
                        stats.bytes_written += chunk_size as u64;
                        match BlockStatus::from_latency(latency_ms) {
                            BlockStatus::Healthy => {
                                stats.healthy_count += 1;
                                BlockStatus::Healthy
                            }
                            BlockStatus::Warning => {
                                stats.warning_count += 1;
                                BlockStatus::Warning
                            }
                            BlockStatus::Slow => {
                                stats.slow_count += 1;
                                BlockStatus::Slow
                            }
                            _ => {
                                stats.critical_count += 1;
                                BlockStatus::Critical
                            }
                        }
                    }
                    Err(e) => {
                        stats.bad_count += 1;
                        let _ = self.event_tx.send(TestProgressEvent::LogMessage(format!(
                            "Ошибка записи блока {}: {}",
                            block_idx, e
                        )));
                        BlockStatus::Bad
                    }
                };

                let elapsed_secs = write_start_time.elapsed().as_secs_f64();
                let speed_mbps = safe_speed_mbps(stats.bytes_written, elapsed_secs);
                stats.current_speed_mbps = speed_mbps;
                stats.avg_write_speed_mbps = speed_mbps;
                stats.processed_blocks = block_idx + 1;

                let _ = self.event_tx.send(TestProgressEvent::BlockCompleted {
                    block_index: block_idx,
                    status,
                    latency_ms,
                    speed_mbps,
                });
            }

            df.sync()?;
        } // Файл записи гарантированно закрыт перед переходом к чтению (устранение Issue 2.7)

        // ==========================================
        // ФАЗА 2: ВЕРИФИКАЦИЯ ЧТЕНИЯ
        // ==========================================
        let _ = self.event_tx.send(TestProgressEvent::PhaseChanged(TestPhase::Reading));
        let read_start_time = Instant::now();

        // Сбрасываем счетчики для отображения прогресса верификации
        stats.healthy_count = 0;
        stats.warning_count = 0;
        stats.slow_count = 0;
        stats.critical_count = 0;

        {
            let mut df = DirectFile::open_for_read(&test_file_path)?;

            for block_idx in 0..total_blocks {
                if self.cancel_flag.load(Ordering::Relaxed) {
                    let _ = self.event_tx.send(TestProgressEvent::PhaseChanged(TestPhase::Aborted));
                    return Err(UsbTestError::Cancelled);
                }

                let block_start = Instant::now();
                let offset = block_idx as u64 * chunk_size as u64;

                let read_res = df.read_block(offset, &mut buffer);
                let latency_ms = block_start.elapsed().as_millis() as u64;

                let status = match read_res {
                    Ok(_) => {
                        stats.bytes_read += chunk_size as u64;
                        let ver_res = verify_block_pattern(&buffer, block_idx as u64, seed);
                        match ver_res {
                            VerificationResult::Ok => match BlockStatus::from_latency(latency_ms) {
                                BlockStatus::Healthy => {
                                    stats.healthy_count += 1;
                                    BlockStatus::Healthy
                                }
                                BlockStatus::Warning => {
                                    stats.warning_count += 1;
                                    BlockStatus::Warning
                                }
                                BlockStatus::Slow => {
                                    stats.slow_count += 1;
                                    BlockStatus::Slow
                                }
                                _ => {
                                    stats.critical_count += 1;
                                    BlockStatus::Critical
                                }
                            },
                            VerificationResult::AddressWrapAround { expected, found } => {
                                stats.bad_count += 1;
                                stats.is_fake_capacity = true;
                                let _ = self.event_tx.send(TestProgressEvent::LogMessage(format!(
                                    "ОБНАРУЖЕНА ПОДДЕЛКА! Блок {} перезаписан блоком {} (Wrap-around)",
                                    expected, found
                                )));
                                BlockStatus::Bad
                            }
                            VerificationResult::AllZeroes => {
                                stats.bad_count += 1;
                                stats.is_fake_capacity = true;
                                let _ = self.event_tx.send(TestProgressEvent::LogMessage(format!(
                                    "Блок {}: контроллер сбросил запись (все нули)",
                                    block_idx
                                )));
                                BlockStatus::Bad
                            }
                            other => {
                                stats.bad_count += 1;
                                let _ = self.event_tx.send(TestProgressEvent::LogMessage(format!(
                                    "Блок {}: ошибка данных: {:?}",
                                    block_idx, other
                                )));
                                BlockStatus::Bad
                            }
                        }
                    }
                    Err(e) => {
                        stats.bad_count += 1;
                        let _ = self.event_tx.send(TestProgressEvent::LogMessage(format!(
                            "Ошибка чтения блока {}: {}",
                            block_idx, e
                        )));
                        BlockStatus::Bad
                    }
                };

                let elapsed_secs = read_start_time.elapsed().as_secs_f64();
                let speed_mbps = safe_speed_mbps(stats.bytes_read, elapsed_secs);
                stats.current_speed_mbps = speed_mbps;
                stats.avg_read_speed_mbps = speed_mbps;
                stats.processed_blocks = block_idx + 1;

                let _ = self.event_tx.send(TestProgressEvent::BlockCompleted {
                    block_index: block_idx,
                    status,
                    latency_ms,
                    speed_mbps,
                });
            }
        } // Файл чтения закрыт

        // Удаление тестового файла
        if self.config.clean_up_after_test && test_file_path.exists() {
            let _ = std::fs::remove_file(&test_file_path);
        }

        stats.elapsed_secs = (write_start_time.elapsed() + read_start_time.elapsed()).as_secs_f64();
        let _ = self.event_tx.send(TestProgressEvent::Finished(stats.clone()));

        Ok(stats)
    }

    fn run_read_verify_only(&mut self) -> Result<TestStats> {
        let chunk_size = self.config.chunk_size;
        let test_file_path = self.get_target_file_path();

        if !test_file_path.exists() {
            return Err(UsbTestError::DriveNotFound(format!(
                "Тестовый файл не найден: {:?}",
                test_file_path
            )));
        }

        let file_meta = std::fs::metadata(&test_file_path)?;
        let total_bytes = file_meta.len();
        let total_blocks = (total_bytes / chunk_size as u64) as usize;

        let mut stats = TestStats {
            total_bytes,
            total_blocks,
            ..Default::default()
        };

        let _ = self.event_tx.send(TestProgressEvent::Started {
            phase: TestPhase::Reading,
            total_blocks,
            total_bytes,
        });

        let mut buffer = AlignedBuffer::new(chunk_size);
        let read_start_time = Instant::now();

        {
            let mut df = DirectFile::open_for_read(&test_file_path)?;

            for block_idx in 0..total_blocks {
                if self.cancel_flag.load(Ordering::Relaxed) {
                    let _ = self.event_tx.send(TestProgressEvent::PhaseChanged(TestPhase::Aborted));
                    return Err(UsbTestError::Cancelled);
                }

                let block_start = Instant::now();
                let offset = block_idx as u64 * chunk_size as u64;

                let read_res = df.read_block(offset, &mut buffer);
                let latency_ms = block_start.elapsed().as_millis() as u64;

                let status = match read_res {
                    Ok(_) => {
                        stats.bytes_read += chunk_size as u64;
                        match BlockStatus::from_latency(latency_ms) {
                            BlockStatus::Healthy => {
                                stats.healthy_count += 1;
                                BlockStatus::Healthy
                            }
                            BlockStatus::Warning => {
                                stats.warning_count += 1;
                                BlockStatus::Warning
                            }
                            BlockStatus::Slow => {
                                stats.slow_count += 1;
                                BlockStatus::Slow
                            }
                            _ => {
                                stats.critical_count += 1;
                                BlockStatus::Critical
                            }
                        }
                    }
                    Err(e) => {
                        stats.bad_count += 1;
                        let _ = self.event_tx.send(TestProgressEvent::LogMessage(format!(
                            "Ошибка чтения блока {}: {}",
                            block_idx, e
                        )));
                        BlockStatus::Bad
                    }
                };

                let elapsed_secs = read_start_time.elapsed().as_secs_f64();
                let speed_mbps = safe_speed_mbps(stats.bytes_read, elapsed_secs);
                stats.current_speed_mbps = speed_mbps;
                stats.avg_read_speed_mbps = speed_mbps;
                stats.processed_blocks = block_idx + 1;

                let _ = self.event_tx.send(TestProgressEvent::BlockCompleted {
                    block_index: block_idx,
                    status,
                    latency_ms,
                    speed_mbps,
                });
            }
        }

        stats.elapsed_secs = read_start_time.elapsed().as_secs_f64();
        let _ = self.event_tx.send(TestProgressEvent::Finished(stats.clone()));
        Ok(stats)
    }

    fn run_fake_capacity_check(&mut self) -> Result<TestStats> {
        // Экспресс-проверка поддельных флешек по геометрической прогрессии смещений
        let test_file_path = self.get_target_file_path();
        let chunk_size = 1024 * 1024; // 1 MiB контрольные маркеры
        let target_bytes = self.config.target_bytes;

        let mut stats = TestStats {
            total_bytes: target_bytes,
            ..Default::default()
        };

        // Точки проверки: 16 МБ, 32 МБ, 64 МБ, 128 МБ, 256 МБ, 512 МБ, 1 ГБ, 2 ГБ, 4 ГБ, 8 ГБ, 16 ГБ, 32 ГБ, 64 ГБ...
        let mut checkpoints = Vec::new();
        checkpoints.push(0u64); // Нулевой блок

        let mut offset = 16 * 1024 * 1024;
        while offset < target_bytes {
            checkpoints.push(offset);
            offset = offset.saturating_mul(2);
        }
        if target_bytes > chunk_size as u64 {
            let last_offset = (target_bytes - chunk_size as u64) / chunk_size as u64 * chunk_size as u64;
            if !checkpoints.contains(&last_offset) {
                checkpoints.push(last_offset);
            }
        }

        let total_blocks = checkpoints.len();
        stats.total_blocks = total_blocks;

        let _ = self.event_tx.send(TestProgressEvent::Started {
            phase: TestPhase::Writing,
            total_blocks,
            total_bytes: (total_blocks * chunk_size) as u64,
        });

        let seed = 0xDEADBEEFCAFE;
        let timestamp = 12345;
        let mut buffer = AlignedBuffer::new(chunk_size);

        // 1. Записываем контрольные маркеры на всех границах
        {
            let mut df = DirectFile::open_for_write(&test_file_path)?;
            for (idx, &cp_offset) in checkpoints.iter().enumerate() {
                if self.cancel_flag.load(Ordering::Relaxed) {
                    return Err(UsbTestError::Cancelled);
                }

                fill_block_pattern(&mut buffer, idx as u64, seed, timestamp);
                df.write_block(cp_offset, &buffer)?;

                let _ = self.event_tx.send(TestProgressEvent::BlockCompleted {
                    block_index: idx,
                    status: BlockStatus::Healthy,
                    latency_ms: 5,
                    speed_mbps: 50.0,
                });
            }
            df.sync()?;
        }

        // 2. Считываем и проверяем каждый маркер, начиная с блока 0
        let _ = self.event_tx.send(TestProgressEvent::PhaseChanged(TestPhase::Verifying));
        {
            let mut df = DirectFile::open_for_read(&test_file_path)?;
            for (idx, &cp_offset) in checkpoints.iter().enumerate() {
                if self.cancel_flag.load(Ordering::Relaxed) {
                    return Err(UsbTestError::Cancelled);
                }

                df.read_block(cp_offset, &mut buffer)?;
                let res = verify_block_pattern(&buffer, idx as u64, seed);

                let status = match res {
                    VerificationResult::Ok => {
                        stats.healthy_count += 1;
                        BlockStatus::Healthy
                    }
                    _ => {
                        stats.bad_count += 1;
                        stats.is_fake_capacity = true;
                        let _ = self.event_tx.send(TestProgressEvent::LogMessage(format!(
                            "Контрольная точка по смещению {} байт повреждена или перезаписана! Флешка поддельная!",
                            cp_offset
                        )));
                        BlockStatus::Bad
                    }
                };

                let _ = self.event_tx.send(TestProgressEvent::BlockCompleted {
                    block_index: idx,
                    status,
                    latency_ms: 5,
                    speed_mbps: 50.0,
                });
            }
        }

        if self.config.clean_up_after_test && test_file_path.exists() {
            let _ = std::fs::remove_file(&test_file_path);
        }

        let _ = self.event_tx.send(TestProgressEvent::Finished(stats.clone()));
        Ok(stats)
    }
}

/// Безопасное вычисление скорости без риска деления на ноль и переполнения в Infinity
pub fn safe_speed_mbps(bytes: u64, elapsed_secs: f64) -> f64 {
    if elapsed_secs <= 0.0001 || bytes == 0 {
        0.0
    } else {
        let mb = bytes as f64 / 1_048_576.0;
        let speed = mb / elapsed_secs;
        if speed.is_finite() {
            speed
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_speed_calculation() {
        assert_eq!(safe_speed_mbps(0, 0.0), 0.0);
        assert_eq!(safe_speed_mbps(1048576, 0.0), 0.0);
        assert_eq!(safe_speed_mbps(1048576, 1.0), 1.0);
        assert_eq!(safe_speed_mbps(10485760, 2.0), 5.0);
    }
}
