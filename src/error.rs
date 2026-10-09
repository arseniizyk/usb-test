use thiserror::Error;

#[derive(Error, Debug)]
pub enum UsbTestError {
    #[error("Диск не найден: {0}")]
    DriveNotFound(String),

    #[error("КРИТИЧЕСКАЯ ЗАЩИТА: Попытка операции с системным или загрузочным диском ({0})! Операция заблокирована.")]
    SystemDriveProtected(String),

    #[error("Диск {0} не является съемным накопителем (DRIVE_REMOVABLE). Тестирование отклонено в целях безопасности.")]
    NonRemovableDrive(String),

    #[error("Отказано в доступе к диску {0}. Возможно, требуются права администратора.")]
    AccessDenied(String),

    #[error("Ошибка ввода-вывода (I/O): {0}")]
    Io(#[from] std::io::Error),

    #[error("Ошибка прямого доступа (Direct I/O): {0}")]
    DirectIoError(String),

    #[error("Ошибка выравнивания буфера памяти (Sector Alignment): {0}")]
    AlignmentError(String),

    #[error("Недопустимый размер сектора или блока: {0} байт")]
    InvalidBlockSize(usize),

    #[error("Несовпадение данных при верификации по смещению {offset}: ожидалось 0x{expected:02X}, получено 0x{found:02X}")]
    VerificationFailed {
        offset: u64,
        expected: u8,
        found: u8,
    },

    #[error("Ошибка IOCTL / геометрии диска: {0}")]
    IoctlError(String),

    #[error("Ошибка парсинга версии: {0}")]
    Semver(#[from] semver::Error),

    #[error("Ошибка сериализации/десериализации JSON: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Сетевая ошибка: {0}")]
    Network(String),

    #[error("Ошибка истории тестов: {0}")]
    HistoryError(String),

    #[error("Операция отменена пользователем")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, UsbTestError>;
