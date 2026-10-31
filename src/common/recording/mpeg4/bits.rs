//! 작은 패킷 버퍼로 시작해 필요할 때만 fallible 확장하고 최악 크기 상한을 지킨다.
use std::io;
pub(super) struct Bits {
    bytes: Vec<u8>,
    used: u8,
    limit: usize,
    overflow: bool,
    allocation_error: Option<std::collections::TryReserveError>,
}
impl Bits {
    pub fn new(limit: usize) -> io::Result<Self> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(limit.min(65_536))
            .map_err(io::Error::other)?;
        Ok(Self {
            bytes,
            used: 0,
            limit,
            overflow: false,
            allocation_error: None,
        })
    }
    pub fn clear(&mut self) {
        self.bytes.clear();
        self.used = 0;
        self.overflow = false;
        self.allocation_error = None;
    }
    pub fn put(&mut self, count: u8, value: u32) {
        if self.allocation_error.is_some() {
            return;
        }
        if count > 32 {
            self.overflow = true;
            return;
        }
        let mut remaining = count;
        while remaining != 0 {
            if self.used == 0 {
                if self.bytes.len() == self.limit {
                    self.overflow = true;
                    return;
                }
                if self.bytes.len() == self.bytes.capacity() {
                    let capacity = self
                        .bytes
                        .capacity()
                        .saturating_mul(2)
                        .max(1)
                        .min(self.limit);
                    if let Err(error) = self.bytes.try_reserve_exact(capacity - self.bytes.len()) {
                        self.allocation_error = Some(error);
                        return;
                    }
                }
                self.bytes.push(0);
            }
            let index = self.bytes.len() - 1;
            let take = remaining.min(8 - self.used);
            let shift = remaining - take;
            let mask = (1_u32 << take) - 1;
            self.bytes[index] |= (((value >> shift) & mask) as u8) << (8 - self.used - take);
            self.used = (self.used + take) % 8;
            remaining -= take;
        }
    }
    pub fn bit(&mut self, value: bool) {
        self.put(1, u32::from(value));
    }
    pub fn code(&mut self, code: (u16, u8)) {
        self.put(code.1, u32::from(code.0));
    }
    pub fn start(&mut self, code: u8) {
        self.put(24, 1);
        self.put(8, u32::from(code));
    }
    pub fn align(&mut self) {
        self.bit(false);
        while self.used != 0 {
            self.bit(true);
        }
    }
    pub fn bytes(&self) -> io::Result<&[u8]> {
        if let Some(error) = &self.allocation_error {
            return Err(io::Error::other(format!(
                "MPEG-4 packet allocation failed: {error}"
            )));
        }
        if self.overflow {
            Err(io::Error::other(
                "MPEG-4 packet exceeded its checked memory bound",
            ))
        } else {
            Ok(&self.bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn packet_capacity_grows_only_when_needed_and_stays_bounded() -> std::io::Result<()> {
        let mut bits = super::Bits::new(10 * 1024 * 1024)?;
        assert_eq!(bits.bytes.capacity(), 65536);
        for i in 0..70000 {
            bits.put(8, (i % 256) as u32);
        }
        assert!(bits.bytes.capacity() <= 131072);
        assert_eq!(bits.bytes()?.len(), 70000);
        for (i, &byte) in bits.bytes()?.iter().enumerate() {
            assert_eq!(byte, (i % 256) as u8);
        }
        bits.clear();
        assert!(bits.bytes()?.is_empty());
        Ok(())
    }
    #[test]
    fn packed_writes_preserve_cross_byte_order_and_overflow() -> std::io::Result<()> {
        for limit in [0, 1, 2, 7, 128] {
            let mut actual = super::Bits::new(limit)?;
            let mut expected = Vec::new();
            let mut used = 0_u8;
            let mut overflow = false;
            for count in [0, 1, 3, 8, 32, 7, 24, 2, 31] {
                let value = 0xa195_67dc;
                actual.put(count, value);
                for shift in (0..count).rev() {
                    if used == 0 {
                        if expected.len() == limit {
                            overflow = true;
                            break;
                        }
                        expected.push(0_u8);
                    }
                    let index = expected.len() - 1;
                    expected[index] |= (((value >> shift) & 1) as u8) << (7 - used);
                    used = (used + 1) % 8;
                }
                assert_eq!(actual.bytes, expected);
                assert_eq!(actual.used, used);
                assert_eq!(actual.bytes().is_err(), overflow);
            }
            actual.clear();
            assert!(actual.bytes()?.is_empty());
            actual.put(33, 0);
            assert!(actual.bytes().is_err());
        }
        Ok(())
    }
}
