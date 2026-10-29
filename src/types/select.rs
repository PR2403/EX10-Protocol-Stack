//! Option 位域与 Tag Singulation（标签选择过滤）构建器（手册 §5.1.1/§5.1.2，p.33-37）。

use crate::error::{DecodeError, EncodeError};

/// Select-Option Bits（Option 字节低 3 位 + BIT3 反选 + BIT5 长度扩展，p.36）。
///
/// 与 p.37「值 → 域存在性」表一一对应：
///
/// | 值 | SelectAddress | SelectDataLength | SelectData | AccessPassword |
/// |---|---|---|---|---|
/// | 0x00 | 无 | 无 | 无 | 无 |
/// | 0x01 | 无 | 有 | 有 | 有 |
/// | 0x02/03/04 | 有 | 有 | 有 | 有 |
/// | 0x05 | 无 | 无 | 无 | 有 |
/// | 0x07 | 无 | 无 | 无 | 有 |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SelectMode {
    /// 0x00 不过滤
    None = 0x00,
    /// 0x01 过滤 EPC 值（无地址）
    EpcValue = 0x01,
    /// 0x02 过滤 TID bank
    Tid = 0x02,
    /// 0x03 过滤 USER bank
    User = 0x03,
    /// 0x04 过滤 EPC bank（含 PC）
    EpcBank = 0x04,
    /// 0x05 不过滤但携带密码（操作已锁定区域）
    LockedNoFilter = 0x05,
    /// 0x07 多标签匹配过滤（需先成功执行 0xAA4C；0x21 不支持）
    MultiTag = 0x07,
}

impl SelectMode {
    pub const fn from_bits(bits: u8) -> Option<Self> {
        Some(match bits & 0x07 {
            0x00 => SelectMode::None,
            0x01 => SelectMode::EpcValue,
            0x02 => SelectMode::Tid,
            0x03 => SelectMode::User,
            0x04 => SelectMode::EpcBank,
            0x05 => SelectMode::LockedNoFilter,
            0x07 => SelectMode::MultiTag,
            _ => return None,
        })
    }

    /// 是否携带 SelectAddress 域（p.37 表）。
    pub const fn has_address(self) -> bool {
        matches!(self, SelectMode::Tid | SelectMode::User | SelectMode::EpcBank)
    }

    /// 是否携带 SelectDataLength / SelectData 域。
    pub const fn has_data(self) -> bool {
        matches!(
            self,
            SelectMode::EpcValue | SelectMode::Tid | SelectMode::User | SelectMode::EpcBank
        )
    }

    /// 是否携带 AccessPassword 域（仅当指令 Data 字段定义此域时）。
    pub const fn has_password(self) -> bool {
        !matches!(self, SelectMode::None)
    }

    pub const fn bits(self) -> u8 {
        self as u8
    }
}

/// Tag Singulation（选择过滤）参数（手册 §5.1.2 p.34-37）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSingulation {
    /// 选择过滤模式（Option 低 3 位）。
    pub mode: SelectMode,
    /// BIT3 反选：返回不匹配规则的标签。
    pub invert: bool,
    /// BIT5：SelectDataLength 为 2 字节（选择数据 > 255 位时）。
    pub long_length: bool,
    /// Select Address：Bank 内偏移（位）。
    pub address: u32,
    /// Select Data Length：比较位数（bit）。
    pub data_len_bits: u16,
    /// Select Data：比较数据，`ceil(data_len_bits/8)` 字节，≤ 32 字节（256 位）。
    pub data: heapless::Vec<u8, 64>,
    /// AccessPassword（若指令 Data 定义此域）。
    pub access_password: u32,
}

impl Default for TagSingulation {
    fn default() -> Self {
        TagSingulation {
            mode: SelectMode::None,
            invert: false,
            long_length: false,
            address: 0,
            data_len_bits: 0,
            data: heapless::Vec::new(),
            access_password: 0,
        }
    }
}

impl TagSingulation {
    /// 构造 EPC 值过滤（0x01）：直接给完整 EPC 字节。
    pub fn epc_value(epc: &[u8]) -> Result<Self, EncodeError> {
        let mut s = TagSingulation {
            mode: SelectMode::EpcValue,
            data_len_bits: (epc.len() * 8) as u16,
            ..Default::default()
        };
        s.data
            .extend_from_slice(epc)
            .map_err(|_| EncodeError::InvalidParameter("select data too long"))?;
        Ok(s)
    }

    /// 构造 bank 过滤（0x02/0x03/0x04）。
    pub fn bank(mode: SelectMode, address_bits: u32, data_bits: u16, data: &[u8]) -> Self {
        TagSingulation {
            mode,
            address: address_bits,
            data_len_bits: data_bits,
            data: data.iter().copied().collect(),
            ..Default::default()
        }
    }

    /// 计算本过滤器对 Option 字节的贡献位（低 3 位 + BIT3 + BIT5）。
    pub fn option_bits(&self) -> u8 {
        let mut b = self.mode.bits();
        if self.invert {
            b |= 0x08;
        }
        if self.long_length {
            b |= 0x20;
        }
        b
    }

    /// 按 p.37 表把 Tag Singulation 域写入 `out`（不含 Option 字节本身）。
    ///
    /// `with_password`：指令 Data 字段是否定义 AccessPassword 域（0x21 无）。
    /// 域顺序与手册实例一致：AccessPassword → SelectAddress → SelectDataLength → SelectData。
    pub fn encode<const N: usize>(
        &self,
        out: &mut heapless::Vec<u8, N>,
        with_password: bool,
    ) -> Result<(), EncodeError> {
        let mode = self.mode;
        if mode == SelectMode::None {
            return Ok(());
        }
        if with_password && mode.has_password() {
            let _ = out.extend_from_slice(&self.access_password.to_be_bytes());
        }
        if mode.has_address() {
            let _ = out.extend_from_slice(&self.address.to_be_bytes());
        }
        if mode.has_data() {
            let need = (self.data_len_bits as usize).div_ceil(8);
            if need != self.data.len() {
                return Err(EncodeError::InvalidParameter(
                    "select data length mismatch with data_len_bits",
                ));
            }
            if self.long_length {
                let _ = out.extend_from_slice(&self.data_len_bits.to_be_bytes());
            } else {
                if self.data_len_bits > 255 {
                    return Err(EncodeError::InvalidParameter(
                        "select data >255 bits requires long_length",
                    ));
                }
                let _ = out.push(self.data_len_bits as u8);
            }
            let _ = out.extend_from_slice(&self.data);
        }
        Ok(())
    }

    /// 从指令 Data 的 Option 之后解析 Tag Singulation 域。
    ///
    /// 返回 `(TagSingulation, 消耗字节数)`。`with_password` 同 [`Self::encode`]。
    pub fn parse(
        data: &[u8],
        option_bits: u8,
        with_password: bool,
    ) -> Result<(Self, usize), DecodeError> {
        let mut s = TagSingulation {
            invert: option_bits & 0x08 != 0,
            long_length: option_bits & 0x20 != 0,
            ..Default::default()
        };
        let mode = SelectMode::from_bits(option_bits)
            .ok_or(DecodeError::BadValue("reserved select bits 0x06"))?;
        s.mode = mode;
        let mut pos = 0;
        let mut take = |n: usize| -> Result<&[u8], DecodeError> {
            if pos + n > data.len() {
                return Err(DecodeError::Truncated);
            }
            let sl = &data[pos..pos + n];
            pos += n;
            Ok(sl)
        };
        if mode != SelectMode::None && with_password && mode.has_password() {
            s.access_password = u32::from_be_bytes(take(4)?.try_into().unwrap());
        }
        if mode.has_address() {
            s.address = u32::from_be_bytes(take(4)?.try_into().unwrap());
        }
        if mode.has_data() {
            s.data_len_bits = if s.long_length {
                u16::from_be_bytes(take(2)?.try_into().unwrap())
            } else {
                take(1)?[0] as u16
            };
            let need = (s.data_len_bits as usize).div_ceil(8);
            let sl = take(need)?;
            s.data = sl.iter().copied().collect();
        }
        Ok((s, pos))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手册 p.35 EPC 前 3 位过滤示例：EpcBank + 1 bit @ 0x22
    #[test]
    fn epc_bank_1bit_example() {
        let sel = TagSingulation::bank(SelectMode::EpcBank, 0x22, 1, &[0x80]);
        assert_eq!(sel.option_bits(), 0x04);
        let mut out = heapless::Vec::<u8, 64>::new();
        sel.encode(&mut out, false).unwrap();
        assert_eq!(
            &out[..],
            &[0x00, 0x00, 0x00, 0x22, 0x01, 0x80]
        );
    }

    /// 手册 p.45 0x21 实例3：Option=0x11 → Select bits=0x01（EPC 值过滤），
    /// SelectDataLength=0x60(96bit) + 12 字节 EPC，0x21 无密码域。
    #[test]
    fn epc_value_no_password() {
        let epc = [0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66];
        let sel = TagSingulation::epc_value(&epc).unwrap();
        let mut out = heapless::Vec::<u8, 64>::new();
        sel.encode(&mut out, false).unwrap();
        let mut want = heapless::Vec::<u8, 64>::new();
        let _ = want.push(0x60);
        let _ = want.extend_from_slice(&epc);
        assert_eq!(&out[..], &want[..]);
    }

    #[test]
    fn roundtrip_with_password_and_address() {
        let sel = TagSingulation::bank(SelectMode::Tid, 0x20, 16, &[0xE2, 0x00]);
        let mut out = heapless::Vec::<u8, 64>::new();
        sel.encode(&mut out, true).unwrap();
        // pwd(4) + addr(4) + len(1) + data(2)
        assert_eq!(out.len(), 11);
        let (back, used) = TagSingulation::parse(&out, sel.option_bits(), true).unwrap();
        assert_eq!(used, 11);
        assert_eq!(back, sel);
    }

    #[test]
    fn none_encodes_nothing() {
        let sel = TagSingulation::default();
        let mut out = heapless::Vec::<u8, 64>::new();
        sel.encode(&mut out, true).unwrap();
        assert!(out.is_empty());
        assert_eq!(sel.option_bits(), 0x00);
    }
}
