use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    pub const ACCENT: Color = Color::Cyan;
    pub const BG_HEADER: Color = Color::Rgb(20, 24, 34);
    pub const HEALTHY: Color = Color::Green;
    pub const WARNING: Color = Color::Yellow;
    pub const SLOW: Color = Color::Rgb(255, 140, 0); // Orange
    pub const CRITICAL: Color = Color::LightRed;
    pub const BAD: Color = Color::Red;
    pub const UNTESTED: Color = Color::DarkGray;

    pub fn title() -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn header() -> Style {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    }

    pub fn selected() -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn danger() -> Style {
        Style::default()
            .fg(Color::Red)
            .add_modifier(Modifier::BOLD)
    }

    pub fn success() -> Style {
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD)
    }
}
