use crate::core::{
    format_bytes, BlockStatus, DriveInfo, TestConfig, TestMode, TestPhase, TestProgressEvent,
    TestStats,
};
use crate::history::{HistoryManager, TestComparison, TestRecord};
use crate::io::TestEngine;
use crate::safety::{get_platform_scanner, validate_drive_for_testing};
use crate::ui::theme::Theme;
use chrono::Utc;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Gauge, List, ListItem, Paragraph, Row, Table,
        TableState,
    },
    Frame, Terminal,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    DriveSelect,
    TestConfig,
    Testing,
    TestFinished,
    History,
    Comparison,
    Feedback,
}

pub struct App {
    pub screen: Screen,
    pub running: bool,
    pub drives: Vec<DriveInfo>,
    pub selected_drive_idx: usize,
    pub config: TestConfig,
    pub test_stats: TestStats,
    pub test_phase: TestPhase,
    pub sector_blocks: Vec<BlockStatus>,
    pub grid_capacity: usize,
    pub test_logs: Vec<String>,
    pub test_cancel_flag: Arc<AtomicBool>,
    pub test_rx: Option<Receiver<TestProgressEvent>>,
    pub history_manager: HistoryManager,
    pub history_records: Vec<TestRecord>,
    pub selected_history_idx: usize,
    pub comparison_idx_a: Option<usize>,
    pub comparison_idx_b: Option<usize>,
    pub current_comparison: Option<TestComparison>,
    pub status_message: Option<String>,
    pub feedback_input: String,
    pub feedback_sent: bool,
    pub table_state: TableState,
}

impl App {
    pub fn new() -> Self {
        let scanner = get_platform_scanner();
        let drives = scanner.scan_drives().unwrap_or_default();
        let history_manager = HistoryManager::new_default();
        let history_records = history_manager.load().unwrap_or_default();

        let mut app = Self {
            screen: Screen::DriveSelect,
            running: true,
            drives,
            selected_drive_idx: 0,
            config: TestConfig::default(),
            test_stats: TestStats::default(),
            test_phase: TestPhase::Idle,
            sector_blocks: vec![BlockStatus::Untested; 300],
            grid_capacity: 300,
            test_logs: Vec::new(),
            test_cancel_flag: Arc::new(AtomicBool::new(false)),
            test_rx: None,
            history_manager,
            history_records,
            selected_history_idx: 0,
            comparison_idx_a: None,
            comparison_idx_b: None,
            current_comparison: None,
            status_message: None,
            feedback_input: String::new(),
            feedback_sent: false,
            table_state: TableState::default(),
        };

        if !app.drives.is_empty() {
            app.table_state.select(Some(0));
            // Автоматически наводимся на первый съемный диск
            if let Some(idx) = app.drives.iter().position(|d| d.is_removable && !d.is_system) {
                app.selected_drive_idx = idx;
                app.table_state.select(Some(idx));
            }
        }

        app
    }

    pub fn rescan_drives(&mut self) {
        let scanner = get_platform_scanner();
        self.drives = scanner.scan_drives().unwrap_or_default();
        if self.selected_drive_idx >= self.drives.len() && !self.drives.is_empty() {
            self.selected_drive_idx = 0;
        }
        self.table_state.select(Some(self.selected_drive_idx));
        self.status_message = Some("Список дисков успешно обновлен".to_string());
    }

    pub fn start_test(&mut self) {
        if self.selected_drive_idx >= self.drives.len() {
            return;
        }
        let drive = &self.drives[self.selected_drive_idx];

        if let Err(e) = validate_drive_for_testing(drive) {
            self.status_message = Some(format!("ОШИБКА БЕЗОПАСНОСТИ: {}", e));
            return;
        }

        self.config.drive_letter = drive.letter;
        if self.config.target_bytes == 0 {
            // По умолчанию тестируем до 1 ГБ или весь свободный объем (если меньше 1 ГБ)
            let test_limit = 1024 * 1024 * 1024; // 1 GiB
            self.config.target_bytes = if drive.free_bytes > 0 {
                drive.free_bytes.min(test_limit)
            } else {
                test_limit
            };
        }

        self.sector_blocks = vec![BlockStatus::Untested; self.grid_capacity];
        self.test_logs.clear();
        self.test_stats = TestStats::default();
        self.test_phase = TestPhase::Writing;
        self.test_cancel_flag.store(false, Ordering::Relaxed);

        let (tx, rx) = channel();
        self.test_rx = Some(rx);

        let mut engine = TestEngine::new(
            self.config.clone(),
            self.test_cancel_flag.clone(),
            tx,
        );

        // Запуск рабочего потока I/O
        std::thread::spawn(move || {
            let _ = engine.run();
        });

        self.screen = Screen::Testing;
    }

    pub fn cancel_test(&mut self) {
        self.test_cancel_flag.store(true, Ordering::Relaxed);
        self.test_phase = TestPhase::Aborted;
        self.test_logs.push("Тест прерван пользователем!".to_string());
    }

    pub fn update_test_progress(&mut self) {
        if let Some(ref rx) = self.test_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    TestProgressEvent::Started { phase, total_blocks, total_bytes } => {
                        self.test_phase = phase;
                        self.test_stats.total_blocks = total_blocks;
                        self.test_stats.total_bytes = total_bytes;
                    }
                    TestProgressEvent::PhaseChanged(phase) => {
                        self.test_phase = phase;
                    }
                    TestProgressEvent::BlockCompleted { block_index, status, speed_mbps, .. } => {
                        let total = self.test_stats.total_blocks.max(1);
                        let grid_idx = (block_index * self.grid_capacity) / total;
                        if grid_idx < self.sector_blocks.len() {
                            self.sector_blocks[grid_idx] = status;
                        }
                        self.test_stats.processed_blocks = block_index + 1;
                        self.test_stats.current_speed_mbps = speed_mbps;
                        match status {
                            BlockStatus::Healthy => self.test_stats.healthy_count += 1,
                            BlockStatus::Warning => self.test_stats.warning_count += 1,
                            BlockStatus::Slow => self.test_stats.slow_count += 1,
                            BlockStatus::Critical => self.test_stats.critical_count += 1,
                            BlockStatus::Bad => self.test_stats.bad_count += 1,
                            _ => {}
                        }
                    }
                    TestProgressEvent::SpeedUpdate { current_mbps, avg_mbps } => {
                        self.test_stats.current_speed_mbps = current_mbps;
                        if self.test_phase == TestPhase::Writing {
                            self.test_stats.avg_write_speed_mbps = avg_mbps;
                        } else {
                            self.test_stats.avg_read_speed_mbps = avg_mbps;
                        }
                    }
                    TestProgressEvent::LogMessage(msg) => {
                        self.test_logs.push(msg);
                    }
                    TestProgressEvent::Finished(stats) => {
                        self.test_stats = stats;
                        self.test_phase = TestPhase::Completed;
                        self.screen = Screen::TestFinished;
                    }
                    TestProgressEvent::Failed(err) => {
                        self.test_phase = TestPhase::Aborted;
                        self.test_logs.push(format!("Критическая ошибка: {}", err));
                        self.screen = Screen::TestFinished;
                    }
                }
            }
        }
    }

    pub fn save_current_to_history(&mut self) {
        if self.selected_drive_idx >= self.drives.len() {
            return;
        }
        let drive = &self.drives[self.selected_drive_idx];

        let record = TestRecord {
            id: format!("test_{}", Utc::now().format("%Y%m%d_%H%M%S")),
            timestamp: Utc::now(),
            drive_letter: drive.letter,
            drive_model: drive.model.clone(),
            volume_name: drive.volume_name.clone(),
            file_system: drive.file_system.clone(),
            total_capacity_bytes: drive.total_bytes,
            tested_bytes: self.test_stats.total_bytes,
            mode: self.config.mode,
            avg_write_speed_mbps: self.test_stats.avg_write_speed_mbps,
            avg_read_speed_mbps: self.test_stats.avg_read_speed_mbps,
            healthy_blocks: self.test_stats.healthy_count,
            warning_blocks: self.test_stats.warning_count,
            slow_blocks: self.test_stats.slow_count,
            critical_blocks: self.test_stats.critical_count,
            bad_blocks: self.test_stats.bad_count,
            is_fake_capacity: self.test_stats.is_fake_capacity,
            elapsed_seconds: self.test_stats.elapsed_secs,
        };

        if self.history_manager.append(record).is_ok() {
            self.history_records = self.history_manager.load().unwrap_or_default();
            self.status_message = Some("Результаты теста успешно сохранены в историю!".to_string());
        }
    }

    pub fn run_loop<B: ratatui::backend::Backend>(
        &mut self,
        terminal: &mut Terminal<B>,
    ) -> std::io::Result<()> {
        while self.running {
            self.update_test_progress();

            terminal.draw(|f| self.render(f))?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind == KeyEventKind::Press {
                        self.handle_key(key.code, key.modifiers);
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_key(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        if modifiers.contains(KeyModifiers::CONTROL) && code == KeyCode::Char('c') {
            if self.screen == Screen::Testing {
                self.cancel_test();
            } else {
                self.running = false;
            }
            return;
        }

        match self.screen {
            Screen::DriveSelect => match code {
                KeyCode::Char('q') | KeyCode::Esc => self.running = false,
                KeyCode::Up => {
                    if self.selected_drive_idx > 0 {
                        self.selected_drive_idx -= 1;
                        self.table_state.select(Some(self.selected_drive_idx));
                    }
                }
                KeyCode::Down => {
                    if !self.drives.is_empty() && self.selected_drive_idx + 1 < self.drives.len() {
                        self.selected_drive_idx += 1;
                        self.table_state.select(Some(self.selected_drive_idx));
                    }
                }
                KeyCode::Enter => {
                    if !self.drives.is_empty() {
                        let drive = &self.drives[self.selected_drive_idx];
                        if drive.is_system {
                            self.status_message = Some(
                                "ОШИБКА: Системный диск C: защищен и не может быть протестирован!".into(),
                            );
                        } else if !drive.is_removable {
                            self.status_message = Some(
                                "ВНИМАНИЕ: Выбран несъемный накопитель. Разрешены только USB/съемные диски.".into(),
                            );
                        } else {
                            self.screen = Screen::TestConfig;
                        }
                    }
                }
                KeyCode::Char('r') | KeyCode::Char('R') => self.rescan_drives(),
                KeyCode::Char('h') | KeyCode::Char('H') => {
                    self.history_records = self.history_manager.load().unwrap_or_default();
                    self.screen = Screen::History;
                }
                KeyCode::Char('f') | KeyCode::Char('F') => self.screen = Screen::Feedback,
                _ => {}
            },
            Screen::TestConfig => match code {
                KeyCode::Esc => self.screen = Screen::DriveSelect,
                KeyCode::Char('1') => self.config.mode = TestMode::WriteAndVerify,
                KeyCode::Char('2') => self.config.mode = TestMode::ReadVerifyOnly,
                KeyCode::Char('3') => self.config.mode = TestMode::FakeCapacityCheck,
                KeyCode::Enter => self.start_test(),
                _ => {}
            },
            Screen::Testing => match code {
                KeyCode::Esc | KeyCode::Char('c') => self.cancel_test(),
                _ => {}
            },
            Screen::TestFinished => match code {
                KeyCode::Char('s') | KeyCode::Char('S') => self.save_current_to_history(),
                KeyCode::Esc | KeyCode::Enter => self.screen = Screen::DriveSelect,
                _ => {}
            },
            Screen::History => match code {
                KeyCode::Esc => self.screen = Screen::DriveSelect,
                KeyCode::Up => {
                    if self.selected_history_idx > 0 {
                        self.selected_history_idx -= 1;
                    }
                }
                KeyCode::Down => {
                    if !self.history_records.is_empty()
                        && self.selected_history_idx + 1 < self.history_records.len()
                    {
                        self.selected_history_idx += 1;
                    }
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    if let Some(idx_a) = self.comparison_idx_a {
                        let idx_b = self.selected_history_idx;
                        if idx_a != idx_b && idx_a < self.history_records.len() && idx_b < self.history_records.len() {
                            let rec_a = &self.history_records[idx_a];
                            let rec_b = &self.history_records[idx_b];
                            self.current_comparison = Some(TestComparison::compare(rec_a, rec_b));
                            self.screen = Screen::Comparison;
                        } else {
                            self.status_message = Some("Выберите два разных теста для сравнения".into());
                        }
                    } else {
                        self.comparison_idx_a = Some(self.selected_history_idx);
                        self.status_message = Some("Выбран первый тест [A]. Теперь выберите второй [B] и нажмите 'c'".into());
                    }
                }
                _ => {}
            },
            Screen::Comparison => match code {
                KeyCode::Esc | KeyCode::Enter => {
                    self.comparison_idx_a = None;
                    self.comparison_idx_b = None;
                    self.screen = Screen::History;
                }
                _ => {}
            },
            Screen::Feedback => match code {
                KeyCode::Esc => self.screen = Screen::DriveSelect,
                KeyCode::Backspace => {
                    self.feedback_input.pop();
                }
                KeyCode::Enter => {
                    if !self.feedback_input.trim().is_empty() {
                        let _ = crate::net::send_feedback(&self.feedback_input, true);
                        self.feedback_sent = true;
                        self.status_message = Some("Спасибо! Ваш отзыв успешно отправлен.".into());
                        self.screen = Screen::DriveSelect;
                    }
                }
                KeyCode::Char(c) => {
                    self.feedback_input.push(c);
                }
                _ => {}
            },
        }
    }

    pub fn render(&mut self, f: &mut Frame) {
        let size = f.area();

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Заголовок
                Constraint::Min(10),   // Основное тело
                Constraint::Length(3), // Строка состояния / подсказки
            ])
            .split(size);

        self.render_header(f, chunks[0]);

        match self.screen {
            Screen::DriveSelect => self.render_drive_select(f, chunks[1]),
            Screen::TestConfig => self.render_test_config(f, chunks[1]),
            Screen::Testing => self.render_testing(f, chunks[1]),
            Screen::TestFinished => self.render_test_finished(f, chunks[1]),
            Screen::History => self.render_history(f, chunks[1]),
            Screen::Comparison => self.render_comparison(f, chunks[1]),
            Screen::Feedback => self.render_feedback(f, chunks[1]),
        }

        self.render_footer(f, chunks[2]);
    }

    fn render_header(&self, f: &mut Frame, area: Rect) {
        let title_spans = vec![
            Span::styled(" 🚀 USB TEST ", Theme::title()),
            Span::styled(format!("v{} ", env!("CARGO_PKG_VERSION")), Style::default().fg(Color::Yellow)),
            Span::styled("│ Высокопроизводительный безопасный диагностический комплекс", Style::default().fg(Color::Gray)),
        ];

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Cyan));

        let p = Paragraph::new(Line::from(title_spans))
            .block(block)
            .alignment(Alignment::Left);
        f.render_widget(p, area);
    }

    fn render_footer(&self, f: &mut Frame, area: Rect) {
        let help_text = match self.screen {
            Screen::DriveSelect => "▲/▼: Выбор │ Enter: Настроить тест │ R: Обновить диски │ H: История │ F: Отзыв │ Q: Выход",
            Screen::TestConfig => "1/2/3: Режим │ Enter: Запустить тест │ Esc: Назад",
            Screen::Testing => "Esc / C: Прервать тест",
            Screen::TestFinished => "S: Сохранить в историю │ Enter / Esc: В главное меню",
            Screen::History => "▲/▼: Навигация │ C: Выбрать для сравнения │ Esc: Назад",
            Screen::Comparison => "Esc / Enter: Назад в историю",
            Screen::Feedback => "Enter: Отправить (HTTPS Opt-In) │ Esc: Отмена (0 сетевых запросов)",
        };

        let mut spans = vec![Span::styled(help_text, Style::default().fg(Color::Yellow))];

        if let Some(ref msg) = self.status_message {
            spans.push(Span::raw(" │ "));
            spans.push(Span::styled(msg, Style::default().fg(Color::Cyan)));
        }

        let p = Paragraph::new(Line::from(spans))
            .block(Block::default().borders(Borders::ALL).border_type(BorderType::Rounded))
            .alignment(Alignment::Center);
        f.render_widget(p, area);
    }

    fn render_drive_select(&mut self, f: &mut Frame, area: Rect) {
        let header_cells = ["Диск", "Метка тома", "ФС", "Емкость", "Свободно", "Шина", "Модель / Статус"]
            .iter()
            .map(|h| Span::styled(*h, Theme::header()));
        let header = Row::new(header_cells).height(1).bottom_margin(1);

        let rows = self.drives.iter().enumerate().map(|(idx, d)| {
            let is_selected = idx == self.selected_drive_idx;
            let status_badge = if d.is_system {
                Span::styled(" СИСТЕМНЫЙ [ЗАЩИЩЕН] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            } else if d.is_removable {
                Span::styled(" СЪЕМНЫЙ USB [ГОТОВ] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" ВНУТРЕННИЙ ДИСК ", Style::default().fg(Color::Yellow))
            };

            let row_style = if is_selected {
                Style::default().bg(Color::Rgb(30, 45, 65)).fg(Color::White)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![
                Span::styled(format!("  {}:", d.letter), Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(&d.volume_name),
                Span::raw(&d.file_system),
                Span::raw(d.formatted_total_size()),
                Span::raw(d.formatted_free_size()),
                Span::raw(d.bus_type.to_string()),
                status_badge,
            ])
            .style(row_style)
            .height(1)
        });

        let widths = [
            Constraint::Length(8),
            Constraint::Length(16),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Min(25),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .title(" 💾 Обнаруженные диски (Поддерживается горячее подключение USB) ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            );

        f.render_stateful_widget(table, area, &mut self.table_state);
    }

    fn render_test_config(&self, f: &mut Frame, area: Rect) {
        if self.selected_drive_idx >= self.drives.len() {
            return;
        }
        let drive = &self.drives[self.selected_drive_idx];

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6),
                Constraint::Length(10),
                Constraint::Min(5),
            ])
            .split(area);

        // Инфо о диске
        let drive_info = vec![
            Line::from(vec![
                Span::raw("Выбран накопитель: "),
                Span::styled(format!("{}: ({})", drive.letter, drive.volume_name), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::raw(format!(" │ Емкость: {} │ ФС: {}", drive.formatted_total_size(), drive.file_system)),
            ]),
            Line::from(vec![
                Span::raw("Шина: "),
                Span::styled(drive.bus_type.to_string(), Style::default().fg(Color::Yellow)),
                Span::raw(format!(" │ Модель: {}", drive.model)),
            ]),
            Line::from(Span::styled("🛡️ Защита данных: Тестирование выполняется через безопасный небуферизованный контейнер без удаления файлов!", Style::default().fg(Color::Green))),
        ];

        let p_drive = Paragraph::new(drive_info)
            .block(Block::default().title(" Параметры накопителя ").borders(Borders::ALL).border_type(BorderType::Rounded));
        f.render_widget(p_drive, chunks[0]);

        // Выбор режима
        let mode_1_style = if self.config.mode == TestMode::WriteAndVerify { Theme::selected() } else { Style::default() };
        let mode_2_style = if self.config.mode == TestMode::ReadVerifyOnly { Theme::selected() } else { Style::default() };
        let mode_3_style = if self.config.mode == TestMode::FakeCapacityCheck { Theme::selected() } else { Style::default() };

        let modes = vec![
            Line::from(vec![
                Span::styled(" [1] ", Style::default().fg(Color::Yellow)),
                Span::styled("Полный тест: Запись и Верификация (Write & Verify Direct I/O)", mode_1_style),
                Span::raw(" - Создает контейнер, пишет уникальные паттерны, верифицирует данные."),
            ]),
            Line::from(vec![
                Span::styled(" [2] ", Style::default().fg(Color::Yellow)),
                Span::styled("Только чтение / Верификация (Read-Only Verify)", mode_2_style),
                Span::raw(" - Проверяет существующие секторы без модификации данных."),
            ]),
            Line::from(vec![
                Span::styled(" [3] ", Style::default().fg(Color::Yellow)),
                Span::styled("Экспресс-детектор поддельных флешек (Fake Capacity Check)", mode_3_style),
                Span::raw(" - Быстрый тест (до 1 мин) на выявление прошитых контроллеров."),
            ]),
        ];

        let p_modes = Paragraph::new(modes)
            .block(Block::default().title(" Выберите режим тестирования ").borders(Borders::ALL).border_type(BorderType::Rounded));
        f.render_widget(p_modes, chunks[1]);
    }

    fn render_testing(&self, f: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Фаза и Прогресс
                Constraint::Length(4), // Спидометр
                Constraint::Min(8),   // Сетка секторов
                Constraint::Length(6), // Логи
            ])
            .split(area);

        // 1. Прогресс-бар
        let pct = if self.test_stats.total_blocks > 0 {
            ((self.test_stats.processed_blocks as f64 / self.test_stats.total_blocks as f64) * 100.0) as u16
        } else {
            0
        };

        let phase_str = match self.test_phase {
            TestPhase::Writing => "ЗАПИСЬ (Direct I/O, No Buffering)",
            TestPhase::Reading => "ЧТЕНИЕ И ВЕРИФИКАЦИЯ ДАННЫХ",
            TestPhase::Verifying => "ПРОВЕРКА КОНТРОЛЬНЫХ ТОЧЕК",
            TestPhase::Completed => "ТЕСТ ЗАВЕРШЕН",
            TestPhase::Aborted => "ПРЕРВАНО",
            _ => "ПОДГОТОВКА",
        };

        let gauge = Gauge::default()
            .block(Block::default().title(format!(" Фаза: {} ", phase_str)).borders(Borders::ALL).border_type(BorderType::Rounded))
            .gauge_style(Style::default().fg(Color::Cyan).bg(Color::DarkGray))
            .percent(pct.min(100))
            .label(format!("{}% ({}/{} блоков)", pct, self.test_stats.processed_blocks, self.test_stats.total_blocks));
        f.render_widget(gauge, chunks[0]);

        // 2. Спидометр и статистика
        let stats_line = Line::from(vec![
            Span::raw("Текущая скорость: "),
            Span::styled(format!("{:.1} МБ/с", self.test_stats.current_speed_mbps), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
            Span::raw(" │ Средняя: "),
            Span::styled(
                format!("{:.1} МБ/с", if self.test_phase == TestPhase::Writing { self.test_stats.avg_write_speed_mbps } else { self.test_stats.avg_read_speed_mbps }),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw(" │ Здоровые: "),
            Span::styled(format!("{}", self.test_stats.healthy_count), Style::default().fg(Color::Green)),
            Span::raw(" │ Медленные: "),
            Span::styled(format!("{}", self.test_stats.slow_count + self.test_stats.critical_count), Style::default().fg(Color::Rgb(255, 140, 0))),
            Span::raw(" │ БИТЫЕ: "),
            Span::styled(format!("{}", self.test_stats.bad_count), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
        ]);

        let p_stats = Paragraph::new(stats_line)
            .block(Block::default().title(" Телеметрия накопителя ").borders(Borders::ALL).border_type(BorderType::Rounded));
        f.render_widget(p_stats, chunks[1]);

        // 3. Интерактивная карта секторов (Grid)
        self.render_sector_grid(f, chunks[2]);

        // 4. Логи
        let logs: Vec<ListItem> = self.test_logs.iter().rev().take(4).map(|l| ListItem::new(l.as_str())).collect();
        let list_logs = List::new(logs)
            .block(Block::default().title(" Журнал событий ").borders(Borders::ALL).border_type(BorderType::Rounded));
        f.render_widget(list_logs, chunks[3]);
    }

    fn render_sector_grid(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(" 🟩 Карта секторов накопителя (Зеленый: <50мс, Желтый: <200мс, Оранжевый: <500мс, Красный: Сбой) ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded);

        let inner_area = block.inner(area);
        f.render_widget(block, area);

        let width = inner_area.width as usize;
        let height = inner_area.height as usize;
        let total_cells = width * height;

        if total_cells == 0 {
            return;
        }

        let mut lines = Vec::new();
        let mut row_spans = Vec::new();

        for i in 0..total_cells {
            let block_idx = (i * self.sector_blocks.len()) / total_cells;
            let status = self.sector_blocks.get(block_idx).copied().unwrap_or(BlockStatus::Untested);

            let (symbol, color) = match status {
                BlockStatus::Healthy => ("■", Theme::HEALTHY),
                BlockStatus::Warning => ("■", Theme::WARNING),
                BlockStatus::Slow => ("■", Theme::SLOW),
                BlockStatus::Critical => ("■", Theme::CRITICAL),
                BlockStatus::Bad => ("✖", Theme::BAD),
                BlockStatus::Untested => ("·", Theme::UNTESTED),
            };

            row_spans.push(Span::styled(symbol, Style::default().fg(color)));

            if row_spans.len() == width {
                lines.push(Line::from(std::mem::take(&mut row_spans)));
            }
        }

        if !row_spans.is_empty() {
            lines.push(Line::from(row_spans));
        }

        let p_grid = Paragraph::new(lines);
        f.render_widget(p_grid, inner_area);
    }

    fn render_test_finished(&self, f: &mut Frame, area: Rect) {
        let is_fake = self.test_stats.is_fake_capacity;
        let bad = self.test_stats.bad_count;

        let verdict = if is_fake {
            Span::styled(" ❌ ВНИМАНИЕ: ОБНАРУЖЕНА ПОДДЕЛЬНАЯ ФЛЕШКА (FAKE CAPACITY)! ", Style::default().fg(Color::White).bg(Color::Red).add_modifier(Modifier::BOLD))
        } else if bad > 0 {
            Span::styled(format!(" ⚠️ ОБНАРУЖЕНЫ ПОВРЕЖДЕННЫЕ СЕКТОРЫ ({}) ", bad), Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD))
        } else {
            Span::styled(" ✅ НАКОПИТЕЛЬ ПОЛНОСТЬЮ ИСПРАВЕН! ОШИБОК НЕ ОБНАРУЖЕНО ", Style::default().fg(Color::Black).bg(Color::Green).add_modifier(Modifier::BOLD))
        };

        let summary = vec![
            Line::from(verdict),
            Line::raw(""),
            Line::from(vec![
                Span::raw("Протестировано данных: "),
                Span::styled(format_bytes(self.test_stats.total_bytes), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw("Средняя скорость записи: "),
                Span::styled(format!("{:.2} МБ/с", self.test_stats.avg_write_speed_mbps), Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::raw("Средняя скорость чтения: "),
                Span::styled(format!("{:.2} МБ/с", self.test_stats.avg_read_speed_mbps), Style::default().fg(Color::Cyan)),
            ]),
            Line::from(vec![
                Span::raw("Время выполнения: "),
                Span::styled(format!("{:.1} сек", self.test_stats.elapsed_secs), Style::default().fg(Color::White)),
            ]),
            Line::raw(""),
            Line::from(vec![
                Span::styled("Горячие клавиши: ", Style::default().fg(Color::Gray)),
                Span::styled("[S] Сохранить результат в историю", Style::default().fg(Color::Yellow)),
                Span::raw(" │ "),
                Span::styled("[Enter / Esc] Вернуться к списку дисков", Style::default().fg(Color::White)),
            ]),
        ];

        let p = Paragraph::new(summary)
            .block(Block::default().title(" Результаты тестирования ").borders(Borders::ALL).border_type(BorderType::Rounded))
            .alignment(Alignment::Center);
        f.render_widget(p, area);
    }

    fn render_history(&self, f: &mut Frame, area: Rect) {
        let header = Row::new(vec!["Дата", "Диск", "Модель", "Запись", "Чтение", "Битая память", "Статус"])
            .style(Theme::header())
            .height(1)
            .bottom_margin(1);

        let rows = self.history_records.iter().enumerate().map(|(idx, r)| {
            let is_sel = idx == self.selected_history_idx;
            let row_style = if is_sel {
                Style::default().bg(Color::Rgb(30, 45, 65)).fg(Color::White)
            } else {
                Style::default()
            };

            let status_span = if r.is_fake_capacity {
                Span::styled("ПОДДЕЛКА", Style::default().fg(Color::Red))
            } else if r.bad_blocks > 0 {
                Span::styled(format!("БИТЫХ: {}", r.bad_blocks), Style::default().fg(Color::Yellow))
            } else {
                Span::styled("ИСПРАВЕН", Style::default().fg(Color::Green))
            };

            Row::new(vec![
                Span::raw(r.timestamp.format("%Y-%m-%d %H:%M").to_string()),
                Span::raw(format!("{}:", r.drive_letter)),
                Span::raw(&r.drive_model),
                Span::raw(format!("{:.1} МБ/с", r.avg_write_speed_mbps)),
                Span::raw(format!("{:.1} МБ/с", r.avg_read_speed_mbps)),
                Span::raw(format!("{}", r.bad_blocks)),
                status_span,
            ])
            .style(row_style)
            .height(1)
        });

        let widths = [
            Constraint::Length(18),
            Constraint::Length(6),
            Constraint::Length(25),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Min(12),
        ];

        let table = Table::new(rows, widths)
            .header(header)
            .block(
                Block::default()
                    .title(" 📋 История проверок накопителей (Нажмите 'C' для выбора двух тестов и сравнения) ")
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded),
            );

        f.render_widget(table, area);
    }

    fn render_comparison(&self, f: &mut Frame, area: Rect) {
        if let Some(ref c) = self.current_comparison {
            let write_delta_style = if c.write_speed_delta_mbps < -5.0 {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };

            let read_delta_style = if c.read_speed_delta_mbps < -5.0 {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Green)
            };

            let degradation_banner = if c.degradation_detected {
                Span::styled(" ⚠️ ОБНАРУЖЕНА ДЕГРАДАЦИЯ ПАМЯТИ ИЛИ СНИЖЕНИЕ СКОРОСТИ! ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(Modifier::BOLD))
            } else {
                Span::styled(" ✅ Характеристики накопителя стабильны ", Style::default().fg(Color::Black).bg(Color::Green))
            };

            let lines = vec![
                Line::from(degradation_banner),
                Line::raw(""),
                Line::from(vec![
                    Span::raw("Модель: "),
                    Span::styled(&c.drive_model, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                ]),
                Line::raw(""),
                Line::from(vec![
                    Span::raw("Скорость записи [A -> B]: "),
                    Span::raw(format!("{:.1} МБ/с -> {:.1} МБ/с (", c.write_speed_a, c.write_speed_b)),
                    Span::styled(format!("{:+.1} МБ/с, {:+.1}%", c.write_speed_delta_mbps, c.write_speed_delta_percent), write_delta_style),
                    Span::raw(")"),
                ]),
                Line::from(vec![
                    Span::raw("Скорость чтения [A -> B]: "),
                    Span::raw(format!("{:.1} МБ/с -> {:.1} МБ/с (", c.read_speed_a, c.read_speed_b)),
                    Span::styled(format!("{:+.1} МБ/с, {:+.1}%", c.read_speed_delta_mbps, c.read_speed_delta_percent), read_delta_style),
                    Span::raw(")"),
                ]),
                Line::from(vec![
                    Span::raw("Дельта битых секторов: "),
                    Span::styled(format!("{:+}", c.bad_blocks_delta), if c.bad_blocks_delta > 0 { Style::default().fg(Color::Red) } else { Style::default().fg(Color::Green) }),
                ]),
            ];

            let p = Paragraph::new(lines)
                .block(Block::default().title(" Сравнение результатов тестов (Side-by-Side) ").borders(Borders::ALL).border_type(BorderType::Rounded))
                .alignment(Alignment::Center);
            f.render_widget(p, area);
        }
    }

    fn render_feedback(&self, f: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled("🔒 Конфиденциальность и безопасность (Opt-In)", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))),
            Line::raw("В отличие от старой версии на C++, отправка происходит исключительно по HTTPS и ТОЛЬКО с вашего явного согласия."),
            Line::raw("Если вы нажмете Esc — никакие сетевые запросы не будут отправлены."),
            Line::raw(""),
            Line::from(vec![
                Span::raw("Ваш отзыв / сообщение: "),
                Span::styled(&self.feedback_input, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("█", Style::default().fg(Color::White)), // Курсор
            ]),
        ];

        let p = Paragraph::new(lines)
            .block(Block::default().title(" Обратная связь ").borders(Borders::ALL).border_type(BorderType::Rounded))
            .alignment(Alignment::Left);
        f.render_widget(p, area);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
