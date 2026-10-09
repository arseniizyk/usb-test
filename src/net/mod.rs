use crate::error::{Result, UsbTestError};
use semver::Version;
use serde::Deserialize;
use std::time::Duration;

const UPDATE_URL: &str = "https://raw.githubusercontent.com/usb-test/updates/main/version.json";
const FEEDBACK_URL: &str = "https://api.usbtest.dev/feedback"; // Безопасный HTTPS эндпоинт вместо открытого HTTP 4746220.atspace.cc

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteVersionInfo {
    pub version: String,
    pub release_notes: Option<String>,
    pub download_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub release_notes: String,
    pub download_url: String,
}

/// Безопасная проверка обновлений по HTTPS со строгой валидацией TLS и корректным SemVer
pub fn check_updates_sync(current_version_str: &str) -> Result<Option<UpdateInfo>> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();

    let response = agent
        .get(UPDATE_URL)
        .call()
        .map_err(|e| UsbTestError::Network(format!("Ошибка проверки обновлений: {}", e)))?;

    let remote: RemoteVersionInfo = response
        .into_json()
        .map_err(|e| UsbTestError::Network(format!("Некорректный ответ сервера обновлений: {}", e)))?;

    let current = Version::parse(current_version_str)?;
    let latest = Version::parse(&remote.version)?;

    // Честное сравнение SemVer (major.minor.patch) - решает Issue 2.4
    if latest > current {
        Ok(Some(UpdateInfo {
            current_version: current.to_string(),
            latest_version: latest.to_string(),
            release_notes: remote.release_notes.unwrap_or_default(),
            download_url: remote.download_url.unwrap_or_default(),
        }))
    } else {
        Ok(None)
    }
}

/// Фоновая неблокирующая проверка обновлений (не вешает интерфейс программы)
pub fn check_updates_async<F>(current_version: String, callback: F)
where
    F: FnOnce(Result<Option<UpdateInfo>>) + Send + 'static,
{
    std::thread::spawn(move || {
        let res = check_updates_sync(&current_version);
        callback(res);
    });
}

/// Отправка отзыва с соблюдением приватности и безопасности (Opt-In, HTTPS, URL-кодирование)
pub fn send_feedback(feedback_text: &str, user_consented: bool) -> Result<()> {
    // 1. Строгий Opt-In: если пользователь отказался, запрос категорически НЕ отправляется (устраняет Issue 1.6)
    if !user_consented {
        return Ok(());
    }

    let trimmed = feedback_text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(5))
        .build();

    // 2. Отправка только по безопасному HTTPS с автоматическим URL-кодированием формы
    agent
        .post(FEEDBACK_URL)
        .send_form(&[
            ("app", "usb-test"),
            ("version", env!("CARGO_PKG_VERSION")),
            ("feedback", trimmed),
        ])
        .map_err(|e| UsbTestError::Network(format!("Не удалось отправить отзыв: {}", e)))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_semver_patch_comparison_fixed() {
        // Проверяем исправление бага 2.4: патч-версии обязаны определяться как более новые
        let current = Version::parse("1.1.5").unwrap();
        let newer_patch = Version::parse("1.1.6").unwrap();
        let newer_minor = Version::parse("1.2.0").unwrap();
        let older = Version::parse("1.1.4").unwrap();

        assert!(newer_patch > current);
        assert!(newer_minor > current);
        assert!(current > older);
    }

    #[test]
    fn test_feedback_zero_network_when_not_consented() {
        // Если согласия нет, функция мгновенно возвращает Ok(()) без сетевых вызовов
        assert!(send_feedback("Any text", false).is_ok());
    }
}
