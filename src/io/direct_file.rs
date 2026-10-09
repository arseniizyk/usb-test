use crate::error::Result;
use crate::io::aligned_buffer::AlignedBuffer;
use std::path::Path;

#[cfg(windows)]
mod platform {
    use super::*;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, HANDLE};
    use crate::error::UsbTestError;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FlushFileBuffers, ReadFile, SetFilePointerEx, WriteFile,
        CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, FILE_FLAG_NO_BUFFERING,
        FILE_FLAG_WRITE_THROUGH, FILE_SHARE_READ, FILE_SHARE_WRITE,
        FILE_BEGIN, OPEN_EXISTING,
    };

    pub struct PlatformDirectFile {
        handle: HANDLE,
    }

    unsafe impl Send for PlatformDirectFile {}
    unsafe impl Sync for PlatformDirectFile {}

    impl PlatformDirectFile {
        pub fn open_for_write(path: &Path) -> Result<Self> {
            let wide_path: Vec<u16> = path.as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH;

            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide_path.as_ptr()),
                    GENERIC_READ.0 | GENERIC_WRITE.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    CREATE_ALWAYS,
                    flags,
                    HANDLE(std::ptr::null_mut()),
                )
            };

            if handle.is_err() || handle.as_ref().map(|h| h.is_invalid()).unwrap_or(true) {
                return Err(UsbTestError::DirectIoError(format!(
                    "Не удалось открыть файл для прямой записи (Direct I/O): {:?}",
                    std::io::Error::last_os_error()
                )));
            }

            Ok(Self { handle: handle.unwrap() })
        }

        pub fn open_for_read(path: &Path) -> Result<Self> {
            let wide_path: Vec<u16> = path.as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();

            let flags = FILE_ATTRIBUTE_NORMAL | FILE_FLAG_NO_BUFFERING;

            let handle = unsafe {
                CreateFileW(
                    PCWSTR(wide_path.as_ptr()),
                    GENERIC_READ.0,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    None,
                    OPEN_EXISTING,
                    flags,
                    HANDLE(std::ptr::null_mut()),
                )
            };

            if handle.is_err() || handle.as_ref().map(|h| h.is_invalid()).unwrap_or(true) {
                return Err(UsbTestError::DirectIoError(format!(
                    "Не удалось открыть файл для прямого чтения (Direct I/O): {:?}",
                    std::io::Error::last_os_error()
                )));
            }

            Ok(Self { handle: handle.unwrap() })
        }

        pub fn write_block(&mut self, offset: u64, buf: &AlignedBuffer) -> Result<usize> {
            let mut new_pos = 0i64;
            let seek_ok = unsafe {
                SetFilePointerEx(self.handle, offset as i64, Some(&mut new_pos), FILE_BEGIN)
            };
            if seek_ok.is_err() {
                return Err(UsbTestError::DirectIoError("Ошибка позиционирования (SetFilePointerEx)".into()));
            }

            let mut written = 0u32;
            let write_ok = unsafe {
                WriteFile(
                    self.handle,
                    Some(buf.as_slice()),
                    Some(&mut written),
                    None,
                )
            };

            if write_ok.is_err() {
                return Err(UsbTestError::DirectIoError(format!(
                    "Ошибка записи Direct I/O: {:?}",
                    std::io::Error::last_os_error()
                )));
            }

            Ok(written as usize)
        }

        pub fn read_block(&mut self, offset: u64, buf: &mut AlignedBuffer) -> Result<usize> {
            let mut new_pos = 0i64;
            let seek_ok = unsafe {
                SetFilePointerEx(self.handle, offset as i64, Some(&mut new_pos), FILE_BEGIN)
            };
            if seek_ok.is_err() {
                return Err(UsbTestError::DirectIoError("Ошибка позиционирования (SetFilePointerEx)".into()));
            }

            let mut bytes_read = 0u32;
            let read_ok = unsafe {
                ReadFile(
                    self.handle,
                    Some(buf.as_mut_slice()),
                    Some(&mut bytes_read),
                    None,
                )
            };

            if read_ok.is_err() {
                return Err(UsbTestError::DirectIoError(format!(
                    "Ошибка чтения Direct I/O: {:?}",
                    std::io::Error::last_os_error()
                )));
            }

            Ok(bytes_read as usize)
        }

        pub fn sync(&mut self) -> Result<()> {
            let ok = unsafe { FlushFileBuffers(self.handle) };
            if ok.is_err() {
                return Err(UsbTestError::DirectIoError("Ошибка сброса буферов (FlushFileBuffers)".into()));
            }
            Ok(())
        }
    }

    impl Drop for PlatformDirectFile {
        fn drop(&mut self) {
            if !self.handle.is_invalid() {
                let _ = unsafe { CloseHandle(self.handle) };
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;
    use std::fs::{File, OpenOptions};
    use std::io::{Read, Seek, SeekFrom, Write};

    pub struct PlatformDirectFile {
        file: File,
    }

    impl PlatformDirectFile {
        pub fn open_for_write(path: &Path) -> Result<Self> {
            let file = OpenOptions::new()
                .create(true)
                .truncate(true)
                .read(true)
                .write(true)
                .open(path)?;

            // На macOS отключаем unified buffer cache через F_NOCACHE
            #[cfg(target_os = "macos")]
            unsafe {
                use std::os::unix::io::AsRawFd;
                libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1);
            }

            Ok(Self { file })
        }

        pub fn open_for_read(path: &Path) -> Result<Self> {
            let file = OpenOptions::new().read(true).open(path)?;

            #[cfg(target_os = "macos")]
            unsafe {
                use std::os::unix::io::AsRawFd;
                libc::fcntl(file.as_raw_fd(), libc::F_NOCACHE, 1);
            }

            Ok(Self { file })
        }

        pub fn write_block(&mut self, offset: u64, buf: &AlignedBuffer) -> Result<usize> {
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.write_all(buf.as_slice())?;
            Ok(buf.len())
        }

        pub fn read_block(&mut self, offset: u64, buf: &mut AlignedBuffer) -> Result<usize> {
            self.file.seek(SeekFrom::Start(offset))?;
            self.file.read_exact(buf.as_mut_slice())?;
            Ok(buf.len())
        }

        pub fn sync(&mut self) -> Result<()> {
            self.file.sync_all()?;
            Ok(())
        }
    }
}

pub struct DirectFile {
    inner: platform::PlatformDirectFile,
}

impl DirectFile {
    pub fn open_for_write(path: &Path) -> Result<Self> {
        Ok(Self {
            inner: platform::PlatformDirectFile::open_for_write(path)?,
        })
    }

    pub fn open_for_read(path: &Path) -> Result<Self> {
        Ok(Self {
            inner: platform::PlatformDirectFile::open_for_read(path)?,
        })
    }

    pub fn write_block(&mut self, offset: u64, buf: &AlignedBuffer) -> Result<usize> {
        self.inner.write_block(offset, buf)
    }

    pub fn read_block(&mut self, offset: u64, buf: &mut AlignedBuffer) -> Result<usize> {
        self.inner.read_block(offset, buf)
    }

    pub fn sync(&mut self) -> Result<()> {
        self.inner.sync()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_direct_file_write_and_read() {
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path();

        let size = 64 * 1024;
        let mut write_buf = AlignedBuffer::new(size);
        for (i, b) in write_buf.iter_mut().enumerate() {
            *b = (i % 256) as u8;
        }

        // 1. Запись
        {
            let mut df = DirectFile::open_for_write(path).unwrap();
            let written = df.write_block(0, &write_buf).unwrap();
            assert_eq!(written, size);
            df.sync().unwrap();
        } // Здесь файл гарантированно закрывается!

        // 2. Чтение (в отдельном блоке, проверка отсутствия sharing violation)
        {
            let mut df = DirectFile::open_for_read(path).unwrap();
            let mut read_buf = AlignedBuffer::new(size);
            let read = df.read_block(0, &mut read_buf).unwrap();
            assert_eq!(read, size);
            assert_eq!(&read_buf[..], &write_buf[..]);
        }
    }
}
