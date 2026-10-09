use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::stdout;
use usb_test::net;
use usb_test::ui::App;

fn main() -> anyhow::Result<()> {
    // 1. Установка hook на случай паники для гарантированного восстановления терминала
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(stdout(), LeaveAlternateScreen);
        default_panic(info);
    }));

    // 2. Инициализация TUI терминала
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // 3. Создание приложения
    let mut app = App::new();

    // 4. Фоновая асинхронная проверка обновлений (без блокировки UI - решение Issue 4.3)
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    net::check_updates_async(current_version, |res| {
        if let Ok(Some(update)) = res {
            println!("\x07"); // bell
            eprintln!("Доступна новая версия: v{}", update.latest_version);
        }
    });

    // 5. Запуск бесконечного цикла событий (без рекурсии - решение Issue 2.3)
    let res = app.run_loop(&mut terminal);

    // 6. Восстановление исходного состояния терминала
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Ошибка приложения: {:?}", err);
    }

    Ok(())
}
