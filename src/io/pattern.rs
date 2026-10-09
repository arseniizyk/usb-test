use crc32fast::Hasher;

const MAGIC: [u8; 4] = *b"USBT";
pub const HEADER_SIZE: usize = 32;

/// Заполняет выровненный буфер уникальным паттерном блока с солью и контрольной суммой
pub fn fill_block_pattern(buf: &mut [u8], block_index: u64, seed: u64, timestamp: u64) {
    let len = buf.len();
    assert!(len >= HEADER_SIZE, "Размер блока меньше размера заголовка");

    // 1. Формируем заголовок (28 байт данных + 4 байта CRC32)
    buf[0..4].copy_from_slice(&MAGIC);
    buf[4..12].copy_from_slice(&block_index.to_le_bytes());
    buf[12..20].copy_from_slice(&seed.to_le_bytes());
    buf[20..28].copy_from_slice(&timestamp.to_le_bytes());

    let mut hasher = Hasher::new();
    hasher.update(&buf[0..28]);
    let header_crc = hasher.finalize();
    buf[28..32].copy_from_slice(&header_crc.to_le_bytes());

    // 2. Заполняем тело детерминированным быстрым генератором XorShift64
    let mut state = seed ^ block_index.wrapping_mul(0x9E3779B97F4A7C15);
    if state == 0 {
        state = 0x853c49e6748fea9b;
    }

    let mut offset = HEADER_SIZE;
    while offset + 8 <= len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        buf[offset..offset + 8].copy_from_slice(&state.to_le_bytes());
        offset += 8;
    }

    // Остаток байт (если блок не кратен 8)
    if offset < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_le_bytes();
        let rem = len - offset;
        buf[offset..len].copy_from_slice(&bytes[..rem]);
    }
}

#[derive(Debug, PartialEq)]
pub enum VerificationResult {
    Ok,
    AllZeroes,
    AllOnes,
    MagicMismatch,
    AddressWrapAround { expected: u64, found: u64 },
    HeaderCrcMismatch,
    DataCorrupted { offset: usize },
}

/// Проверяет целостность прочитанного блока и детектирует поддельные накопители
pub fn verify_block_pattern(
    buf: &[u8],
    expected_block_index: u64,
    expected_seed: u64,
) -> VerificationResult {
    let len = buf.len();
    if len < HEADER_SIZE {
        return VerificationResult::DataCorrupted { offset: 0 };
    }

    // 1. Проверка на все нули (сброс записи контроллером)
    if buf.iter().take(256).all(|&b| b == 0) && buf.iter().all(|&b| b == 0) {
        return VerificationResult::AllZeroes;
    }

    // 2. Проверка на все 0xFF (стертая память)
    if buf.iter().take(256).all(|&b| b == 0xFF) && buf.iter().all(|&b| b == 0xFF) {
        return VerificationResult::AllOnes;
    }

    // 3. Проверка magic заголовка
    if buf[0..4] != MAGIC {
        return VerificationResult::MagicMismatch;
    }

    // 4. Проверка индекса блока (выявляет циклический wrap-around поддельной флешки)
    let found_index = u64::from_le_bytes(buf[4..12].try_into().unwrap());
    if found_index != expected_block_index {
        return VerificationResult::AddressWrapAround {
            expected: expected_block_index,
            found: found_index,
        };
    }

    // 5. Проверка CRC заголовка
    let mut hasher = Hasher::new();
    hasher.update(&buf[0..28]);
    let expected_header_crc = hasher.finalize();
    let found_header_crc = u32::from_le_bytes(buf[28..32].try_into().unwrap());

    if expected_header_crc != found_header_crc {
        return VerificationResult::HeaderCrcMismatch;
    }

    // 6. Проверка тела данных
    let mut state = expected_seed ^ expected_block_index.wrapping_mul(0x9E3779B97F4A7C15);
    if state == 0 {
        state = 0x853c49e6748fea9b;
    }

    let mut offset = HEADER_SIZE;
    while offset + 8 <= len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let expected_bytes = state.to_le_bytes();
        if buf[offset..offset + 8] != expected_bytes {
            return VerificationResult::DataCorrupted { offset };
        }
        offset += 8;
    }

    if offset < len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let expected_bytes = state.to_le_bytes();
        let rem = len - offset;
        if buf[offset..len] != expected_bytes[..rem] {
            return VerificationResult::DataCorrupted { offset };
        }
    }

    VerificationResult::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_generation_and_verification() {
        let mut buf = vec![0u8; 64 * 1024];
        let seed = 123456789;
        let block_idx = 42;
        let ts = 1000;

        fill_block_pattern(&mut buf, block_idx, seed, ts);
        assert_eq!(verify_block_pattern(&buf, block_idx, seed), VerificationResult::Ok);
    }

    #[test]
    fn test_pattern_detects_wrap_around() {
        let mut buf = vec![0u8; 64 * 1024];
        let seed = 123456789;
        // Записали блок 100
        fill_block_pattern(&mut buf, 100, seed, 1000);

        // Ожидаем блок 5
        let res = verify_block_pattern(&buf, 5, seed);
        assert_eq!(
            res,
            VerificationResult::AddressWrapAround {
                expected: 5,
                found: 100
            }
        );
    }

    #[test]
    fn test_pattern_detects_zeroes_and_corruption() {
        let zero_buf = vec![0u8; 1024];
        assert_eq!(
            verify_block_pattern(&zero_buf, 0, 123),
            VerificationResult::AllZeroes
        );

        let mut buf = vec![0u8; 1024];
        fill_block_pattern(&mut buf, 1, 123, 100);
        buf[200] ^= 0x01; // Внесли ошибку
        match verify_block_pattern(&buf, 1, 123) {
            VerificationResult::DataCorrupted { offset } => assert_eq!(offset, 200),
            other => panic!("Expected DataCorrupted, got {:?}", other),
        }
    }
}
