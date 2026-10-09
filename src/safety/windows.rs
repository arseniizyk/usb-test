use super::DriveScanner;
use crate::core::{BusType, DriveInfo};
use crate::error::{Result, UsbTestError};
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDriveStringsW,
    GetVolumeInformationW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Ioctl::{
    PropertyStandardQuery, StorageDeviceProperty, IOCTL_STORAGE_QUERY_PROPERTY,
    STORAGE_DEVICE_DESCRIPTOR, STORAGE_PROPERTY_QUERY,
};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::System::SystemInformation::GetSystemDirectoryW;

const WIN_DRIVE_REMOVABLE: u32 = 2;

#[derive(Default)]
pub struct WindowsDriveScanner;

impl WindowsDriveScanner {
    pub fn new() -> Self {
        Self
    }
}

impl DriveScanner for WindowsDriveScanner {
    fn scan_drives(&self) -> Result<Vec<DriveInfo>> {
        let system_letter = self.get_system_drive_letter();

        let mut buffer = vec![0u16; 512];
        let len = unsafe { GetLogicalDriveStringsW(Some(&mut buffer)) } as usize;
        if len == 0 {
            return Err(UsbTestError::Io(std::io::Error::last_os_error()));
        }

        if len > buffer.len() {
            buffer.resize(len + 1, 0);
            unsafe { GetLogicalDriveStringsW(Some(&mut buffer)) };
        }

        let mut drives = Vec::new();
        let mut start = 0;
        for i in 0..len {
            if buffer[i] == 0 {
                if start < i {
                    let slice = &buffer[start..i];
                    if let Some(drive_path) = parse_drive_path(slice) {
                        if let Some(info) = query_drive_info(&drive_path, system_letter) {
                            drives.push(info);
                        }
                    }
                }
                start = i + 1;
            }
        }

        Ok(drives)
    }

    fn get_system_drive_letter(&self) -> char {
        let mut sys_dir = vec![0u16; 260];
        let len = unsafe { GetSystemDirectoryW(Some(&mut sys_dir)) } as usize;
        if len > 0 && sys_dir[0] != 0 {
            let ch = char::from_u32(sys_dir[0] as u32).unwrap_or('C');
            ch.to_ascii_uppercase()
        } else {
            'C'
        }
    }
}

fn parse_drive_path(slice: &[u16]) -> Option<String> {
    let os_str = OsString::from_wide(slice);
    os_str.into_string().ok()
}

fn query_drive_info(root_path: &str, system_letter: char) -> Option<DriveInfo> {
    let letter = root_path.chars().next()?.to_ascii_uppercase();
    let is_system = letter == system_letter;

    let wide_path: Vec<u16> = root_path.encode_utf16().chain(std::iter::once(0)).collect();
    let drive_type = unsafe { GetDriveTypeW(PCWSTR(wide_path.as_ptr())) };
    let is_removable = drive_type == WIN_DRIVE_REMOVABLE;

    let mut free_bytes_available = 0u64;
    let mut total_bytes = 0u64;
    let mut total_free_bytes = 0u64;
    let _ = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(wide_path.as_ptr()),
            Some(&mut free_bytes_available),
            Some(&mut total_bytes),
            Some(&mut total_free_bytes),
        )
    };

    let mut volume_name_buf = vec![0u16; 260];
    let mut fs_name_buf = vec![0u16; 260];
    let mut serial_number = 0u32;
    let mut max_component_len = 0u32;
    let mut fs_flags = 0u32;

    let _ = unsafe {
        GetVolumeInformationW(
            PCWSTR(wide_path.as_ptr()),
            Some(&mut volume_name_buf),
            Some(&mut serial_number),
            Some(&mut max_component_len),
            Some(&mut fs_flags),
            Some(&mut fs_name_buf),
        )
    };

    let volume_name = String::from_utf16_lossy(&volume_name_buf)
        .trim_matches('\0')
        .to_string();
    let file_system = String::from_utf16_lossy(&fs_name_buf)
        .trim_matches('\0')
        .to_string();

    let (bus_type, model) = query_storage_properties(letter);

    Some(DriveInfo {
        letter,
        root_path: root_path.to_string(),
        volume_name: if volume_name.is_empty() { "Съемный диск".to_string() } else { volume_name },
        file_system: if file_system.is_empty() { "Неизвестно".to_string() } else { file_system },
        total_bytes,
        free_bytes: total_free_bytes,
        sector_size: 512,
        is_removable,
        is_system,
        bus_type,
        model,
    })
}

fn query_storage_properties(drive_letter: char) -> (BusType, String) {
    let device_path = format!("\\\\.\\{}:", drive_letter);
    let wide_device: Vec<u16> = device_path.encode_utf16().chain(std::iter::once(0)).collect();

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide_device.as_ptr()),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
            HANDLE(std::ptr::null_mut()),
        )
    };

    if handle.is_err() || handle.as_ref().map(|h| h.is_invalid()).unwrap_or(true) {
        return (BusType::Unknown, "Generic Storage Device".to_string());
    }

    let handle = handle.unwrap();

    let query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: PropertyStandardQuery,
        AdditionalParameters: [0],
    };

    let mut out_buffer = vec![0u8; 1024];
    let mut bytes_returned = 0u32;

    let ok = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_STORAGE_QUERY_PROPERTY,
            Some(&query as *const _ as *const _),
            std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
            Some(out_buffer.as_mut_ptr() as *mut _),
            out_buffer.len() as u32,
            Some(&mut bytes_returned),
            None,
        )
    };

    let _ = unsafe { CloseHandle(handle) };

    if ok.is_err() || bytes_returned < std::mem::size_of::<STORAGE_DEVICE_DESCRIPTOR>() as u32 {
        return (BusType::Unknown, "Generic Storage Device".to_string());
    }

    let desc = unsafe { &*(out_buffer.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };

    // STORAGE_BUS_TYPE:
    // 0 = Unknown, 1 = Scsi, 3 = Ata, 7 = Usb, 11 = Sata, 12 = Sd, 17 = Nvme
    let bus_type = match desc.BusType.0 {
        1 => BusType::Scsi,
        7 => BusType::Usb,
        11 => BusType::Sata,
        12 => BusType::Sd,
        17 => BusType::Nvme,
        _ => BusType::Unknown,
    };

    let mut model = String::new();
    if desc.ProductIdOffset > 0 && (desc.ProductIdOffset as usize) < out_buffer.len() {
        let raw = &out_buffer[desc.ProductIdOffset as usize..];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        model = String::from_utf8_lossy(&raw[..end]).trim().to_string();
    }

    if model.is_empty() {
        model = "Removable Drive".to_string();
    }

    (bus_type, model)
}
