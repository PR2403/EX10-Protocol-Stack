//! 波特率枚举（附录 2，p.170）。
//!
//! 波特率仅可经扩展指令 0xAA40 KEY=0x06 设置（0x06 普通指令已失效）；
//! KEYDATA 为下表 4 字节大端值。

/// 附录 2 波特率值表。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum BaudRate {
    Baud9600 = 0x0000_2580,
    Baud19200 = 0x0000_4B00,
    Baud38400 = 0x0000_9600,
    Baud57600 = 0x0000_E100,
    Baud115200 = 0x0001_C200,
    Baud230400 = 0x0003_8400,
    Baud460800 = 0x0007_0800,
    Baud921600 = 0x000E_1000,
}

impl BaudRate {
    /// 从 4 字节 KEYDATA 原始值解析。
    pub const fn from_u32(v: u32) -> Option<Self> {
        match v {
            0x0000_2580 => Some(BaudRate::Baud9600),
            0x0000_4B00 => Some(BaudRate::Baud19200),
            0x0000_9600 => Some(BaudRate::Baud38400),
            0x0000_E100 => Some(BaudRate::Baud57600),
            0x0001_C200 => Some(BaudRate::Baud115200),
            0x0003_8400 => Some(BaudRate::Baud230400),
            0x0007_0800 => Some(BaudRate::Baud460800),
            0x000E_1000 => Some(BaudRate::Baud921600),
            _ => None,
        }
    }

    /// 十进制波特率值（串口配置用）。
    pub const fn baud(self) -> u32 {
        self as u32
    }

    /// 4 字节大端 KEYDATA。
    pub const fn keydata(self) -> [u8; 4] {
        (self as u32).to_be_bytes()
    }
}

impl From<BaudRate> for u32 {
    fn from(b: BaudRate) -> u32 {
        b as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_appendix2() {
        assert_eq!(BaudRate::Baud115200.keydata(), [0x00, 0x01, 0xC2, 0x00]);
        assert_eq!(BaudRate::Baud921600.keydata(), [0x00, 0x0E, 0x10, 0x00]);
        // p.144 响应 KEYDATA = 00 0E 10 00 -> 921600
        assert_eq!(BaudRate::from_u32(0x000E_1000), Some(BaudRate::Baud921600));
        assert_eq!(BaudRate::from_u32(1234), None);
        assert_eq!(BaudRate::Baud115200.baud(), 115_200);
    }
}
