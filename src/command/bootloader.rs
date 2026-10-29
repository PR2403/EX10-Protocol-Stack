//! Bootloader / 通用系统指令（手册第 4 章，p.19-31）。
//!
//! 0x03/0x04/0x09/0x0C/0x10 在两层均可用；0x01/0x02/0x08 仅 Bootloader 层。

use crate::error::{DecodeError, EncodeError};
use crate::frame::RawFrame;

/// 运行阶段（0x0C，p.29）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunStage {
    /// 0x11 Bootloader 层
    Boot,
    /// 0x12 APP Firmware 层
    App,
}

impl RunStage {
    pub const fn from_u8(v: u8) -> Option<Self> {
        match v {
            0x11 => Some(RunStage::Boot),
            0x12 => Some(RunStage::App),
            _ => None,
        }
    }
}

/// 芯片类型（Hardware Version 第 1 字节，p.23）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipType {
    E710,
    E510,
    E310,
    E910,
    Unknown(u8),
}

impl ChipType {
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0x31 => ChipType::E710,
            0x32 => ChipType::E510,
            0x33 => ChipType::E310,
            0x34 => ChipType::E910,
            other => ChipType::Unknown(other),
        }
    }

    pub const fn describe(self) -> &'static str {
        match self {
            ChipType::E710 => "E710",
            ChipType::E510 => "E510",
            ChipType::E310 => "E310",
            ChipType::E910 => "E910",
            ChipType::Unknown(_) => "Unknown",
        }
    }
}

impl core::fmt::Display for ChipType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ChipType::Unknown(v) => write!(f, "Unknown({v:#04X})"),
            other => f.write_str(other.describe()),
        }
    }
}

/// 硬件版本拆解（p.23）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HwVersion {
    /// 第 1 字节：芯片类型。
    pub chip: ChipType,
    /// 第 2 字节：高 4 位模块类型，低 4 位天线口编码（0→1 口 … 5→32 口）。
    pub ant_port_raw: u8,
    /// 第 3 字节：认证区域码。
    pub cert_region: u8,
    /// 第 4 字节：版本更新序号。
    pub revision: u8,
}

impl HwVersion {
    /// 天线口数量（低 4 位编码 0→1,1→2,2→4,3→8,4→16,5→32）。
    pub const fn antenna_ports(&self) -> u32 {
        match self.ant_port_raw & 0x0F {
            0 => 1,
            1 => 2,
            2 => 4,
            3 => 8,
            4 => 16,
            5 => 32,
            _ => 1,
        }
    }
}

/// 版本信息（0x03/0x04 响应，p.23-24）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionInfo {
    /// Bootloader Version（4B，逐字节 nibble 对版本段）。
    pub boot_ver: [u8; 4],
    /// Hardware Version。
    pub hw: HwVersion,
    /// Firmware 编译日期（4B BCD yyyymmdd，如 `20 22 07 08`=2022-07-08）。
    pub fw_date: [u8; 4],
    /// Firmware Version（4B）。
    pub fw_ver: [u8; 4],
    /// 支持的标签协议（恒 0x00000010=Gen2）。
    pub protocol: u32,
}

impl VersionInfo {
    /// 解析 0x03/0x04 响应 Data（20 字节）。
    pub fn parse(data: &[u8]) -> Result<Self, DecodeError> {
        if data.len() != 20 {
            return Err(DecodeError::Truncated);
        }
        let mut boot_ver = [0u8; 4];
        boot_ver.copy_from_slice(&data[0..4]);
        let hw = HwVersion {
            chip: ChipType::from_u8(data[4]),
            ant_port_raw: data[5],
            cert_region: data[6],
            revision: data[7],
        };
        let mut fw_date = [0u8; 4];
        fw_date.copy_from_slice(&data[8..12]);
        let mut fw_ver = [0u8; 4];
        fw_ver.copy_from_slice(&data[12..16]);
        Ok(VersionInfo {
            boot_ver,
            hw,
            fw_date,
            fw_ver,
            protocol: u32::from_be_bytes([data[16], data[17], data[18], data[19]]),
        })
    }
}

/// 序列号（0x10 响应，p.30）：Year(4)+Serial(8)，数字为各字节低 nibble 拼接。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SerialNumber {
    /// 制造年份原始 4 字节（如 `02 00 02 03` → 2023）。
    pub raw_year: [u8; 4],
    /// 序列号原始 8 字节。
    pub raw_serial: [u8; 8],
}

impl SerialNumber {
    pub fn parse(data: &[u8]) -> Result<Self, DecodeError> {
        if data.len() != 12 {
            return Err(DecodeError::Truncated);
        }
        let mut raw_year = [0u8; 4];
        raw_year.copy_from_slice(&data[0..4]);
        let mut raw_serial = [0u8; 8];
        raw_serial.copy_from_slice(&data[4..12]);
        Ok(SerialNumber {
            raw_year,
            raw_serial,
        })
    }
}

/// 0x01 写 Flash 请求参数（p.19-20，仅 Bootloader 层，固件升级专用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteFlashRequest<'a> {
    /// 0=仍有后续数据，0xFF=最后一块。
    pub fin_flag: u8,
    /// 烧录地址，首块为 0x08008000。
    pub write_addr: u32,
    /// WRITEDATA（字节数须为 4 的倍数；WRITELEN=字节数/4，常规为 32）。
    pub write_data: &'a [u8],
}

impl WriteFlashRequest<'_> {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.write_data.is_empty() || !self.write_data.len().is_multiple_of(4) {
            return Err(EncodeError::InvalidParameter("write data must be multiples of 4"));
        }
        let word_len = self.write_data.len() / 4;
        if word_len > 255 {
            return Err(EncodeError::InvalidParameter("write length > 255 words"));
        }
        let _ = out.push(self.fin_flag);
        let _ = out.extend_from_slice(&self.write_addr.to_be_bytes());
        let _ = out.push(word_len as u8);
        let _ = out.extend_from_slice(self.write_data);
        Ok(())
    }
}

/// 0x02 读 Flash 请求（p.21）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadFlashRequest {
    /// 起始地址（最低 0x08008000）。
    pub read_addr: u32,
    /// 字数（0-32），读取字节数 = READLEN*4。
    pub read_len_words: u8,
}

impl ReadFlashRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.read_len_words > 32 {
            return Err(EncodeError::InvalidParameter("read len > 32 words"));
        }
        let _ = out.extend_from_slice(&self.read_addr.to_be_bytes());
        let _ = out.push(self.read_len_words);
        Ok(())
    }
}

/// 0x08 校验 Firmware 请求（p.25-26）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyFirmwareRequest {
    /// 校验起址（必须 0x08008000）。
    pub check_addr: u32,
    /// 0x01 写入的总字节数 / 4。
    pub check_data_len: u32,
    /// CHECKCRC，用 [`check_crc`] 由写入数据算出。
    pub check_crc: u32,
}

impl VerifyFirmwareRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        let _ = out.extend_from_slice(&self.check_addr.to_be_bytes());
        let _ = out.extend_from_slice(&self.check_data_len.to_be_bytes());
        let _ = out.extend_from_slice(&self.check_crc.to_be_bytes());
        Ok(())
    }
}

/// CHECKCRC 组装（p.26）：写入数据按 4 字节分组，四列分别累加，取各列和的低 8 位
/// 拼成 32 位（DATA1 为最高字节）。
pub fn check_crc(written: &[u8]) -> u32 {
    let mut cols = [0u32; 4];
    for chunk in written.chunks(4) {
        for (i, &b) in chunk.iter().enumerate() {
            cols[i] += b as u32;
        }
    }
    ((cols[0] & 0xFF) << 24)
        | ((cols[1] & 0xFF) << 16)
        | ((cols[2] & 0xFF) << 8)
        | (cols[3] & 0xFF)
}

// ---------------------------------------------------------------------------
// 响应解析函数
// ---------------------------------------------------------------------------

/// 0x03/0x04 响应 → [`VersionInfo`]。
pub fn parse_version(frame: &RawFrame<'_>) -> Result<VersionInfo, DecodeError> {
    VersionInfo::parse(frame.ext_payload()?)
}

/// 0x0C 响应 → [`RunStage`]（p.29）。
pub fn parse_run_stage(frame: &RawFrame<'_>) -> Result<RunStage, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() != 1 {
        return Err(DecodeError::Truncated);
    }
    RunStage::from_u8(data[0]).ok_or(DecodeError::BadValue("unknown run stage"))
}

/// 0x10 响应 → [`SerialNumber`]。
pub fn parse_serial(frame: &RawFrame<'_>) -> Result<SerialNumber, DecodeError> {
    SerialNumber::parse(frame.ext_payload()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::RawFrame;

    /// 手册 p.24 获取版本响应（注意 Data 20 字节）。
    #[test]
    fn parse_version_golden() {
        let buf: [u8; 27] = [
            0xFF, 0x14, 0x03, 0x00, 0x00, 0x22, 0x02, 0x18, 0x00, 0x31, 0x00, 0x00, 0x00, 0x20,
            0x22, 0x07, 0x08, 0x22, 0x07, 0x08, 0x00, 0x00, 0x00, 0x00, 0x10, 0xFD, 0x54,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        assert_eq!(f.status, 0);
        let v = parse_version(&f).unwrap();
        assert_eq!(v.boot_ver, [0x22, 0x02, 0x18, 0x00]); // v2.2.18
        assert_eq!(v.hw.chip, ChipType::E710);
        assert_eq!(v.hw.antenna_ports(), 1); // 低 4 位 = 0 → 1 口
        assert_eq!(v.hw.cert_region, 0x00); // CHINA
        assert_eq!(v.fw_date, [0x20, 0x22, 0x07, 0x08]); // 2022-07-08
        assert_eq!(v.fw_ver, [0x22, 0x07, 0x08, 0x00]);
        assert_eq!(v.protocol, 0x0000_0010);
    }

    #[test]
    fn parse_version_real_device() {
        // 真机 2026-10-29 实测版本响应（AGENTS.md 权威向量）：24 字节主体 + CRC=B031
        let body: [u8; 24] = [
            0x14, 0x03, 0x00, 0x00, 0x18, 0x06, 0x04, 0x00, 0xA9, 0x00, 0x00, 0x01, 0x20, 0x20,
            0x07, 0x03, 0x20, 0x07, 0x03, 0x00, 0x00, 0x00, 0x00, 0x10,
        ];
        let c = crate::frame::crc::crc16_ex10(&body);
        assert_eq!(c, 0xB031);
        let mut full = [0u8; 27];
        full[0] = 0xFF;
        full[1..25].copy_from_slice(&body);
        full[25] = (c >> 8) as u8;
        full[26] = c as u8;
        let f = RawFrame::parse(&full).unwrap();
        let v = parse_version(&f).unwrap();
        assert_eq!(v.boot_ver, [0x18, 0x06, 0x04, 0x00]); // v1.8.6.4
        assert_eq!(v.hw.chip, ChipType::Unknown(0xA9)); // 真机芯片码 0xA9
        assert_eq!(v.fw_date, [0x20, 0x20, 0x07, 0x03]); // 2020-07-03
        assert_eq!(v.fw_ver, [0x20, 0x07, 0x03, 0x00]);
    }

    /// 手册 p.29 运行阶段响应。
    #[test]
    fn parse_run_stage_golden() {
        let buf = [0xFF, 0x01, 0x0C, 0x00, 0x00, 0x12, 0x63, 0x43];
        let f = RawFrame::parse(&buf).unwrap();
        assert_eq!(parse_run_stage(&f).unwrap(), RunStage::App);
    }

    /// 手册 p.31 序列号响应。
    #[test]
    fn parse_serial_golden() {
        let buf: [u8; 19] = [
            0xFF, 0x0C, 0x10, 0x00, 0x00, 0x02, 0x00, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01,
            0x02, 0x45, 0x09, 0xB5, 0xA4,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        let s = parse_serial(&f).unwrap();
        assert_eq!(s.raw_year, [0x02, 0x00, 0x02, 0x03]); // 2023
        assert_eq!(s.raw_serial, [0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x45, 0x09]);
    }

    #[test]
    fn encode_requests() {
        let mut out = heapless::Vec::<u8, 248>::new();
        // p.31 请求: Option/DataFlags 各 0
        let _ = out.push(0x00);
        let _ = out.push(0x00);
        assert_eq!(&out[..], &[0x00, 0x00]);

        // p.21 请求域组装
        let mut out = heapless::Vec::<u8, 248>::new();
        ReadFlashRequest {
            read_addr: 0x0800_8000,
            read_len_words: 1,
        }
        .encode(&mut out)
        .unwrap();
        assert_eq!(&out[..], &[0x08, 0x00, 0x80, 0x00, 0x01]);

        // p.26 0x08 请求（手册打印 CRC 有误，见 plan 勘误；只验证 Data 域）
        let mut out = heapless::Vec::<u8, 248>::new();
        VerifyFirmwareRequest {
            check_addr: 0x0800_8000,
            check_data_len: 0xA1,
            check_crc: 0x0205_080C,
        }
        .encode(&mut out)
        .unwrap();
        assert_eq!(
            &out[..],
            &[0x08, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0xA1, 0x02, 0x05, 0x08, 0x0C]
        );
    }

    #[test]
    fn check_crc_columns() {
        // 按列累加：数据 [01020304, 05060708] →
        // D1=01+05=06 D2=02+06=08 D3=03+07=0A D4=04+08=0C
        assert_eq!(check_crc(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]), 0x0608_0A0C);
    }
}
