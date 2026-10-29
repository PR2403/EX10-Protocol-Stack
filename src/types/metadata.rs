//! MetadataFlags 与 Metadata 解析（手册 §5.1.3/§5.1.4，p.38-40）。

use crate::error::DecodeError;

/// Metadata Flags 位标志（§5.1.3 p.38）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct MetadataFlags(pub u16);

impl MetadataFlags {
    pub const NONE: Self = Self(0x0000);
    /// BIT0 ReadCount（1 字节）
    pub const READ_COUNT: Self = Self(0x0001);
    /// BIT1 RSSI（1 字节，补码 dBm）
    pub const RSSI: Self = Self(0x0002);
    /// BIT2 AntennaId（1 字节）
    pub const ANTENNA_ID: Self = Self(0x0004);
    /// BIT3 Frequency（3 字节，kHz）
    pub const FREQUENCY: Self = Self(0x0008);
    /// BIT4 Timestamp（4 字节，ms，首次盘存到的时间）
    pub const TIMESTAMP: Self = Self(0x0010);
    /// BIT5 Phase（2 字节；语义随指令，见 [`PhaseMode`]）
    pub const PHASE: Self = Self(0x0020);
    /// BIT6 ProtocolId（1 字节，0x05=GEN2）
    pub const PROTOCOL_ID: Self = Self(0x0040);
    /// BIT7 TagDataLength（2 字节 bit 数）+ TagData
    pub const TAG_DATA: Self = Self(0x0080);

    pub const fn bits(self) -> u16 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// 便捷组合：ReadCount|RSSI|AntennaId|Freq|Timestamp|Phase（手册常见 0x003F）。
    pub const COMMON: Self = Self(0x003F);

    /// 全量（0x00FF）。
    pub const ALL: Self = Self(0x00FF);

    /// 解析出启用的标志列表（Display 用）。
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl core::fmt::Display for MetadataFlags {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#06X}", self.0)
    }
}

/// Phase 域的两种语义（§5.1.4 p.39）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseMode {
    /// 0x21 单标签盘存 / 0x28 读存储区：高字节=起始相位、低字节=结束相位，
    /// 各 8 位，度数 = (相位/256)*360。
    StartStop,
    /// 0x22 同步盘存 / 异步盘存：2 字节结束相位，低 12 位有效，度数 = (相位/4096)*360。
    End,
}

/// 一条标签的元数据（按 MetadataFlags 逐域解析；缺失域为 `None`）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Metadata<'a> {
    /// 盘存时间内被盘存到的次数。
    pub read_count: Option<u8>,
    /// 信号强度 dBm（补码有符号字节）。
    pub rssi_dbm: Option<i8>,
    /// 盘存到时所用的天线号。
    pub antenna_id: Option<u8>,
    /// 盘存到时频率（kHz）。
    pub frequency_khz: Option<u32>,
    /// 从发出盘存指令到获取此标签的时间（ms）。
    pub timestamp_ms: Option<u32>,
    /// 相位原始值（语义随 [`PhaseMode`]）。
    pub phase: Option<u16>,
    /// 标签协议（0x05=GEN2）。
    pub protocol_id: Option<u8>,
    /// TagData 位长度（TagData 域存在时）。
    pub tag_data_len_bits: Option<u16>,
    /// TagData 字节（借用输入缓冲）。
    pub tag_data: &'a [u8],
}

impl Metadata<'_> {
    /// Phase 度数换算（§5.1.4 p.39）。
    pub fn phase_degrees(&self, mode: PhaseMode) -> Option<f32> {
        self.phase
            .map(|p| match mode {
                PhaseMode::StartStop => (p as f32 / 256.0) * 360.0,
                PhaseMode::End => ((p & 0x0FFF) as f32 / 4096.0) * 360.0,
            })
    }
}

/// 从 `data[0..]` 按标志逐域解析 Metadata。
///
/// 返回 `(Metadata, 消费字节数)`；数据不足返回 [`DecodeError::Truncated`]，绝不 panic。
pub fn parse_metadata<'a>(
    data: &'a [u8],
    flags: MetadataFlags,
    phase_mode: PhaseMode,
) -> Result<(Metadata<'a>, usize), DecodeError> {
    let mut md = Metadata::default();
    let mut pos = 0usize;
    let mut take = |n: usize| -> Result<&'a [u8], DecodeError> {
        if pos + n > data.len() {
            return Err(DecodeError::Truncated);
        }
        let sl = &data[pos..pos + n];
        pos += n;
        Ok(sl)
    };

    if flags.contains(MetadataFlags::READ_COUNT) {
        md.read_count = Some(take(1)?[0]);
    }
    if flags.contains(MetadataFlags::RSSI) {
        md.rssi_dbm = Some(take(1)?[0] as i8);
    }
    if flags.contains(MetadataFlags::ANTENNA_ID) {
        md.antenna_id = Some(take(1)?[0]);
    }
    if flags.contains(MetadataFlags::FREQUENCY) {
        let b = take(3)?;
        md.frequency_khz = Some(((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32);
    }
    if flags.contains(MetadataFlags::TIMESTAMP) {
        md.timestamp_ms = Some(u32::from_be_bytes(take(4)?.try_into().unwrap()));
    }
    if flags.contains(MetadataFlags::PHASE) {
        let b = take(2)?;
        md.phase = Some(u16::from_be_bytes([b[0], b[1]]));
        let _ = phase_mode; // 语义由调用方解释
    }
    if flags.contains(MetadataFlags::PROTOCOL_ID) {
        md.protocol_id = Some(take(1)?[0]);
    }
    if flags.contains(MetadataFlags::TAG_DATA) {
        let bits = u16::from_be_bytes(take(2)?.try_into().unwrap());
        md.tag_data_len_bits = Some(bits);
        let bytes = (bits as usize).div_ceil(8);
        md.tag_data = take(bytes)?;
    }
    Ok((md, pos))
}

/// 序列化 Metadata（按标志域序写入；编码侧极少用，主要为对称测试）。
pub fn encode_metadata<const N: usize>(
    md: &Metadata<'_>,
    flags: MetadataFlags,
    out: &mut heapless::Vec<u8, N>,
) -> Result<(), DecodeError> {
    if flags.contains(MetadataFlags::READ_COUNT) {
        let _ = out.push(md.read_count.unwrap_or(0));
    }
    if flags.contains(MetadataFlags::RSSI) {
        let _ = out.push(md.rssi_dbm.unwrap_or(0) as u8);
    }
    if flags.contains(MetadataFlags::ANTENNA_ID) {
        let _ = out.push(md.antenna_id.unwrap_or(0));
    }
    if flags.contains(MetadataFlags::FREQUENCY) {
        let v = md.frequency_khz.unwrap_or(0);
        let _ = out.push((v >> 16) as u8);
        let _ = out.push((v >> 8) as u8);
        let _ = out.push(v as u8);
    }
    if flags.contains(MetadataFlags::TIMESTAMP) {
        let _ = out.extend_from_slice(&md.timestamp_ms.unwrap_or(0).to_be_bytes());
    }
    if flags.contains(MetadataFlags::PHASE) {
        let _ = out.extend_from_slice(&md.phase.unwrap_or(0).to_be_bytes());
    }
    if flags.contains(MetadataFlags::PROTOCOL_ID) {
        let _ = out.push(md.protocol_id.unwrap_or(0x05));
    }
    if flags.contains(MetadataFlags::TAG_DATA) {
        let bits = md.tag_data_len_bits.unwrap_or((md.tag_data.len() * 8) as u16);
        let _ = out.extend_from_slice(&bits.to_be_bytes());
        let _ = out.extend_from_slice(md.tag_data);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手册 p.62 标签包实例2 的元数据段（flags=0x00BF）：
    /// 01 D3 01 0D CC 3A 0000001A 0017 0000 → ReadCount/RSSI/Ant/Freq/Ts/Phase
    #[test]
    fn parse_common_flags() {
        let data = [
            0x01, 0xD3, 0x01, 0x0D, 0xCC, 0x3A, 0x00, 0x00, 0x00, 0x1A, 0x00, 0x17,
        ];
        let flags = MetadataFlags(0x003F);
        let (md, used) = parse_metadata(&data, flags, PhaseMode::End).unwrap();
        assert_eq!(used, 12);
        assert_eq!(md.read_count, Some(1));
        assert_eq!(md.rssi_dbm, Some(-45)); // 0xD3
        assert_eq!(md.antenna_id, Some(1));
        assert_eq!(md.frequency_khz, Some(0x0DCC3A));
        assert_eq!(md.timestamp_ms, Some(0x1A));
        assert_eq!(md.phase, Some(0x0017));
    }

    #[test]
    fn truncated_never_panics() {
        let data = [0x01u8];
        assert_eq!(
            parse_metadata(&data, MetadataFlags::ALL, PhaseMode::End),
            Err(DecodeError::Truncated)
        );
    }

    #[test]
    fn encode_parse_roundtrip() {
        let md = Metadata {
            read_count: Some(3),
            rssi_dbm: Some(-40),
            antenna_id: Some(2),
            frequency_khz: Some(920_875),
            timestamp_ms: Some(1234),
            phase: Some(0x123),
            protocol_id: Some(5),
            tag_data_len_bits: Some(16),
            tag_data: &[0xAB, 0xCD],
        };
        let flags = MetadataFlags::ALL;
        let mut buf = heapless::Vec::<u8, 64>::new();
        encode_metadata(&md, flags, &mut buf).unwrap();
        let (back, used) = parse_metadata(&buf, flags, PhaseMode::End).unwrap();
        assert_eq!(used, buf.len());
        assert_eq!(back, md);
    }
}
