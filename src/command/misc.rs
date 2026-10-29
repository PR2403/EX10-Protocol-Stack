//! 其它扩展指令：0xAA4A 回波 / 0xAA4C 多标签过滤 / 0xAA50 宜链温度 / 0xAA51 宜链亮灯
//! （手册第 10 章，p.149-163）。

use crate::error::{DecodeError, EncodeError};
use crate::frame::{builder::build_extended, FrameBuf, RawFrame};
use crate::types::metadata::{parse_metadata, Metadata, MetadataFlags, PhaseMode};

/// 0xAA4A 回波检测请求（p.149-150）。
///
/// SubData = 功率高/低字节(2，恒 0x07D0) + 逻辑天线(1) + 频段(1) + 频点数 N(1)
/// + [频点(3B) × N]。N=0 检测整个频段（FCC 约 25s，建议超时 30s）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EchoDetectRequest<'a> {
    /// 检测功率（0.01dBm，手册：目前不起实际作用，填 3000）。
    pub power: u16,
    /// 逻辑天线号。
    pub antenna: u8,
    /// 频段码（同 RegionCode 值域；国外版只能指定本国频段）。
    pub band: u8,
    /// 指定频点表（≤ 该频段最大频点数；中国版可个别指定，国外必须空）。
    pub frequencies: &'a [u32],
}

impl EchoDetectRequest<'_> {
    pub fn encode(&self, out: &mut FrameBuf) -> Result<usize, EncodeError> {
        if self.frequencies.len() > 255 {
            return Err(EncodeError::InvalidParameter("too many frequencies"));
        }
        let mut sub = heapless::Vec::<u8, 240>::new();
        let _ = sub.extend_from_slice(&self.power.to_be_bytes());
        let _ = sub.push(self.antenna);
        let _ = sub.push(self.band);
        let _ = sub.push(self.frequencies.len() as u8);
        for &f in self.frequencies {
            let b = f.to_be_bytes();
            let _ = sub.push(b[1]); // 3 字节频点（kHz）
            let _ = sub.push(b[2]);
            let _ = sub.push(b[3]);
        }
        build_extended(0xAA4A, &sub, out)
    }
}

/// 单频点回波检测结果（p.150-151）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EchoResult {
    /// 频点（kHz）。
    pub frequency_khz: u32,
    /// 回波损耗原始值 VL，单位 0.1dB（值越小匹配越好，通常应 < 70 即 7dB）。
    pub return_loss_raw: u8,
}

/// 10^x（no_std 无 libm 依赖的最小实现；x∈[0,1.3] 时误差 <1e-5）。
fn pow10(x: f32) -> f32 {
    // 10^x = 2^(x·log2(10)) = 2^i · 2^f，2^f 用 e^(f·ln2) 泰勒展开
    let y = x * core::f32::consts::LOG2_10;
    let i = y as i32;
    let f = y - i as f32;
    let t = f * core::f32::consts::LN_2;
    let mut term = 1.0f32;
    let mut sum = 1.0f32;
    let mut n = 1.0f32;
    for _ in 0..7 {
        term *= t / n;
        sum += term;
        n += 1.0;
    }
    let mut scale = 1.0f32;
    for _ in 0..i.unsigned_abs() {
        scale *= 2.0;
    }
    if i >= 0 { sum * scale } else { sum / scale }
}

impl EchoResult {
    /// 回波损耗 RL = 10^(VL/200)。
    pub fn return_loss(&self) -> f32 {
        pow10(self.return_loss_raw as f32 / 200.0)
    }

    /// 驻波比 VSWR = (1+RL)/(RL-1)。手册例：VL=0x78(120) → VSWR≈1.67。
    pub fn vswr(&self) -> f32 {
        let rl = self.return_loss();
        (1.0 + rl) / (rl - 1.0)
    }
}

/// 0xAA4A 响应解析：data = 回显头(5B) + 每 4 字节（频点 3B + 回波 1B）。
pub fn parse_echo_detect(frame: &RawFrame<'_>) -> Result<(u16, u8, u8, heapless::Vec<EchoResult, 64>), DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() < 5 {
        return Err(DecodeError::Truncated);
    }
    let power = u16::from_be_bytes([data[0], data[1]]);
    let antenna = data[2];
    let band = data[3];
    let n = data[4] as usize;
    let m = (data.len() - 5) / 4;
    if data.len() != 5 + m * 4 || (n != 0 && m != n) {
        return Err(DecodeError::BadFrame);
    }
    let mut results = heapless::Vec::new();
    for i in 0..m {
        let o = 5 + i * 4;
        let freq = ((data[o] as u32) << 16) | ((data[o + 1] as u32) << 8) | data[o + 2] as u32;
        let _ = results.push(EchoResult {
            frequency_khz: freq,
            return_loss_raw: data[o + 3],
        });
    }
    Ok((power, antenna, band, results))
}

/// 一条多标签过滤条目（p.152-153 SELTAGDATAN）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelTagEntry {
    /// 过滤 bank：1=EPC，2=TID，3=USER。
    pub bank: u8,
    /// 过滤位地址。
    pub address: u32,
    /// 过滤位长度（≤255）。
    pub bits_len: u8,
    /// 过滤数据（ceil(bits_len/8) 字节）。
    pub data: heapless::Vec<u8, 32>,
}

impl SelTagEntry {
    pub fn encode(&self, out: &mut heapless::Vec<u8, 240>) -> Result<(), EncodeError> {
        let need = (self.bits_len as usize).div_ceil(8);
        if need != self.data.len() {
            return Err(EncodeError::InvalidParameter("sel data length mismatch"));
        }
        let entry_len = 7 + need; // selLEN(1)+bank(1)+addr(4)+bitsLen(1)+data
        if entry_len > 255 {
            return Err(EncodeError::InvalidParameter("entry too long"));
        }
        let _ = out.push(entry_len as u8);
        let _ = out.push(self.bank);
        let _ = out.extend_from_slice(&self.address.to_be_bytes());
        let _ = out.push(self.bits_len);
        let _ = out.extend_from_slice(&self.data);
        Ok(())
    }
}

/// 0xAA4C 多标签匹配过滤设置请求（p.152-154）。
///
/// 最多 16 张标签、每条 ≤255 位；一条指令发完时 SELFLAG=0xFFFF。
/// 使用多标签过滤盘存（Select-Option=0x07）前必须先成功执行本指令。
pub struct MultiTagFilterRequest<'a> {
    pub entries: &'a [SelTagEntry],
}

impl MultiTagFilterRequest<'_> {
    pub fn encode(&self, out: &mut FrameBuf) -> Result<usize, EncodeError> {
        if self.entries.is_empty() || self.entries.len() > 16 {
            return Err(EncodeError::InvalidParameter("1..=16 tags"));
        }
        let mut sub = heapless::Vec::<u8, 240>::new();
        let _ = sub.extend_from_slice(&0xFFFFu16.to_be_bytes()); // 单条指令发完
        let _ = sub.push(self.entries.len() as u8);
        for e in self.entries {
            e.encode(&mut sub)?;
        }
        build_extended(0xAA4C, &sub, out)
    }
}

/// 0xAA50 宜链温度标签读取请求（p.155-156）。
///
/// SubData = TIMEALL(2) + TIMESELWAIT(2) + TIMEREADWAIT(2) + 0x28 形态域。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YilianTempRequest<'a> {
    /// 指令执行总时间（ms）。
    pub time_all_ms: u16,
    /// 发送完匹配过滤后的等待（ms，≤300，不用为 0）。
    pub time_sel_wait_ms: u16,
    /// 读内存前等待（ms，≤300，不用为 0）。
    pub time_read_wait_ms: u16,
    /// 0x28 形态：Some=返回元数据（BIT4）。
    pub metadata_flags: Option<MetadataFlags>,
    /// 读的 bank（温度数据所在 USER 区等）。
    pub mem_bank: u8,
    pub address: u32,
    pub word_count: u8,
    pub singulation: crate::types::select::TagSingulation,
    _m: core::marker::PhantomData<&'a ()>,
}

impl YilianTempRequest<'_> {
    pub fn encode(&self, out: &mut FrameBuf) -> Result<usize, EncodeError> {
        if self.time_sel_wait_ms > 300 || self.time_read_wait_ms > 300 {
            return Err(EncodeError::InvalidParameter("wait times ≤300ms"));
        }
        let mut sub = heapless::Vec::<u8, 240>::new();
        let _ = sub.extend_from_slice(&self.time_all_ms.to_be_bytes());
        let _ = sub.extend_from_slice(&self.time_sel_wait_ms.to_be_bytes());
        let _ = sub.extend_from_slice(&self.time_read_wait_ms.to_be_bytes());
        let mut option = self.singulation.option_bits();
        if self.metadata_flags.is_some() {
            option |= 0x10;
        }
        let _ = sub.push(option);
        if let Some(f) = self.metadata_flags {
            let _ = sub.extend_from_slice(&f.bits().to_be_bytes());
        }
        let _ = sub.push(self.mem_bank);
        let _ = sub.extend_from_slice(&self.address.to_be_bytes());
        let _ = sub.push(self.word_count);
        self.singulation.encode(&mut sub, true)?;
        build_extended(0xAA50, &sub, out)
    }
}

/// 宜链温度量程（p.158，由 PC 低字节判定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TempRange {
    /// PC 低字节 0x00：-30~97℃。
    Range30to97,
    /// PC 低字节 0x48：0~120℃。
    Range0to120,
    /// PC 低字节 0x78：-40~120℃。
    Range40to120,
    /// 未知量程。
    Unknown,
}

/// 由 PC 低字节判定量程。
pub fn temp_range_from_pc(pc: u16) -> TempRange {
    match pc & 0xFF {
        0x00 => TempRange::Range30to97,
        0x48 => TempRange::Range0to120,
        0x78 => TempRange::Range40to120,
        _ => TempRange::Unknown,
    }
}

/// 换算温度（p.158-159）：温度数据 [整数, 小数]，
/// 小数 = byte2*100/256。
pub fn convert_temperature(range: TempRange, raw: [u8; 2]) -> Option<f32> {
    let (int_part, frac) = match range {
        TempRange::Range30to97 => {
            let negative = raw[0] & 0x80 != 0;
            let i = if negative {
                (!raw[0]).wrapping_sub(30) as i16
            } else {
                raw[0] as i16 - 30
            };
            let f = if negative {
                ((!raw[1]).wrapping_add(1) as u32) * 100 / 256
            } else {
                raw[1] as u32 * 100 / 256
            };
            (i, f)
        }
        TempRange::Range0to120 => (raw[0] as i16, raw[1] as u32 * 100 / 256),
        TempRange::Range40to120 => (raw[0] as i16 - 45, raw[1] as u32 * 100 / 256),
        TempRange::Unknown => return None,
    };
    Some(int_part as f32 + frac as f32 / 100.0)
}

/// 0xAA50 响应（两种形态，p.156-157）。
pub struct YilianTempResponse<'a> {
    pub option: u8,
    pub metadata_flags: Option<MetadataFlags>,
    pub metadata: Option<Metadata<'a>>,
    /// 读到的存储区数据（温度原始 [整数, 小数] 在其中）。
    pub read_data: &'a [u8],
    /// PC+EPCID+TagCRC 总字节长。
    pub tag_len: u8,
    pub pc: u16,
    pub epc_id: &'a [u8],
    pub tag_crc: u16,
}

/// 0xAA50 响应解析。`with_meta`/`flags`/`word_count` 由请求回推（域序 p.156-157）。
pub fn parse_yilian_temp<'a>(
    frame: &'a RawFrame<'a>,
    with_meta: bool,
    _flags: MetadataFlags,
    word_count: usize,
) -> Result<YilianTempResponse<'a>, DecodeError> {
    let data = frame.ext_payload()?;
    if data.is_empty() {
        return Err(DecodeError::Truncated);
    }
    let option = data[0];
    let mut pos = 1;
    let (mf, metadata) = if with_meta {
        if data.len() < pos + 2 {
            return Err(DecodeError::Truncated);
        }
        let f = MetadataFlags(u16::from_be_bytes([data[pos], data[pos + 1]]));
        pos += 2;
        let (md, used) = parse_metadata(&data[pos..], f, PhaseMode::StartStop)?;
        pos += used;
        (Some(f), Some(md))
    } else {
        (None, None)
    };
    if data.len() < pos + word_count * 2 + 1 {
        return Err(DecodeError::Truncated);
    }
    let read_data = &data[pos..pos + word_count * 2];
    pos += word_count * 2;
    let tag_len = data[pos] as usize;
    pos += 1;
    if data.len() < pos + tag_len {
        return Err(DecodeError::Truncated);
    }
    let pc = u16::from_be_bytes([data[pos], data[pos + 1]]);
    let epc_id = &data[pos + 2..pos + tag_len - 2];
    let tag_crc = u16::from_be_bytes([data[pos + tag_len - 2], data[pos + tag_len]]);
    Ok(YilianTempResponse {
        option,
        metadata_flags: mf,
        metadata,
        read_data,
        tag_len: tag_len as u8,
        pc,
        epc_id,
        tag_crc,
    })
}

/// 0xAA51 宜链亮灯请求（p.160-163）。
///
/// SubData = Timeout(2) + Option(1)。普通单标签点亮：Timeout = 点亮时长(ms)；
/// Option BIT7=1 时 Timeout 低字节须为 0xFF 且实际含义为亮灯持续时长；
/// Option=0x07（多标签齐亮，需先 0xAA4C）时只返回成功/失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LedLightRequest {
    pub timeout_ms: u16,
    /// Option（BIT7 重定义 Timeout 语义；0x07=多标签同时点亮）。
    pub option: u8,
}

impl LedLightRequest {
    pub fn encode(&self, out: &mut FrameBuf) -> Result<usize, EncodeError> {
        // 注：手册 p.163 文字说 BIT7 模式 Timeout 低字节须固定 0xFF，但其官方
        // 实例为 Timeout=0x2020 Option=0x87，两者矛盾；此处不强制，由调用方决定。
        let mut sub = heapless::Vec::<u8, 16>::new();
        let _ = sub.extend_from_slice(&self.timeout_ms.to_be_bytes());
        let _ = sub.push(self.option);
        build_extended(0xAA51, &sub, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::select::{SelectMode, TagSingulation};

    fn hex(buf: &[u8]) -> String {
        buf.iter().map(|b| format!("{b:02X} ")).collect()
    }

    /// 手册 p.151：天线 1、北美、全部频点。手册尾值 `B9 BB BD 67`
    /// （SubCRC=B9、Terminator、CRC=BD67）。
    #[test]
    fn echo_detect_golden() {
        let req = EchoDetectRequest {
            power: 3000,
            antenna: 1,
            band: 0x01,
            frequencies: &[],
        };
        let mut buf = FrameBuf::new();
        req.encode(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 13 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 4A 0B B8 01 01 00 B9 BB BD 67 "
        );
    }

    /// p.151 响应解析与 VSWR 换算：首频点 915250，VL=0x78 → VSWR≈1.67。
    #[test]
    fn echo_parse_and_vswr() {
        // 构造响应 data：0BB8 01 01 02 | 0DF732 78 | 0DC65E 82
        let mut sub = heapless::Vec::<u8, 64>::new();
        let _ = sub.extend_from_slice(&[0x0B, 0xB8, 0x01, 0x01, 0x02, 0x0D, 0xF7, 0x32, 0x78]);
        let _ = sub.extend_from_slice(&[0x0D, 0xC6, 0x5E, 0x82]);
        let mut f = heapless::Vec::<u8, 96>::new();
        let payload_len = 12 + sub.len() as u8; // Marker(10)+SubCode(2)+SubData
        let _ = f.push(0xFF);
        let _ = f.push(payload_len);
        let _ = f.push(0xAA);
        let _ = f.extend_from_slice(&[0x00, 0x00]);
        let _ = f.extend_from_slice(&crate::frame::EXT_MARKER);
        let _ = f.extend_from_slice(&[0xAA, 0x4A]);
        let _ = f.extend_from_slice(&sub);
        let c = crate::frame::crc::crc16_ex10(&f[1..]);
        let _ = f.push((c >> 8) as u8);
        let _ = f.push(c as u8);
        let frame = RawFrame::parse(&f).unwrap();
        let (power, antenna, band, results) = parse_echo_detect(&frame).unwrap();
        assert_eq!((power, antenna, band), (3000, 1, 1));
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].frequency_khz, 915_250);
        assert!((results[0].vswr() - 1.67).abs() < 0.01);
    }

    /// 手册 p.153-154：3 标签过滤（SELFLAG=FFFF）。手册尾值 `8D BB 4E 77`。
    #[test]
    fn multi_tag_filter_golden() {
        let epc1 = [
            0xE2, 0x00, 0x81, 0x81, 0x81, 0x16, 0x02, 0x53, 0x10, 0x40, 0xAF, 0x36,
        ];
        let entries = [
            SelTagEntry {
                bank: 1,
                address: 0x20,
                bits_len: 0x60,
                data: epc1.iter().copied().collect(),
            },
            SelTagEntry {
                bank: 1,
                address: 0x20,
                bits_len: 0x10,
                data: [0x42, 0x98].iter().copied().collect(),
            },
            SelTagEntry {
                bank: 1,
                address: 0x20,
                bits_len: 0x60,
                data: [
                    0x11, 0x22, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x10, 0x00,
                ]
                .iter()
                .copied()
                .collect(),
            },
        ];
        let req = MultiTagFilterRequest { entries: &entries };
        let mut buf = FrameBuf::new();
        req.encode(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 40 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 4C FF FF 03 13 01 00 00 00 20 60 E2 00 \
             81 81 81 16 02 53 10 40 AF 36 09 01 00 00 00 20 10 42 98 13 01 00 00 00 20 60 11 \
             22 10 00 00 00 00 00 02 00 10 00 8D BB 4E 77 "
        );
    }

    /// 手册 p.159 宜链温度请求（尾值 `C8 BB 6C B6`：SubCRC=C8、CRC=6CB6）。
    #[test]
    fn yilian_temp_golden() {
        let req = YilianTempRequest {
            time_all_ms: 0x044C,
            time_sel_wait_ms: 0,
            time_read_wait_ms: 100,
            metadata_flags: Some(MetadataFlags(0x000F)),
            mem_bank: 0x03,
            address: 0x7F,
            word_count: 1,
            // 手册：过滤 TID，从地址 0 开始，过滤数据 E2008x（20 位）
            singulation: TagSingulation::bank(SelectMode::Tid, 0, 0x14, &[0xE2, 0x00, 0x80]),
            _m: core::marker::PhantomData,
        };
        let mut buf = FrameBuf::new();
        req.encode(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 29 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 50 04 4C 00 00 00 64 12 00 0F 03 00 00 \
             00 7F 01 00 00 00 00 00 00 00 00 14 E2 00 80 C8 BB 6C B6 "
        );
    }

    /// 手册 p.163 亮灯：多标签齐亮 Timeout=0x2020 Option=0x87。
    #[test]
    fn led_light_golden() {
        let mut buf = FrameBuf::new();
        LedLightRequest {
            timeout_ms: 0x2020,
            option: 0x87,
        }
        .encode(&mut buf)
        .unwrap();
        assert_eq!(
            hex(&buf),
            "FF 11 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 51 20 20 87 C2 BB DC 50 "
        );
    }

    /// 温度量程判定与换算（p.158）。
    #[test]
    fn temperature_conversion() {
        assert_eq!(temp_range_from_pc(0x0800), TempRange::Range30to97);
        assert_eq!(temp_range_from_pc(0x2948), TempRange::Range0to120);
        assert_eq!(temp_range_from_pc(0x3178), TempRange::Range40to120);
        assert_eq!(temp_range_from_pc(0x1234), TempRange::Unknown);
        // 0~120：整数不变，小数*100/256
        assert!((convert_temperature(TempRange::Range0to120, [25, 128]).unwrap() - 25.5).abs() < 0.01);
        // -30~97 正数：25 → -5
        assert!((convert_temperature(TempRange::Range30to97, [25, 0]).unwrap() - (-5.0)).abs() < 0.01);
        // -40~120：整数减 45
        assert!((convert_temperature(TempRange::Range40to120, [100, 0]).unwrap() - 55.0).abs() < 0.01);
    }
}
