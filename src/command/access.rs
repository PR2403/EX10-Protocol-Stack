//! 标签访问指令：0x23 写EPC / 0x24 写 / 0x25 锁 / 0x26 杀 / 0x28 读 / 0x2D 块写
//! （手册第 6 章，p.70-95）。

use crate::error::{DecodeError, EncodeError};
use crate::frame::RawFrame;
use crate::types::bank::MemBank;
use crate::types::metadata::{parse_metadata, Metadata, MetadataFlags, PhaseMode};
use crate::types::select::TagSingulation;

/// WriteReadFlag（p.71/77）：启用"写入后读取"。
pub const WRITE_READ_FLAG_EPC: u8 = 0x8C; // 0x23
pub const WRITE_READ_FLAG_DATA: u8 = 0x84; // 0x24

/// 0x23 写标签 EPC 请求（p.71-72）。
///
/// Data 序 = Timeout(2) + [WriteReadFlag(1)=0x8C] + Option(1) + [RFU(1)]
/// + [AccessPwd(4)] + TagSingulation + TagEpcID + [Read 域]。
///
/// RFU 仅在"无 WriteReadFlag 且 Option=0x00"时存在；Select=0x00 时无 AccessPwd。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteEpcRequest<'a> {
    pub timeout_ms: u16,
    /// Some=写入后读回（0x8C）。
    pub read_after_write: Option<ReadAfterWrite>,
    pub singulation: TagSingulation,
    /// 新 EPC（≤496 位 = 62 字节；模块自动维护 PC 长度位）。
    pub epc: &'a [u8],
}

/// 写入后读取的参数（0x23/0x24 共用，p.72/77）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadAfterWrite {
    pub mem_bank: MemBank,
    /// 读取起始地址（块地址）。
    pub address: u32,
    /// 读取字数。
    pub word_count: u8,
}

impl ReadAfterWrite {
    const fn flag(&self, opcode: u8) -> u8 {
        if opcode == 0x23 {
            WRITE_READ_FLAG_EPC
        } else {
            WRITE_READ_FLAG_DATA
        }
    }
}

impl WriteEpcRequest<'_> {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.epc.len() > 62 || !self.epc.len().is_multiple_of(2) {
            return Err(EncodeError::InvalidParameter("epc must be even bytes ≤62"));
        }
        let _ = out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        let with_pwd = self.singulation.mode != crate::types::select::SelectMode::None;
        if let Some(raw) = &self.read_after_write {
            let _ = out.push(raw.flag(0x23));
        }
        let _ = out.push(self.singulation.option_bits());
        if self.read_after_write.is_none() && !with_pwd {
            // RFU：仅当无 WriteReadFlag 且 Option=0x00 时存在
            let _ = out.push(0x00);
        }
        self.singulation.encode(out, true)?;
        let _ = out.extend_from_slice(self.epc);
        if let Some(raw) = &self.read_after_write {
            let _ = out.push(raw.mem_bank.value());
            let _ = out.extend_from_slice(&raw.address.to_be_bytes());
            let _ = out.push(raw.word_count);
        }
        Ok(())
    }
}

/// 0x23 嵌入读响应（p.72）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteReadResponse<'a> {
    /// 回显 WriteReadFlag（0x8C / 0x84）。
    pub write_read_flag: u8,
    pub option: u8,
    pub read_data: &'a [u8],
}

/// 0x24 写标签请求（p.76-77）。
///
/// Data 序 = Timeout(2) + [WriteReadFlag(1)=0x84] + Option(1) + WriteAddress(4)
/// + WriteMemBank(1) + [AccessPwd(4)] + TagSingulation + WriteData + [Read 域]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTagRequest<'a> {
    pub timeout_ms: u16,
    pub read_after_write: Option<ReadAfterWrite>,
    pub singulation: TagSingulation,
    /// 写入起始块地址。
    pub address: u32,
    pub mem_bank: MemBank,
    /// 写入数据（偶数字节，一次 ≤64 字节 = 32 字）。
    pub data: &'a [u8],
}

impl WriteTagRequest<'_> {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.data.is_empty() || !self.data.len().is_multiple_of(2) || self.data.len() > 64 {
            return Err(EncodeError::InvalidParameter(
                "write data must be even bytes 2..=64",
            ));
        }
        let _ = out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        if let Some(raw) = &self.read_after_write {
            let _ = out.push(raw.flag(0x24));
        }
        let _ = out.push(self.singulation.option_bits());
        let _ = out.extend_from_slice(&self.address.to_be_bytes());
        let _ = out.push(self.mem_bank.value());
        self.singulation.encode(out, true)?;
        let _ = out.extend_from_slice(self.data);
        if let Some(raw) = &self.read_after_write {
            let _ = out.push(raw.mem_bank.value());
            let _ = out.extend_from_slice(&raw.address.to_be_bytes());
            let _ = out.push(raw.word_count);
        }
        Ok(())
    }
}

/// 0x25 锁标签请求（p.83-84）。
///
/// Mask/Action 各 2 字节低 10 位有效：每 bank 2 位 [R/W, Perm]，
/// 位序 = KillPwd(bit0-1)、AccessPwd(2-3)、EPC(4-5)、TID(6-7)、USER(8-9)。
/// Action 位：R/W=1 锁定；Perm=1 永久锁。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockTagRequest {
    pub timeout_ms: u16,
    pub singulation: TagSingulation,
    /// AccessPassword（锁操作不得为 0，手册易错点 ⑨）。
    pub access_password: u32,
    pub mask_bits: u16,
    pub action_bits: u16,
}

impl LockTagRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.access_password == 0 {
            return Err(EncodeError::InvalidParameter(
                "lock requires non-zero access password",
            ));
        }
        if self.mask_bits & !0x03FF != 0 || self.action_bits & !0x03FF != 0 {
            return Err(EncodeError::InvalidParameter("mask/action low 10 bits only"));
        }
        let _ = out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        let _ = out.push(self.singulation.option_bits());
        let _ = out.extend_from_slice(&self.access_password.to_be_bytes());
        let _ = out.extend_from_slice(&self.mask_bits.to_be_bytes());
        let _ = out.extend_from_slice(&self.action_bits.to_be_bytes());
        self.singulation.encode(out, false)
    }
}

/// 0x26 销毁标签请求（p.86-87）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KillTagRequest {
    pub timeout_ms: u16,
    pub singulation: TagSingulation,
    /// 销毁密码（不得为 0）。
    pub kill_password: u32,
}

impl KillTagRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.kill_password == 0 {
            return Err(EncodeError::InvalidParameter(
                "kill requires non-zero kill password",
            ));
        }
        let _ = out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        let _ = out.push(self.singulation.option_bits());
        let _ = out.extend_from_slice(&self.kill_password.to_be_bytes());
        let _ = out.push(0x00); // RFU
        self.singulation.encode(out, false)
    }
}

/// 0x28 读标签存储区请求（p.88-89）。
///
/// Data 序 = Timeout(2) + Option(1) + [MetadataFlags(2)] + MemBank(1) + Address(4)
/// + WordCount(1) + [AccessPwd(4)] + TagSingulation。BIT4=1 时返回元数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadTagRequest<'a> {
    pub timeout_ms: u16,
    /// Some=返回元数据（Option BIT4=1）。
    pub metadata_flags: Option<MetadataFlags>,
    pub mem_bank: MemBank,
    pub address: u32,
    /// 读取字数（≤96）。
    pub word_count: u8,
    pub singulation: TagSingulation,
    _marker: core::marker::PhantomData<&'a ()>,
}

impl<'a> ReadTagRequest<'a> {
    pub fn new(
        timeout_ms: u16,
        metadata_flags: Option<MetadataFlags>,
        mem_bank: MemBank,
        address: u32,
        word_count: u8,
        singulation: TagSingulation,
    ) -> Self {
        ReadTagRequest {
            timeout_ms,
            metadata_flags,
            mem_bank,
            address,
            word_count,
            singulation,
            _marker: core::marker::PhantomData,
        }
    }

    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.word_count == 0 || self.word_count > 96 {
            return Err(EncodeError::InvalidParameter("read 1..=96 words"));
        }
        let _ = out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        let mut option = self.singulation.option_bits();
        if self.metadata_flags.is_some() {
            option |= 0x10;
        }
        let _ = out.push(option);
        if let Some(flags) = self.metadata_flags {
            let _ = out.extend_from_slice(&flags.bits().to_be_bytes());
        }
        let _ = out.push(self.mem_bank.value());
        let _ = out.extend_from_slice(&self.address.to_be_bytes());
        let _ = out.push(self.word_count);
        self.singulation.encode(out, true)
    }
}

/// 0x28 响应（两种形态，p.89-90）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadTagResponse<'a> {
    pub option: u8,
    pub metadata_flags: Option<MetadataFlags>,
    pub metadata: Option<Metadata<'a>>,
    pub read_data: &'a [u8],
}

/// 0x2D 块写请求（p.93-94）。
///
/// Data 序 = Timeout(2) + ChipType(1)=0x00 + Option(1, BIT6=1) + SubCmd(2)=0x00C7
/// + [AccessPwd(4)] + TagSingulation + WriteFlags(1)=0x00 + MemBank(1) + Address(4)
/// + WordCount(1) + Data(2N)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockWriteRequest<'a> {
    pub timeout_ms: u16,
    pub singulation: TagSingulation,
    pub mem_bank: MemBank,
    pub address: u32,
    /// 写入字数（≤48）。
    pub word_count: u8,
    pub data: &'a [u8],
}

impl BlockWriteRequest<'_> {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.word_count == 0 || self.word_count > 48 {
            return Err(EncodeError::InvalidParameter("block write 1..=48 words"));
        }
        if self.data.len() != self.word_count as usize * 2 {
            return Err(EncodeError::InvalidParameter("data length != word_count*2"));
        }
        let _ = out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        let _ = out.push(0x00); // Chip Type
        // Option：Select bits + BIT6=1（0x2D 硬性要求）
        let _ = out.push(self.singulation.option_bits() | 0x40);
        let _ = out.extend_from_slice(&0x00C7u16.to_be_bytes()); // Sub Command write
        // AccessPwd 域存在性由 Select bits 决定（BIT6=1 即 Select=0 时无密码）
        self.singulation.encode(out, true)?;
        let _ = out.push(0x00); // Write Flags
        let _ = out.push(self.mem_bank.value());
        let _ = out.extend_from_slice(&self.address.to_be_bytes());
        let _ = out.push(self.word_count);
        let _ = out.extend_from_slice(self.data);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 响应解析
// ---------------------------------------------------------------------------

/// 0x23/0x24 写入后读取响应解析（p.72/77）。
pub fn parse_write_read<'a>(
    frame: &'a RawFrame<'a>,
) -> Result<WriteReadResponse<'a>, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() < 2 {
        return Err(DecodeError::Truncated);
    }
    Ok(WriteReadResponse {
        write_read_flag: data[0],
        option: data[1],
        read_data: &data[2..],
    })
}

/// 0x28 响应解析（Option BIT4 决定是否含元数据）。
pub fn parse_read_tag<'a>(frame: &'a RawFrame<'a>) -> Result<ReadTagResponse<'a>, DecodeError> {
    let data = frame.ext_payload()?;
    if data.is_empty() {
        return Err(DecodeError::Truncated);
    }
    let option = data[0];
    let mut pos = 1;
    let (flags, metadata) = if data.len() >= 3 && option & 0x10 != 0 {
        let flags = MetadataFlags(u16::from_be_bytes([data[1], data[2]]));
        pos = 3;
        let (md, used) = parse_metadata(&data[pos..], flags, PhaseMode::StartStop)?;
        pos += used;
        (Some(flags), Some(md))
    } else {
        (None, None)
    };
    Ok(ReadTagResponse {
        option,
        metadata_flags: flags,
        metadata,
        read_data: &data[pos..],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::builder::build_request;
    use crate::types::select::SelectMode;

    fn frame_bytes(opcode: u8, data: &[u8]) -> heapless::Vec<u8, 256> {
        let mut buf = crate::frame::FrameBuf::new();
        build_request(opcode, data, &mut buf).unwrap();
        buf
    }

    fn hex(buf: &[u8]) -> String {
        buf.iter().map(|b| format!("{b:02X} ")).collect()
    }

    /// 手册 p.73 例1：FF 0C 23 03 E8 00 00 1111222233334444 632C
    #[test]
    fn write_epc_golden() {
        let req = WriteEpcRequest {
            timeout_ms: 1000,
            read_after_write: None,
            singulation: TagSingulation::default(),
            epc: &[0x11, 0x22, 0x33, 0x33, 0x44, 0x44][..4],
        };
        // 用 8 字节 EPC 与手册一致
        let req = WriteEpcRequest {
            epc: &[0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44],
            ..req
        };
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d).unwrap();
        let buf = frame_bytes(0x23, &d);
        assert_eq!(
            hex(&buf),
            "FF 0C 23 03 E8 00 00 11 11 22 22 33 33 44 44 63 2C "
        );
    }

    /// 手册 p.73 例3：Select=0x04（EPC bank 过滤）+ 密码。
    /// FF 19 23 03 E8 04 00000000 00000020 08 11 111122223333444455556666 98 48
    #[test]
    fn write_epc_select_golden() {
        let req = WriteEpcRequest {
            timeout_ms: 1000,
            read_after_write: None,
            singulation: TagSingulation::bank(SelectMode::EpcBank, 0x20, 8, &[0x11]),
            epc: &[0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66],
        };
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d).unwrap();
        let buf = frame_bytes(0x23, &d);
        assert_eq!(
            hex(&buf),
            "FF 19 23 03 E8 04 00 00 00 00 00 00 00 20 08 11 11 11 22 22 33 33 44 44 55 55 \
             66 66 98 48 "
        );
    }

    /// 手册 p.78 例2：FF 10 24 03 E8 00 00000001 03 AAAABBBBCCCCDDDD C7 B3
    #[test]
    fn write_tag_golden() {
        let req = WriteTagRequest {
            timeout_ms: 1000,
            read_after_write: None,
            singulation: TagSingulation::default(),
            address: 1,
            mem_bank: MemBank::User,
            data: &[0xAA, 0xAA, 0xBB, 0xBB, 0xCC, 0xCC, 0xDD, 0xDD],
        };
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d).unwrap();
        let buf = frame_bytes(0x24, &d);
        assert_eq!(
            hex(&buf),
            "FF 10 24 03 E8 00 00 00 00 01 03 AA AA BB BB CC CC DD DD C7 B3 "
        );
    }

    /// 手册 p.85 例1：EPC 值过滤锁定 EPC bank（Perm 位）。
    /// FF 18 25 03E8 01 11223344 0020 0020 60 EPC12 9E7A
    #[test]
    fn lock_tag_golden() {
        let epc = [0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66];
        let req = LockTagRequest {
            timeout_ms: 1000,
            singulation: TagSingulation {
                mode: SelectMode::EpcValue,
                data: epc.iter().copied().collect(),
                data_len_bits: 96,
                access_password: 0x1122_3344,
                ..Default::default()
            },
            access_password: 0x1122_3344,
            mask_bits: 0x0020,
            action_bits: 0x0020,
        };
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d).unwrap();
        let buf = frame_bytes(0x25, &d);
        assert_eq!(
            hex(&buf),
            "FF 18 25 03 E8 01 11 22 33 44 00 20 00 20 60 11 11 22 22 33 33 44 44 55 55 66 66 \
             9E 7A "
        );
    }

    /// 手册 p.87 例1：FF 08 26 03 E8 00 11223344 00 9116（杀密码非 0 才合法，
    /// 手册示例用 0x11223344）。
    #[test]
    fn kill_tag_golden() {
        let req = KillTagRequest {
            timeout_ms: 1000,
            singulation: TagSingulation::default(),
            kill_password: 0x1122_3344,
        };
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d).unwrap();
        let buf = frame_bytes(0x26, &d);
        assert_eq!(hex(&buf), "FF 08 26 03 E8 00 11 22 33 44 00 91 16 ");
        // 密码 0 拒绝（易错点 ⑨）
        let mut d2 = heapless::Vec::<u8, 64>::new();
        assert!(KillTagRequest {
            timeout_ms: 1000,
            singulation: TagSingulation::default(),
            kill_password: 0
        }
        .encode(&mut d2)
        .is_err());
    }

    /// 手册 p.91 例1：FF 09 28 03 E8 00 02 00000001 02 C1 F3
    #[test]
    fn read_tag_golden() {
        let req = ReadTagRequest::new(
            1000,
            None,
            MemBank::Tid,
            1,
            2,
            TagSingulation::default(),
        );
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d).unwrap();
        let buf = frame_bytes(0x28, &d);
        assert_eq!(hex(&buf), "FF 09 28 03 E8 00 02 00 00 00 01 02 C1 F3 ");
    }

    /// 手册 p.91 例1 响应：FF 05 28 00 00 00 600401 3513 04?? 直接构造：
    /// Data = Option(00) + DataRead(6004 01) —— 手册印 Len=05。
    #[test]
    fn read_tag_parse_golden() {
        // 手册完整帧：FF 05 28 00 00 00 60 04 01 35 13 04（CRC=1304 自验）
        let buf = [
            0xFF, 0x05, 0x28, 0x00, 0x00, 0x00, 0x60, 0x04, 0x01, 0x35, 0x13, 0x04,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        let r = parse_read_tag(&f).unwrap();
        assert_eq!(r.option, 0x00);
        assert_eq!(r.read_data, &[0x60, 0x04, 0x01, 0x35]); // TID 前 4 字节
        assert!(r.metadata.is_none());
    }

    /// 手册 p.92 例4 响应：带元数据（AntID+Timestamp）。
    #[test]
    fn read_tag_parse_with_metadata() {
        // Data = 14 0014 02 00000015 12345678
        let mut b = heapless::Vec::<u8, 32>::new();
        let _ = b.extend_from_slice(&[
            0xFF, 0x0C, 0x28, 0x00, 0x00, 0x14, 0x00, 0x14, 0x02, 0x00, 0x00, 0x00, 0x15, 0x12,
            0x34, 0x56, 0x78,
        ]);
        let c = crate::frame::crc::crc16_ex10(&b[1..]);
        assert_eq!((c >> 8) as u8, 0xDC); // 手册 CRC DC42
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let r = parse_read_tag(&f).unwrap();
        assert_eq!(r.metadata_flags, Some(MetadataFlags(0x0014)));
        let md = r.metadata.unwrap();
        assert_eq!(md.antenna_id, Some(2));
        assert_eq!(md.timestamp_ms, Some(0x15));
        assert_eq!(r.read_data, &[0x12, 0x34, 0x56, 0x78]);
    }

    /// 手册 p.95 例1 0x2D（PDF 提取粘连，按域表重构验证）。
    #[test]
    fn block_write_encode() {
        let req = BlockWriteRequest {
            timeout_ms: 1000,
            singulation: TagSingulation::bank(SelectMode::EpcValue, 0, 16, &[0x11, 0x22]),
            mem_bank: MemBank::User,
            address: 0,
            word_count: 1,
            data: &[0x00, 0x00],
        };
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d).unwrap();
        // Timeout Chip Option(BIT6|Select) SubCmd Pwd(4) SelLen SelData WFlag Bank Addr WC Data
        assert_eq!(
            &d[..10],
            &[0x03, 0xE8, 0x00, 0x41, 0x00, 0xC7, 0x00, 0x00, 0x00, 0x00]
        );
        assert_eq!(&d[10..13], &[0x10, 0x11, 0x22]); // SelLen=16bit + SelData
        // WFlag Bank Addr WC Data
        assert_eq!(&d[13..], &[0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]);
    }

    /// 写入后读响应解析（p.74 例4 响应形态）。
    #[test]
    fn write_read_parse() {
        let mut b = heapless::Vec::<u8, 64>::new();
        let body: [u8; 6] = [
            0x2E, 0x23, 0x00, 0x00, 0x8C, 0x00, // Len(46) Op Status Flag Option
        ];
        let _ = b.extend_from_slice(&[0xFF]);
        let _ = b.extend_from_slice(&body);
        // 补 44 字节 ReadData（2+42）
        for i in 0..44u8 {
            let _ = b.push(i);
        }
        let c = crate::frame::crc::crc16_ex10(&b[1..]);
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let r = parse_write_read(&f).unwrap();
        assert_eq!(r.write_read_flag, 0x8C);
        assert_eq!(r.option, 0x00);
        assert_eq!(r.read_data.len(), 44);
    }

    #[test]
    fn rejects_bad_params() {
        let mut d = heapless::Vec::<u8, 128>::new();
        // 写数据奇数长度
        assert!(WriteTagRequest {
            timeout_ms: 1000,
            read_after_write: None,
            singulation: TagSingulation::default(),
            address: 0,
            mem_bank: MemBank::User,
            data: &[0xAA, 0xBB, 0xCC],
        }
        .encode(&mut d)
        .is_err());
        // 读字数 0 / >96
        assert!(ReadTagRequest::new(1000, None, MemBank::Tid, 0, 0, TagSingulation::default())
            .encode(&mut d)
            .is_err());
        assert!(
            ReadTagRequest::new(1000, None, MemBank::Tid, 0, 97, TagSingulation::default())
                .encode(&mut d)
                .is_err()
        );
        // 块写超 48 字
        assert!(BlockWriteRequest {
            timeout_ms: 1000,
            singulation: TagSingulation::default(),
            mem_bank: MemBank::User,
            address: 0,
            word_count: 49,
            data: &[0u8; 98],
        }
        .encode(&mut d)
        .is_err());
    }
}
