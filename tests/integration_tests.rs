use std::sync::atomic::AtomicBool;
use std::sync::mpsc::channel;
use std::sync::Arc;
use usb_test::core::{TestConfig, TestMode, TestPhase, TestProgressEvent};
use usb_test::history::{HistoryManager, TestRecord};
use usb_test::io::TestEngine;

#[test]
fn test_end_to_end_write_and_verify_cycle() {
    let temp_name = format!(".test_cycle_{}.tmp", rand::random::<u32>());
    let config = TestConfig {
        drive_letter: 'T',
        mode: TestMode::WriteAndVerify,
        chunk_size: 64 * 1024,
        target_bytes: 512 * 1024, // 512 KiB (8 блоков по 64 KiB)
        test_file_name: temp_name,
        clean_up_after_test: true,
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();

    let mut engine = TestEngine::new(config, cancel_flag, tx);
    let stats = engine.run().expect("Тест записи и верификации должен пройти успешно");

    assert_eq!(stats.total_blocks, 8);
    assert_eq!(stats.processed_blocks, 8);
    assert_eq!(stats.bytes_written, 512 * 1024);
    assert_eq!(stats.bytes_read, 512 * 1024);
    assert_eq!(stats.bad_count, 0);
    assert_eq!(stats.healthy_count, 8);
    assert!(!stats.is_fake_capacity);

    // Проверяем поток событий
    let events: Vec<TestProgressEvent> = rx.try_iter().collect();
    assert!(!events.is_empty());
    assert!(events.iter().any(|e| matches!(e, TestProgressEvent::Started { .. })));
    assert!(events.iter().any(|e| matches!(e, TestProgressEvent::PhaseChanged(TestPhase::Reading))));
    assert!(events.iter().any(|e| matches!(e, TestProgressEvent::Finished(_))));
}

#[test]
fn test_end_to_end_fake_capacity_check() {
    let temp_name = format!(".test_fake_{}.tmp", rand::random::<u32>());
    let config = TestConfig {
        drive_letter: 'T',
        mode: TestMode::FakeCapacityCheck,
        chunk_size: 1024 * 1024,
        target_bytes: 64 * 1024 * 1024, // 64 MiB
        test_file_name: temp_name,
        clean_up_after_test: true,
    };

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let (tx, _rx) = channel();

    let mut engine = TestEngine::new(config, cancel_flag, tx);
    let stats = engine.run().expect("Экспресс-проверка подлинности должна пройти успешно");

    assert!(stats.total_blocks > 0);
    assert_eq!(stats.bad_count, 0);
    assert!(!stats.is_fake_capacity);
}

#[test]
fn test_history_export_and_import() {
    let dir = tempfile::tempdir().unwrap();
    let history_file = dir.path().join("history.json");
    let export_file = dir.path().join("export.json");

    let manager = HistoryManager::with_path(history_file.clone());

    let record = TestRecord {
        id: "exp_1".to_string(),
        timestamp: chrono::Utc::now(),
        drive_letter: 'E',
        drive_model: "Kingston Ex".to_string(),
        volume_name: "KINGSTON".to_string(),
        file_system: "FAT32".to_string(),
        total_capacity_bytes: 16 * 1024 * 1024 * 1024,
        tested_bytes: 1024 * 1024 * 1024,
        mode: TestMode::WriteAndVerify,
        avg_write_speed_mbps: 35.5,
        avg_read_speed_mbps: 90.0,
        healthy_blocks: 500,
        warning_blocks: 0,
        slow_blocks: 0,
        critical_blocks: 0,
        bad_blocks: 0,
        is_fake_capacity: false,
        elapsed_seconds: 15.0,
    };

    manager.append(record.clone()).unwrap();

    // Экспорт
    manager.export_to_file(&export_file).unwrap();
    assert!(export_file.exists());

    // Создаем второй менеджер в другой директории и импортируем
    let new_history_file = dir.path().join("new_history.json");
    let new_manager = HistoryManager::with_path(new_history_file);

    let count = new_manager.import_from_file(&export_file).unwrap();
    assert_eq!(count, 1);

    let loaded = new_manager.load().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].id, "exp_1");
    assert_eq!(loaded[0].avg_write_speed_mbps, 35.5);
}
