//! 标签盘存指令：0x21 单标签 / 0x22 同步 / 0x29 缓冲区（手册 §5.2-5.4，p.41-56）。

use crate::error::{DecodeError, EncodeError};
use crate::frame::RawFrame;
use crate::types::bank::MemBank;
use crate::types::metadata::{parse_metadata, Metadata, MetadataFlags, PhaseMode};
use crate::types::select::TagSingulation;

/// 0x21 单标签盘存请求（p.42）。
///
/// Data = Timeout(2) + Option(1) + [MetadataFlags(2)] + TagSingulation；
/// Option BIT4=1 时携带 MetadataFlags（响应也带元数据）。
/// 注意：0x21 的 Data 无 AccessPassword 域，且不支持 Select-Option=0x07。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleInventoryRequest {
    /// 盘存超时（ms，§5.1.6 p.41）。
    pub timeout_ms: u16,
    /// Some=返回元数据（Option BIT4=1）；None=仅 EPC。
    pub metadata_flags: Option<MetadataFlags>,
    /// 选择过滤（无密码域）。
    pub singulation: TagSingulation,
}

impl SingleInventoryRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.singulation.mode == crate::types::select::SelectMode::MultiTag {
            return Err(EncodeError::InvalidParameter("0x21 does not support 0x07"));
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
        self.singulation.encode(out, false)
    }
}

/// 0x21 响应（两种形态，p.42-43）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SingleInventoryResponse<'a> {
    /// 回显 Option。
    pub option: u8,
    /// 元数据（仅请求带 MetadataFlags 时）。
    pub metadata_flags: Option<MetadataFlags>,
    pub metadata: Option<Metadata<'a>>,
    /// PC + EPCID。
    pub epc: &'a [u8],
    /// 标签 CRC（0x21 该域恒为 TagCRC，p.40）。
    pub tag_crc: u16,
}

/// 0x21 响应解析。
pub fn parse_single_inventory<'a>(
    frame: &'a RawFrame<'a>,
) -> Result<SingleInventoryResponse<'a>, DecodeError> {
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
    if data.len() < pos + 2 {
        return Err(DecodeError::Truncated);
    }
    let tag_crc = u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]]);
    Ok(SingleInventoryResponse {
        option,
        metadata_flags: flags,
        metadata,
        epc: &data[pos..data.len() - 2],
        tag_crc,
    })
}

/// 0x22 SearchFlags 域（p.46-48）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SearchFlags {
    /// 低字节 BIT2：启用盘存嵌入指令（Embedded Command Content）。
    pub embedded: bool,
    /// 低字节 BIT4：IMPINJ TAGFOCUS（仅 S1 静态 TARGET A 有效）。
    pub tagfocus: bool,
    /// 高字节=0xA5：群读宜链温度标签（0x22 专用；0xAA48 不支持）。
    pub yilian_group_temp: bool,
    /// 低字节 BIT0/BIT1 原始值（手册实例出现 0x03，语义未详述，原样透传）。
    pub mode_bits: u8,
}

impl SearchFlags {
    pub const fn to_u16(self) -> u16 {
        let mut v = (self.mode_bits & 0x03) as u16;
        if self.embedded {
            v |= 0x0004;
        }
        if self.tagfocus {
            v |= 0x0010;
        }
        if self.yilian_group_temp {
            v |= 0xA500;
        }
        v
    }

    pub const fn from_u16(v: u16) -> Self {
        SearchFlags {
            embedded: v & 0x0004 != 0,
            tagfocus: v & 0x0010 != 0,
            yilian_group_temp: (v >> 8) as u8 == 0xA5,
            mode_bits: (v & 0x03) as u8,
        }
    }
}

/// 0x22 嵌入读指令（仅支持嵌入 0x28，p.47-49）。
///
/// Embedded Values = 0x28 的请求 Data：Timeout(2)+Option(1)+MemBank(1)
/// +Address(**4**)+WordCount(**1**)，共 9 字节（手册 p.51 实例）。
/// Embedded Values 中的 Timeout/Option 恒 0（不起作用，p.49）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmbeddedRead {
    pub mem_bank: MemBank,
    /// 读起始地址（字，4 字节）。
    pub address: u32,
    /// 读字数（1 字节；手册当前实现上限 32 字，p.49/p.51 矛盾取 32）。
    pub word_count: u8,
}

impl EmbeddedRead {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        // p.49 文字说嵌入读“最多 32 字节(16 字)”，但 p.51 例3 实际读 32 字(0x20)，
        // 两者矛盾；取宽限制 32 字与实例一致。
        if self.word_count > 32 {
            return Err(EncodeError::InvalidParameter("embedded read max 32 words"));
        }
        let _ = out.push(0x01); // Embedded Command Count 恒 1
        let _ = out.push(9); // Embedded Values 长度：Timeout2+Option1+Bank1+Addr4+Words1
        let _ = out.push(0x28); // Opcode
        let _ = out.extend_from_slice(&0u16.to_be_bytes()); // Timeout 恒 0
        let _ = out.push(0x00); // Option 恒 0
        let _ = out.push(self.mem_bank.value());
        let _ = out.extend_from_slice(&self.address.to_be_bytes());
        let _ = out.push(self.word_count);
        Ok(())
    }
}

/// 0x22 同步盘存请求（p.46-48）。
///
/// Data 序 = Option + SearchFlags(2) + Timeout(2) + [AccessPwd(4)] + TagSingulation
/// + [Embedded Command Content]。Select-Option=0x00 时不含 AccessPassword。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncInventoryRequest {
    /// Option BIT7=1 启用 FASTID（Select=0x07 或启用附加数据时无效）。
    pub fastid: bool,
    pub search_flags: SearchFlags,
    pub timeout_ms: u16,
    pub singulation: TagSingulation,
    /// 嵌入 0x28 读（search_flags.embedded 时必须 Some）。
    pub embedded: Option<EmbeddedRead>,
}

impl SyncInventoryRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.search_flags.embedded != self.embedded.is_some() {
            return Err(EncodeError::InvalidParameter(
                "search_flags.embedded must match embedded field",
            ));
        }
        let mut option = self.singulation.option_bits();
        if self.fastid {
            option |= 0x80;
        }
        let _ = out.push(option);
        let _ = out.extend_from_slice(&self.search_flags.to_u16().to_be_bytes());
        let _ = out.extend_from_slice(&self.timeout_ms.to_be_bytes());
        self.singulation.encode(out, true)?;
        if let Some(e) = &self.embedded {
            e.encode(out)?;
        }
        Ok(())
    }
}

/// 0x22 嵌入指令结果段（p.48-49）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedResult<'a> {
    pub count: u8,
    pub opcode: u8,
    pub succeeded: u16,
    pub failed: u16,
    /// 嵌入 0x28 成功时返回的首个标签数据（可为空）。
    pub data: &'a [u8],
}

/// 0x22 响应（p.48）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncInventoryResponse<'a> {
    pub option: u8,
    /// 原始 SearchFlags（>255 标签时 BIT4=1）。
    pub search_flags: u16,
    /// 盘存到的标签数（BIT4=1 时为 4 字节）。
    pub tags_found: u32,
    pub embedded_result: Option<EmbeddedResult<'a>>,
}

/// 0x22 响应解析。
pub fn parse_sync_inventory<'a>(
    frame: &'a RawFrame<'a>,
) -> Result<SyncInventoryResponse<'a>, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() < 4 {
        return Err(DecodeError::Truncated);
    }
    let option = data[0];
    let search_flags = u16::from_be_bytes([data[1], data[2]]);
    let mut pos = 3;
    let tags_found = if search_flags & 0x0010 != 0 {
        if data.len() < pos + 4 {
            return Err(DecodeError::Truncated);
        }
        let v = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        pos += 4;
        v
    } else {
        let v = data[pos] as u32;
        pos += 1;
        v
    };
    let embedded_result = if search_flags & 0x0004 != 0 {
        if data.len() < pos + 7 {
            return Err(DecodeError::Truncated);
        }
        let count = data[pos];
        let opcode = data[pos + 1];
        let succeeded = u16::from_be_bytes([data[pos + 2], data[pos + 3]]);
        let failed = u16::from_be_bytes([data[pos + 4], data[pos + 5]]);
        pos += 6;
        Some(EmbeddedResult {
            count,
            opcode,
            succeeded,
            failed,
            data: &data[pos..],
        })
    } else {
        None
    };
    Ok(SyncInventoryResponse {
        option,
        search_flags,
        tags_found,
        embedded_result,
    })
}

/// 0x29 Read Option（p.52）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOption {
    /// 0x00 获取尚未获取的标签。
    New = 0x00,
    /// 0x01 重复获取上次 0x29 读到的标签。
    Previous = 0x01,
}

/// 0x29 获取标签缓冲请求（p.52）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GetTagBufferRequest {
    pub metadata_flags: MetadataFlags,
    pub read_option: ReadOption,
}

impl GetTagBufferRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        let _ = out.extend_from_slice(&self.metadata_flags.bits().to_be_bytes());
        let _ = out.push(self.read_option as u8);
        Ok(())
    }
}

/// Tag EPC and Meta Data 块（§5.1.5 p.40）。
///
/// 块布局 = Metadata(flags) + EPCLength + PC(2) + EPCID + 尾域(2)。
/// 尾域语义（p.40）：flags 未启用相位时为 TagCRC；启用相位时（0x29/0xAA）为
/// 起始相位 PHASE2（结束相位在 Metadata 里）。EPCLength：0x29 = 2 字节域、
/// 单位 **bit**；0xAA 通知包 = 1 字节域、单位 **byte**（均含 PC+EPCID+尾域）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagEpcAndMeta<'a> {
    pub metadata: Metadata<'a>,
    /// PC Word 原始值（高 5 位 = EPC 字长）。
    pub pc: u16,
    /// 纯 EPCID 字节（不含 PC 与 TagCRC；FASTID 标签含 EPC0+TagCRC0+TID）。
    pub epc_id: &'a [u8],
    /// 块尾 2 字节原始值：flags 未启用相位时为 TagCRC；启用相位时为起始相位
    /// PHASE2（结束相位在 [`Self::metadata`] 的 phase 字段）。
    pub tag_crc: u16,
    /// FASTID 判定成功时提取的 TID。
    pub tid: Option<&'a [u8]>,
}

/// EPCLength 域的宽度与计量单位（易错点 p.40/p.61/p.64）：
/// 0x29 = 2 字节域、单位 **bit**；0xAA 通知包 = **1 字节域**、单位 byte。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpcLenUnit {
    /// 2 字节域，值为 bit 数（含 PC+EPCID+TagCRC）。
    Bits2,
    /// 1 字节域，值为字节数（含 PC+TagCRC）。
    Bytes1,
}

/// 解析一个 Tag EPC and Meta Data 块，返回 `(块, 消耗字节数)`。
pub fn parse_tag_epc_and_meta(
    data: &[u8],
    flags: MetadataFlags,
    unit: EpcLenUnit,
) -> Result<(TagEpcAndMeta<'_>, usize), DecodeError> {
    let (metadata, mut pos) = parse_metadata(data, flags, PhaseMode::End)?;
    if data.len() < pos + 4 {
        return Err(DecodeError::Truncated);
    }
    let (epc_len_raw, hdr) = match unit {
        EpcLenUnit::Bits2 => {
            if data.len() < pos + 2 {
                return Err(DecodeError::Truncated);
            }
            (u16::from_be_bytes([data[pos], data[pos + 1]]) as usize, 2usize)
        }
        EpcLenUnit::Bytes1 => (data[pos] as usize, 1usize),
    };
    pos += hdr;
    let pc = u16::from_be_bytes([data[pos], data[pos + 1]]);
    pos += 2;
    let block_bytes = match unit {
        EpcLenUnit::Bits2 => epc_len_raw.div_ceil(8),
        EpcLenUnit::Bytes1 => epc_len_raw,
    };
    // EPCLength 覆盖 PC(2)+EPCID+TagCRC(2)
    if block_bytes < 4 || data.len() < pos + block_bytes - 2 {
        return Err(DecodeError::Truncated);
    }
    let epc_id_len = block_bytes - 4;
    let epc_id = &data[pos..pos + epc_id_len];
    pos += epc_id_len;
    let tag_crc = u16::from_be_bytes([data[pos], data[pos + 1]]);
    pos += 2;
    let tid = try_extract_fastid(pc, epc_id);
    Ok((
        TagEpcAndMeta {
            metadata,
            pc,
            epc_id,
            tag_crc,
            tid,
        },
        pos,
    ))
}

/// FASTID 判定（§5.4.1 p.53-54）。
///
/// PC 高 5 位算 EPC 字长，≥7 时按 Impinj FASTID 格式重算 PC0+EPC0 的 TagCRC
/// 与域内 TagCRC0 比对；命中则 EPC 域 = EPC0 + TagCRC0(2) + TID(12)。
pub fn try_extract_fastid(pc: u16, epc_field: &[u8]) -> Option<&[u8]> {
    let word_len = (pc >> 8) >> 3 & 0x1F;
    if word_len < 7 {
        return None;
    }
    let epc0_words = (word_len - 7) as usize;
    let epc0_len = epc0_words * 2;
    if epc_field.len() < epc0_len + 2 + 12 {
        return None; // EPC0+TagCRC0+TID 至少需要这么多
    }
    let epc0 = &epc_field[..epc0_len];
    let crc0 = u16::from_be_bytes([epc_field[epc0_len], epc_field[epc0_len + 1]]);
    let pc0_hi = (word_len - 7) << 3;
    let pc0 = (pc0_hi << 8) | (pc & 0xFF);
    let mut buf = heapless::Vec::<u8, 64>::new();
    buf.extend_from_slice(&pc0.to_be_bytes()).ok()?;
    buf.extend_from_slice(epc0).ok()?;
    if crate::frame::crc::tag_crc(&buf) == crc0 {
        Some(&epc_field[epc0_len + 2..])
    } else {
        None
    }
}

/// 0x29 响应解析为游标（避免为 1200 标签上限预分配，p.52-53）。
///
/// Data = MetadataFlags(2) + ReadOption(1) + TagCount(1) + N × TagEpcAndMeta。
pub struct TagBufferCursor<'a> {
    data: &'a [u8],
    flags: MetadataFlags,
    unit: EpcLenUnit,
    count: u8,
    read_option: u8,
    pos: usize,
    idx: u8,
}

impl<'a> TagBufferCursor<'a> {
    pub fn parse(frame: &'a RawFrame<'a>) -> Result<Self, DecodeError> {
        let data = frame.ext_payload()?;
        if data.len() < 4 {
            return Err(DecodeError::Truncated);
        }
        Ok(TagBufferCursor {
            flags: MetadataFlags(u16::from_be_bytes([data[0], data[1]])),
            read_option: data[2],
            count: data[3],
            data: &data[4..],
            unit: EpcLenUnit::Bits2,
            pos: 0,
            idx: 0,
        })
    }

    pub fn flags(&self) -> MetadataFlags {
        self.flags
    }

    /// 响应中的标签总数（避免与 `Iterator::count` 冲突）。
    pub fn tag_count(&self) -> u8 {
        self.count
    }

    /// Read Option 原始字节（0=新标签，1=上次重复读）。
    pub fn read_option_raw(&self) -> u8 {
        self.read_option
    }
}

impl<'a> Iterator for TagBufferCursor<'a> {
    type Item = Result<TagEpcAndMeta<'a>, DecodeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.count {
            return None;
        }
        match parse_tag_epc_and_meta(&self.data[self.pos..], self.flags, self.unit) {
            Ok((tag, used)) => {
                self.pos += used;
                self.idx += 1;
                Some(Ok(tag))
            }
            Err(e) => {
                self.idx = self.count; // 出错即终止
                Some(Err(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::select::SelectMode;

    /// 手册 p.44 0x21 实例1：FF 03 21 03 E8 00 A5 E8
    #[test]
    fn single_inventory_encode_golden() {
        let mut out = heapless::Vec::<u8, 248>::new();
        SingleInventoryRequest {
            timeout_ms: 1000,
            metadata_flags: None,
            singulation: TagSingulation::default(),
        }
        .encode(&mut out)
        .unwrap();
        let mut buf = crate::frame::FrameBuf::new();
        crate::frame::builder::build_request(0x21, &out, &mut buf).unwrap();
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(hex, "FF 03 21 03 E8 00 A5 E8 ");
    }

    /// 手册 p.44 实例2：FF 05 21 01 E8 10 00 14 2F 6D
    #[test]
    fn single_inventory_metadata_golden() {
        let mut out = heapless::Vec::<u8, 248>::new();
        SingleInventoryRequest {
            timeout_ms: 488,
            metadata_flags: Some(MetadataFlags(0x0014)),
            singulation: TagSingulation::default(),
        }
        .encode(&mut out)
        .unwrap();
        let mut buf = crate::frame::FrameBuf::new();
        crate::frame::builder::build_request(0x21, &out, &mut buf).unwrap();
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(hex, "FF 05 21 01 E8 10 00 14 2F 6D ");
    }

    /// 手册 p.45 实例3：EPC 过滤（Option=0x11 → Select=0x01+BIT4）
    #[test]
    fn single_inventory_select_golden() {
        let epc = [0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66];
        let mut out = heapless::Vec::<u8, 248>::new();
        SingleInventoryRequest {
            timeout_ms: 488,
            metadata_flags: Some(MetadataFlags(0x0014)),
            singulation: TagSingulation::epc_value(&epc).unwrap(),
        }
        .encode(&mut out)
        .unwrap();
        let mut buf = crate::frame::FrameBuf::new();
        crate::frame::builder::build_request(0x21, &out, &mut buf).unwrap();
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(
            hex,
            "FF 12 21 01 E8 11 00 14 60 11 11 22 22 33 33 44 44 55 55 66 66 9F CE "
        );
    }

    /// 手册 p.43-44 实例1 响应：仅 EPC 形态（EPC=PC+EPCID，尾部 TagCRC）。
    #[test]
    fn single_inventory_parse_epc_only() {
        // Data = Option(00) + EPC(C0C0 1111 2222 3333 AAAA 0009) + TagCRC(641F)
        let mut b = heapless::Vec::<u8, 32>::new();
        let _ = b.extend_from_slice(&[
            0xFF, 0x0F, 0x21, 0x00, 0x00, 0x00, 0xC0, 0xC0, 0x11, 0x11, 0x22, 0x22, 0x33, 0x33,
            0xAA, 0xAA, 0x00, 0x09, 0x64, 0x1F,
        ]);
        let c = crate::frame::crc::crc16_ex10(&b[1..20]);
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let r = parse_single_inventory(&f).unwrap();
        assert_eq!(r.option, 0x00);
        assert_eq!(r.epc, &[0xC0, 0xC0, 0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0xAA, 0xAA, 0x00, 0x09]);
        assert_eq!(r.tag_crc, 0x641F & 0xFFFF);
    }

    /// 手册 p.44 实例2 响应：含元数据形态（AntID + Timestamp）。
    #[test]
    fn single_inventory_parse_with_metadata() {
        // FF 16 21 00 00 10 00 14 01 00 BB 5F 04 01 23 45 67 89 AB CD EF 01 23 45 67 E6 C8 + CRC 83 D0
        let mut b = heapless::Vec::<u8, 48>::new();
        let _ = b.extend_from_slice(&[
            0xFF, 0x16, 0x21, 0x00, 0x00, 0x10, 0x00, 0x14, 0x01, 0x00, 0xBB, 0x5F, 0x04, 0x01,
            0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, 0xE6, 0xC8,
        ]);
        let c = crate::frame::crc::crc16_ex10(&b[1..]);
        assert_eq!((c >> 8) as u8, 0x83); // 手册 CRC 83 D0
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let r = parse_single_inventory(&f).unwrap();
        assert_eq!(r.metadata_flags, Some(MetadataFlags(0x0014)));
        let md = r.metadata.unwrap();
        assert_eq!(md.antenna_id, Some(1));
        assert_eq!(md.timestamp_ms, Some(0x00BB5F04));
        assert_eq!(r.epc, &[0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67]);
        assert_eq!(r.tag_crc, 0xE6C8);
    }

    /// 手册 p.50 0x22 例1：EPC 过滤盘存。
    /// 注：手册印 Len=0x0F 但实例只有 13 字节 data（PDF 提取/印刷丢字），
    /// 按 p.37 域存在性表（Select=0x04 需 Pwd+Addr+Len+Data）实际为 15 字节。
    #[test]
    fn sync_inventory_encode_golden() {
        let mut out = heapless::Vec::<u8, 248>::new();
        SyncInventoryRequest {
            fastid: false,
            search_flags: SearchFlags::default(),
            timeout_ms: 1000,
            singulation: TagSingulation::bank(SelectMode::EpcBank, 0, 8, &[0x78]),
            embedded: None,
        }
        .encode(&mut out)
        .unwrap();
        // Option SF Timeout Pwd(4) Addr(4) Len Data
        assert_eq!(
            &out[..],
            &[
                0x04, 0x00, 0x00, 0x03, 0xE8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x08, 0x78
            ]
        );
    }

    /// 手册 p.50 例2：FASTID 无过滤。FF 05 22 80 00 00 00 C8 33 2D
    #[test]
    fn sync_inventory_fastid_golden() {
        let mut out = heapless::Vec::<u8, 248>::new();
        SyncInventoryRequest {
            fastid: true,
            search_flags: SearchFlags::default(),
            timeout_ms: 200,
            singulation: TagSingulation::default(),
            embedded: None,
        }
        .encode(&mut out)
        .unwrap();
        let mut buf = crate::frame::FrameBuf::new();
        crate::frame::builder::build_request(0x22, &out, &mut buf).unwrap();
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(hex, "FF 05 22 80 00 00 00 C8 33 2D ");
    }

    /// 手册 p.50-51 例3：嵌入 0x28 读 USER bank 32 字。
    /// Data = Option(1)+SF(2)+Timeout(2)+Embedded(12) = 17 字节，
    /// 与手册 Len=0x11 一致（修正：旧实现把 Address/WordCount 写成 2+2 字节，少 1 字节）。
    #[test]
    fn sync_inventory_embedded_encode() {
        let mut out = heapless::Vec::<u8, 248>::new();
        SyncInventoryRequest {
            fastid: false,
            search_flags: SearchFlags {
                embedded: true,
                ..Default::default()
            },
            timeout_ms: 1000,
            singulation: TagSingulation::default(),
            embedded: Some(EmbeddedRead {
                mem_bank: MemBank::User,
                address: 0,
                word_count: 32,
            }),
        }
        .encode(&mut out)
        .unwrap();
        assert_eq!(
            &out[..],
            &[
                0x00, 0x00, 0x04, 0x03, 0xE8, // Option SF Timeout
                0x01, 0x09, 0x28, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x20, // Embedded
            ]
        );
        assert_eq!(out.len(), 17); // 与手册 p.51 示例的 Len=0x11 一致
    }

    /// 手册 p.50 例1 响应 + >255 标签变体 + p.51 例4 嵌入响应。
    #[test]
    fn sync_inventory_parse_golden() {
        // 例1: FF 04 22 00 00 04 00 00 02 B7 6E → 2 个标签
        let mut b = heapless::Vec::<u8, 32>::new();
        let _ = b.extend_from_slice(&[0xFF, 0x04, 0x22, 0x00, 0x00, 0x04, 0x00, 0x00, 0x02]);
        let c = crate::frame::crc::crc16_ex10(&b[1..]);
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let r = parse_sync_inventory(&f).unwrap();
        assert_eq!(r.tags_found, 2);
        assert!(r.embedded_result.is_none());

        // >255: FF 07 22 00 00 04 00 10 00 00 01 01 5A 0E → 257
        let mut b = heapless::Vec::<u8, 32>::new();
        let _ = b.extend_from_slice(&[
            0xFF, 0x07, 0x22, 0x00, 0x00, 0x04, 0x00, 0x10, 0x00, 0x00, 0x01, 0x01,
        ]);
        let c = crate::frame::crc::crc16_ex10(&b[1..]);
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let r = parse_sync_inventory(&f).unwrap();
        assert_eq!(r.tags_found, 257);

        // p.51 例4 嵌入响应: 02 00 04 1C 01 28 00 01 00 2F 2222 1111
        let mut b = heapless::Vec::<u8, 32>::new();
        let _ = b.extend_from_slice(&[
            0xFF, 0x0E, 0x22, 0x00, 0x00, 0x02, 0x00, 0x04, 0x1C, 0x01, 0x28, 0x00, 0x01, 0x00,
            0x2F, 0x22, 0x22, 0x11, 0x11,
        ]);
        let c = crate::frame::crc::crc16_ex10(&b[1..]);
        assert_eq!((c >> 8) as u8, 0xC6); // 手册 CRC C6F2
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let r = parse_sync_inventory(&f).unwrap();
        assert_eq!(r.tags_found, 0x1C);
        let er = r.embedded_result.unwrap();
        assert_eq!(er.opcode, 0x28);
        assert_eq!(er.succeeded, 1);
        assert_eq!(er.failed, 0x2F);
        assert_eq!(er.data, &[0x22, 0x22, 0x11, 0x11]); // 读到的访问密码
    }

    /// 手册 p.55 0x29 请求：FF 03 29 00 15 00 E1 22
    #[test]
    fn tag_buffer_request_golden() {
        let mut out = heapless::Vec::<u8, 248>::new();
        GetTagBufferRequest {
            metadata_flags: MetadataFlags(0x0015),
            read_option: ReadOption::New,
        }
        .encode(&mut out)
        .unwrap();
        let mut buf = crate::frame::FrameBuf::new();
        crate::frame::builder::build_request(0x29, &out, &mut buf).unwrap();
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(hex, "FF 03 29 00 15 00 E1 22 ");
    }

    /// 手册 p.55 0x29 响应例1（flags=0x0015，2 标签）：EPCLength 单位 bit。
    #[test]
    fn tag_buffer_parse_golden() {
        let mut b = heapless::Vec::<u8, 128>::new();
        let body: [u8; 56] = [
            0x34, 0x29, 0x00, 0x00, 0x00, 0x15, 0x00, 0x02, // Len Op Status Flags Opt Count
            0x22, 0x01, 0x02, 0x50, 0xCE, 0xF6, // 标签1 meta: RC=34 Ant=1 Ts
            0x00, 0x80, 0x31, 0xC1, 0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55,
            0x66, 0x66, 0xFB, 0x15, // EPCLen PC EPCID TagCRC
            0x01, 0x04, 0x1D, 0x3D, 0x3C, 0x00, // 标签2 meta: RC=1 Ant=4 Ts=1D3D3C00
            0x00, 0x80, 0x30, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x23, 0x54, 0x4A, 0xC8, // EPCLen PC EPCID TagCRC=4AC8
        ];
        let _ = b.extend_from_slice(&[0xFF]);
        let _ = b.extend_from_slice(&body);
        let c = crate::frame::crc::crc16_ex10(&body);
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = RawFrame::parse(&b).unwrap();
        let cursor = TagBufferCursor::parse(&f).unwrap();
        assert_eq!(cursor.tag_count(), 2);
        let tags: heapless::Vec<TagEpcAndMeta<'_>, 8> = cursor
            .map(|t| t.unwrap())
            .collect::<heapless::Vec<_, 8>>();
        assert_eq!(tags.len(), 2);
        let t0 = &tags[0];
        assert_eq!(t0.metadata.read_count, Some(0x22));
        assert_eq!(t0.metadata.antenna_id, Some(1));
        assert_eq!(t0.metadata.timestamp_ms, Some(0x0250_CEF6));
        assert_eq!(t0.pc, 0x31C1);
        assert_eq!(
            t0.epc_id,
            &[0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44, 0x55, 0x55, 0x66, 0x66]
        );
        assert_eq!(t0.tag_crc, 0xFB15);
        let t1 = &tags[1];
        assert_eq!(t1.pc, 0x3000);
        assert_eq!(
            t1.epc_id,
            &[0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x23, 0x54]
        );
        assert_eq!(t1.tag_crc, 0x4AC8); // 与附录 5 TagCRC 向量互证
    }

    /// FASTID 判定（p.53-54 规则）：构造 EPC0+TagCRC0+TID 的字段自洽验证。
    #[test]
    fn fastid_detect() {
        // PC 高 5 位 EPC 字长 9 → EPC0 = 9-7 = 2 字 = 4 字节
        let epc0 = [0x11u8; 4];
        let pc0_hi = ((9 - 7) << 3) as u16; // 0x10
        let pc0 = pc0_hi << 8;
        let mut buf = heapless::Vec::<u8, 64>::new();
        let _ = buf.extend_from_slice(&pc0.to_be_bytes());
        let _ = buf.extend_from_slice(&epc0);
        let crc0 = crate::frame::crc::tag_crc(&buf);
        // 组 FASTID EPC 域：EPC0 + TagCRC0 + TID(12)
        let mut field = heapless::Vec::<u8, 64>::new();
        let _ = field.extend_from_slice(&epc0);
        let _ = field.extend_from_slice(&crc0.to_be_bytes());
        let _ = field.extend_from_slice(&[0xE2u8; 12]);
        assert!(try_extract_fastid(0x4800, &field).is_some());
        // 非 FASTID：字长 < 7
        assert!(try_extract_fastid(0x2000, &field).is_none());
        // TagCRC0 不匹配 → None
        let mut bad = field.clone();
        bad[4] ^= 0xFF;
        assert!(try_extract_fastid(0x4800, &bad).is_none());
    }
}
