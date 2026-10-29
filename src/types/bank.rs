//! Gen2 标签存储区（附录 4，p.176）。

/// 标签存储区 Bank（0x24/0x28 等指令的 MemBank 域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MemBank {
    /// 0x00 Reserved（Kill/Access 密码）
    Reserved = 0x00,
    /// 0x01 EPC（含 PC 与 TagCRC）
    Epc = 0x01,
    /// 0x02 TID
    Tid = 0x02,
    /// 0x03 USER
    User = 0x03,
}

impl MemBank {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x00 => Some(MemBank::Reserved),
            0x01 => Some(MemBank::Epc),
            0x02 => Some(MemBank::Tid),
            0x03 => Some(MemBank::User),
            _ => None,
        }
    }

    pub const fn value(self) -> u8 {
        self as u8
    }
}

impl core::fmt::Display for MemBank {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            MemBank::Reserved => "Reserved",
            MemBank::Epc => "EPC",
            MemBank::Tid => "TID",
            MemBank::User => "USER",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        for v in 0u8..4 {
            assert_eq!(MemBank::from_u8(v).unwrap().value(), v);
        }
        assert!(MemBank::from_u8(4).is_none());
    }
}
