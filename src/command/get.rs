//! 获取指令（手册第 8 章，p.113-130）。

use crate::error::DecodeError;
use crate::frame::RawFrame;
use crate::types::region::RegionCode;

/// 0x61 查询天线配置的 Option（p.114）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AntennaQueryOption {
    /// 0x00 标签访问操作使用的天线。
    Access = 0x00,
    /// 0x02 盘存使用的逻辑天线。
    Inventory = 0x02,
    /// 0x03 所有逻辑天线的读写功率。
    Power = 0x03,
    /// 0x04 读写功率 + 配置时间。
    PowerAndTime = 0x04,
    /// 0x05 天线连接状态（判别闭环天线）。
    ConnectionStatus = 0x05,
}

impl AntennaQueryOption {
    pub const fn value(self) -> u8 {
        self as u8
    }
}

/// 天线功率配置信息（0x61 Option=3/4 响应项，p.115）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AntennaPowerInfo {
    pub antenna: u8,
    pub read_power: u16,
    pub write_power: u16,
    /// 仅 Option=4。
    pub setting_time_us: Option<u16>,
}

/// 天线连接状态（0x61 Option=5 响应项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AntennaConnection {
    pub antenna: u8,
    /// 1=连接，0=未连接。
    pub connected: bool,
}

/// 0x61 响应（按查询 Option 分形态，p.115）。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AntennaConfigResponse {
    /// Option=0：1 对 TX/RX。
    Access { tx: u8, rx: u8 },
    /// Option=2：N 对 TX/RX。
    Inventory(heapless::Vec<(u8, u8), 32>),
    /// Option=3/4。
    Power(heapless::Vec<AntennaPowerInfo, 32>),
    /// Option=5。
    Connections(heapless::Vec<AntennaConnection, 32>),
}

/// 0x62 功率信息响应（p.118-119）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PowerInfoResponse {
    /// 上电默认读发射功率（不建议使用，实际值看 0x61）。
    pub default_read_power: u16,
    /// Option=1 时存在。
    pub max_power: Option<u16>,
    pub min_power: Option<u16>,
}

/// 0x65 响应（p.121-122）。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrequencyHopResponse {
    /// 跳频表（kHz）。
    Table(heapless::Vec<u32, 50>),
    /// Option=2 天线驻留时间（ms）。
    DwellTime { timeout_ms: u32 },
}

/// 0x6A 读写器配置响应（p.125）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaderConfigResponse {
    pub key: u8,
    pub value: u8,
}

/// 0x6B 协议配置响应（p.126-127）：3 字节（无 Option）或 4 字节形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtocolConfigResponse {
    pub parameter: u8,
    pub option: Option<u8>,
    pub value: u8,
}

// ---------------------------------------------------------------------------
// 请求编码（Data 域）
// ---------------------------------------------------------------------------

/// 0x61 请求 Data。
pub fn encode_antenna_query<const N: usize>(option: AntennaQueryOption, out: &mut heapless::Vec<u8, N>) {
    let _ = out.push(option.value());
}

/// 0x62 请求 Data（0=仅默认功率，1=含最大/最小）。
pub fn encode_power_query<const N: usize>(with_range: bool, out: &mut heapless::Vec<u8, N>) {
    let _ = out.push(if with_range { 0x01 } else { 0x00 });
}

/// 0x65 请求驻留时间 Data；获取跳频表则 Data 为空。
pub fn encode_dwell_time_query<const N: usize>(out: &mut heapless::Vec<u8, N>) {
    let _ = out.push(0x02);
}

/// 0x6A 请求 Data。
pub fn encode_reader_config_query<const N: usize>(key: u8, out: &mut heapless::Vec<u8, N>) {
    let _ = out.push(0x01);
    let _ = out.push(key);
}

/// 0x6B 请求 Data。
pub fn encode_protocol_config_query<const N: usize>(protocol: u8, parameter: u8, out: &mut heapless::Vec<u8, N>) {
    let _ = out.push(protocol);
    let _ = out.push(parameter);
}

// ---------------------------------------------------------------------------
// 响应解析
// ---------------------------------------------------------------------------

/// 0x61 响应解析（五种形态由请求 Option 决定，p.115）。
pub fn parse_antenna_config(
    frame: &RawFrame<'_>,
    option: AntennaQueryOption,
) -> Result<AntennaConfigResponse, DecodeError> {
    let data = frame.ext_payload()?;
    Ok(match option {
        AntennaQueryOption::Access => {
            if data.len() < 2 {
                return Err(DecodeError::Truncated);
            }
            AntennaConfigResponse::Access {
                tx: data[0],
                rx: data[1],
            }
        }
        AntennaQueryOption::Inventory => {
            // Data[0] 为回显 Option，其后为 N 对 TX/RX
            let body = if data.is_empty() {
                return Err(DecodeError::Truncated);
            } else {
                &data[1..]
            };
            if body.len() % 2 != 0 {
                return Err(DecodeError::BadFrame);
            }
            let mut v = heapless::Vec::new();
            let mut pos = 0;
            while pos < body.len() {
                let _ = v.push((body[pos], body[pos + 1]));
                pos += 2;
            }
            AntennaConfigResponse::Inventory(v)
        }
        AntennaQueryOption::Power | AntennaQueryOption::PowerAndTime => {
            let stride = match option {
                AntennaQueryOption::Power => 5,
                _ => 7,
            };
            // Data[0] 为回显 Option，其后为 N 组
            if data.is_empty() {
                return Err(DecodeError::Truncated);
            }
            let body = &data[1..];
            if body.len() % stride != 0 {
                return Err(DecodeError::BadFrame);
            }
            let mut v = heapless::Vec::new();
            let mut pos = 0;
            while pos < body.len() {
                let b = &body[pos..pos + stride];
                pos += stride;
                let _ = v.push(AntennaPowerInfo {
                    antenna: b[0],
                    read_power: u16::from_be_bytes([b[1], b[2]]),
                    write_power: u16::from_be_bytes([b[3], b[4]]),
                    setting_time_us: if stride == 7 {
                        Some(u16::from_be_bytes([b[5], b[6]]))
                    } else {
                        None
                    },
                });
            }
            AntennaConfigResponse::Power(v)
        }
        AntennaQueryOption::ConnectionStatus => {
            // Data[0] 为回显 Option，其后为 N 对天线/状态
            let body = if data.is_empty() {
                return Err(DecodeError::Truncated);
            } else {
                &data[1..]
            };
            if body.len() % 2 != 0 {
                return Err(DecodeError::BadFrame);
            }
            let mut v = heapless::Vec::new();
            let mut pos = 0;
            while pos < body.len() {
                let _ = v.push(AntennaConnection {
                    antenna: body[pos],
                    connected: body[pos + 1] == 1,
                });
                pos += 2;
            }
            AntennaConfigResponse::Connections(v)
        }
    })
}

/// 0x62 响应解析（p.119）。
pub fn parse_power_info(frame: &RawFrame<'_>) -> Result<PowerInfoResponse, DecodeError> {
    let data = frame.ext_payload()?;
    match data.len() {
        3 => Ok(PowerInfoResponse {
            default_read_power: u16::from_be_bytes([data[1], data[2]]),
            max_power: None,
            min_power: None,
        }),
        7 => Ok(PowerInfoResponse {
            default_read_power: u16::from_be_bytes([data[1], data[2]]),
            max_power: Some(u16::from_be_bytes([data[3], data[4]])),
            min_power: Some(u16::from_be_bytes([data[5], data[6]])),
        }),
        _ => Err(DecodeError::BadFrame),
    }
}

/// 0x63/0x70 响应 → 当前/可用协议（恒 0x0005）。
pub fn parse_protocol(frame: &RawFrame<'_>) -> Result<u16, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() != 2 {
        return Err(DecodeError::BadFrame);
    }
    Ok(u16::from_be_bytes([data[0], data[1]]))
}

/// 0x65 响应解析（有 Option 域=驻留时间，否则为频表，p.121）。
pub fn parse_frequency_hop(frame: &RawFrame<'_>) -> Result<FrequencyHopResponse, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() == 5 {
        // Option(1) + HopTime(4)
        return Ok(FrequencyHopResponse::DwellTime {
            timeout_ms: u32::from_be_bytes([data[1], data[2], data[3], data[4]]),
        });
    }
    if data.len() % 4 == 0 && !data.is_empty() {
        let mut v = heapless::Vec::new();
        for c in data.chunks(4) {
            let _ = v.push(u32::from_be_bytes([c[0], c[1], c[2], c[3]]));
        }
        return Ok(FrequencyHopResponse::Table(v));
    }
    Err(DecodeError::BadFrame)
}

/// 0x66 响应 → 各 GPI 引脚电平。
pub fn parse_gpi(frame: &RawFrame<'_>) -> Result<heapless::Vec<u8, 16>, DecodeError> {
    Ok(frame.ext_payload()?.iter().copied().collect())
}

/// 0x67 响应 → 当前工作区域。
pub fn parse_region(frame: &RawFrame<'_>) -> Result<RegionCode, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() != 1 {
        return Err(DecodeError::BadFrame);
    }
    RegionCode::from_u8(data[0]).ok_or(DecodeError::BadValue("unknown region code"))
}

/// 0x6A 响应解析（p.125）。
pub fn parse_reader_config(frame: &RawFrame<'_>) -> Result<ReaderConfigResponse, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() != 3 {
        return Err(DecodeError::BadFrame);
    }
    Ok(ReaderConfigResponse {
        key: data[1],
        value: data[2],
    })
}

/// 0x6B 响应解析：Data = Protocol(1)+Parameter(1)+[Option(1)]+Value(1)。
pub fn parse_protocol_config(frame: &RawFrame<'_>) -> Result<ProtocolConfigResponse, DecodeError> {
    let data = frame.ext_payload()?;
    match data.len() {
        3 => Ok(ProtocolConfigResponse {
            parameter: data[1],
            option: None,
            value: data[2],
        }),
        4 => Ok(ProtocolConfigResponse {
            parameter: data[1],
            option: Some(data[2]),
            value: data[3],
        }),
        _ => Err(DecodeError::BadFrame),
    }
}

/// 0x71 响应 → 可用区域码列表。
pub fn parse_available_regions(
    frame: &RawFrame<'_>,
) -> Result<heapless::Vec<RegionCode, 32>, DecodeError> {
    let data = frame.ext_payload()?;
    let mut v = heapless::Vec::new();
    for &b in data {
        let r = RegionCode::from_u8(b).ok_or(DecodeError::BadValue("unknown region code"))?;
        let _ = v.push(r);
    }
    Ok(v)
}

/// 0x72 响应 → 模块温度（有符号 ℃，p.130）。
pub fn parse_temperature(frame: &RawFrame<'_>) -> Result<i8, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() != 1 {
        return Err(DecodeError::BadFrame);
    }
    Ok(data[0] as i8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(buf: &[u8]) -> RawFrame<'_> {
        RawFrame::parse(buf).unwrap()
    }

    #[test]
    fn antenna_access_golden() {
        // p.116 例1 响应: FF 02 61 00 00 03 03 4C 20 → TX=3 RX=3
        let f = frame(&[0xFF, 0x02, 0x61, 0x00, 0x00, 0x03, 0x03, 0x4C, 0x20]);
        let r = parse_antenna_config(&f, AntennaQueryOption::Access).unwrap();
        assert_eq!(r, AntennaConfigResponse::Access { tx: 3, rx: 3 });
    }

    #[test]
    fn antenna_inventory_golden() {
        // p.116 例2 响应：手册印 Len=05 但数据 6 字节（PDF 提取疑误），
        // 重构为 Len=05（Option+2对）并自算 CRC 验证解析逻辑。
        let mut b = heapless::Vec::<u8, 32>::new();
        let _ = b.extend_from_slice(&[0xFF, 0x05, 0x61, 0x00, 0x00, 0x02, 0x03, 0x03, 0x04, 0x04]);
        let c = crate::frame::crc::crc16_ex10(&b[1..]);
        let _ = b.push((c >> 8) as u8);
        let _ = b.push(c as u8);
        let f = frame(&b);
        let r = parse_antenna_config(&f, AntennaQueryOption::Inventory).unwrap();
        assert_eq!(
            r,
            AntennaConfigResponse::Inventory(heapless::Vec::from_iter([(3, 3), (4, 4)]))
        );
    }

    #[test]
    fn power_info_golden() {
        // p.119: FF 07 62 00 00 01 0C E4 0C E4 00 00 06 EF
        let f = frame(&[
            0xFF, 0x07, 0x62, 0x00, 0x00, 0x01, 0x0C, 0xE4, 0x0C, 0xE4, 0x00, 0x00, 0x06, 0xEF,
        ]);
        let r = parse_power_info(&f).unwrap();
        assert_eq!(r.default_read_power, 0x0CE4); // 3300 = 33dBm
        assert_eq!(r.max_power, Some(0x0CE4));
        assert_eq!(r.min_power, Some(0x0000));
    }

    #[test]
    fn freq_table_golden() {
        // p.122: FF 0C 65 00 00 00 0D F9 26 00 0D C8 52 00 0E 24 1E 2C B5
        let f = frame(&[
            0xFF, 0x0C, 0x65, 0x00, 0x00, 0x00, 0x0D, 0xF9, 0x26, 0x00, 0x0D, 0xC8, 0x52, 0x00,
            0x0E, 0x24, 0x1E, 0x2C, 0xB5,
        ]);
        let r = parse_frequency_hop(&f).unwrap();
        match r {
            FrequencyHopResponse::Table(t) => {
                // 响应示例为设备当前表：915750/903250/926750（与请求示例频点不同）
                assert_eq!(&t[..], &[915_750, 903_250, 926_750]);
            }
            _ => panic!("expect table"),
        }
    }

    #[test]
    fn gpi_region_golden() {
        // p.122 GPI: FF 02 66 00 00 00 01 CA B2 → [0, 1]
        let f = frame(&[0xFF, 0x02, 0x66, 0x00, 0x00, 0x00, 0x01, 0xCA, 0xB2]);
        assert_eq!(&parse_gpi(&f).unwrap()[..], &[0, 1]);
        // p.123 region: FF 01 67 00 00 01 B4 80 → NA
        let f = frame(&[0xFF, 0x01, 0x67, 0x00, 0x00, 0x01, 0xB4, 0x80]);
        assert_eq!(parse_region(&f).unwrap(), RegionCode::NorthAmerica);
    }

    #[test]
    fn reader_and_protocol_config_golden() {
        // p.125: FF 03 6A 00 00 01 00 01 3E 45
        let f = frame(&[0xFF, 0x03, 0x6A, 0x00, 0x00, 0x01, 0x00, 0x01, 0x3E, 0x45]);
        let r = parse_reader_config(&f).unwrap();
        assert_eq!((r.key, r.value), (0x00, 0x01));
        // p.127 Session: FF 03 6B 00 00 05 00 00 08 74
        let f = frame(&[0xFF, 0x03, 0x6B, 0x00, 0x00, 0x05, 0x00, 0x00, 0x08, 0x74]);
        let r = parse_protocol_config(&f).unwrap();
        assert_eq!(
            r,
            ProtocolConfigResponse {
                parameter: 0x00,
                option: None,
                value: 0x00
            }
        );
        // p.127 Target: FF 04 6B 00 00 05 01 01 00 2C 68
        let f = frame(&[
            0xFF, 0x04, 0x6B, 0x00, 0x00, 0x05, 0x01, 0x01, 0x00, 0x2C, 0x68,
        ]);
        let r = parse_protocol_config(&f).unwrap();
        assert_eq!(r.option, Some(0x01));
        assert_eq!(r.value, 0x00);
    }

    #[test]
    fn available_regions_golden() {
        // p.129: FF 04 71 00 00 01 06 08 FF DB 40 → NA/China1/CE_LOW/FullBand
        let f = frame(&[
            0xFF, 0x04, 0x71, 0x00, 0x00, 0x01, 0x06, 0x08, 0xFF, 0xDB, 0x40,
        ]);
        let r = parse_available_regions(&f).unwrap();
        assert_eq!(
            &r[..],
            &[
                RegionCode::NorthAmerica,
                RegionCode::China1,
                RegionCode::CeLow,
                RegionCode::FullBand
            ]
        );
    }

    #[test]
    fn temperature_golden() {
        // p.130: FF 01 72 00 00 27 48 20 → 39°C
        let f = frame(&[0xFF, 0x01, 0x72, 0x00, 0x00, 0x27, 0x48, 0x20]);
        assert_eq!(parse_temperature(&f).unwrap(), 39);
    }
}
