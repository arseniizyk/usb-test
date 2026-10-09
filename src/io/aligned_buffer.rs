use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ops::{Deref, DerefMut};

pub const SECTOR_ALIGNMENT: usize = 4096;

/// Буфер памяти, выровненный по границе 4096 байт для прямого ввода-вывода (Direct I/O / NO_BUFFERING)
pub struct AlignedBuffer {
    ptr: *mut u8,
    layout: Layout,
    len: usize,
}

unsafe impl Send for AlignedBuffer {}
unsafe impl Sync for AlignedBuffer {}

impl AlignedBuffer {
    pub fn new(size: usize) -> Self {
        assert!(size > 0, "Размер буфера должен быть больше нуля");
        assert!(
            size.is_multiple_of(SECTOR_ALIGNMENT),
            "Размер буфера ({}) должен быть кратен {} байт",
            size,
            SECTOR_ALIGNMENT
        );

        let layout = Layout::from_size_align(size, SECTOR_ALIGNMENT)
            .expect("Не удалось создать layout для AlignedBuffer");

        let ptr = unsafe { alloc_zeroed(layout) };
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }

        Self {
            ptr,
            layout,
            len: size,
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl Drop for AlignedBuffer {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                dealloc(self.ptr, self.layout);
            }
        }
    }
}

impl Deref for AlignedBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl DerefMut for AlignedBuffer {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aligned_buffer_allocation_and_access() {
        let size = 64 * 1024;
        let mut buf = AlignedBuffer::new(size);
        assert_eq!(buf.len(), size);
        assert_eq!(buf[0], 0);

        // Проверяем выравнивание указателя
        let addr = buf.as_slice().as_ptr() as usize;
        assert_eq!(addr % SECTOR_ALIGNMENT, 0);

        // Проверяем запись и чтение
        buf[0] = 0xAA;
        buf[size - 1] = 0xBB;
        assert_eq!(buf[0], 0xAA);
        assert_eq!(buf[size - 1], 0xBB);
    }
}
