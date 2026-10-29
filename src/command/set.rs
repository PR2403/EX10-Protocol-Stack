//! 设置指令（手册第 7 章，p.99-112）。

use crate::error::EncodeError;
use crate::types::region::RegionCode;

/// 0x91 天线端口/功率设置（p.99-101）。
///
/// 三种形态：无 Option 单口、Option=0x02 多口、Option=0x03/0x04 功率(+配置时间)。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetAntennaRequest {
    /// Option 省略或 0x00：设置标签访问操作使用的单天线（TX,RX 对）。
    AccessPort { tx: u8, rx: u8 },
    /// Option=0x02：设置盘存使用的多天线（≤32 对，工作顺序按物理口号升序）。
    InventoryPorts(heapless::Vec<(u8, u8), 32>),
    /// Option=0x03：只设置发射功率（0.01dBm）。
    Power {
        cfgs: heapless::Vec<AntennaPower, 32>,
    },
    /// Option=0x04：设置发射功率与天线配置时间（μs）。
    PowerAndSettingTime {
        cfgs: heapless::Vec<AntennaPower, 32>,
    },
}

/// TX Logical Antenna Power Configuration（p.100）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AntennaPower {
    /// TX 逻辑天线号。
    pub antenna: u8,
    /// 读功率（0.01dBm，实际精度 1dBm；30dBm = 3000 = 0x0BB8）。
    pub read_power: u16,
    /// 写功率（0.01dBm）。
    pub write_power: u16,
    /// 天线配置时间（μs，仅 Option=0x04 存在）。
    pub setting_time_us: u16,
}

impl SetAntennaRequest {
    /// 编码 Data 域（不含 Op）。
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        match self {
            SetAntennaRequest::AccessPort { tx, rx } => {
                let _ = out.push(0x00);
                let _ = out.push(*tx);
                let _ = out.push(*rx);
            }
            SetAntennaRequest::InventoryPorts(ports) => {
                if ports.is_empty() {
                    return Err(EncodeError::InvalidParameter("no antenna ports"));
                }
                let _ = out.push(0x02);
                for &(tx, rx) in ports {
                    let _ = out.push(tx);
                    let _ = out.push(rx);
                }
            }
            SetAntennaRequest::Power { cfgs } | SetAntennaRequest::PowerAndSettingTime { cfgs } => {
                if cfgs.is_empty() {
                    return Err(EncodeError::InvalidParameter("no antenna power cfgs"));
                }
                let _ = out.push(match self {
                    SetAntennaRequest::Power { .. } => 0x03,
                    _ => 0x04,
                });
                for c in cfgs {
                    let _ = out.push(c.antenna);
                    let _ = out.extend_from_slice(&c.read_power.to_be_bytes());
                    let _ = out.extend_from_slice(&c.write_power.to_be_bytes());
                    if matches!(self, SetAntennaRequest::PowerAndSettingTime { .. }) {
                        let _ = out.extend_from_slice(&c.setting_time_us.to_be_bytes());
                    }
                }
            }
        }
        Ok(())
    }
}

/// 0x93 设置标签协议（p.102）：恒 0x0005（GEN2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetProtocolRequest {
    pub protocol: u16,
}

impl SetProtocolRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.protocol != 0x0005 {
            return Err(EncodeError::InvalidParameter("only GEN2 0x0005 supported"));
        }
        let _ = out.extend_from_slice(&self.protocol.to_be_bytes());
        Ok(())
    }
}

/// 0x95 跳频/驻留时间设置（p.103-104）。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetFrequencyHopRequest {
    /// 跳频表（≤50 个频率，kHz）。
    FrequencyTable(heapless::Vec<u32, 50>),
    /// Option=0x02：天线驻留时间 20-60000ms（0x01 跳频时间目前无效）。
    DwellTime { timeout_ms: u32 },
}

impl SetFrequencyHopRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        match self {
            SetFrequencyHopRequest::FrequencyTable(freqs) => {
                if freqs.is_empty() {
                    return Err(EncodeError::InvalidParameter("empty frequency table"));
                }
                for &f in freqs {
                    let _ = out.extend_from_slice(&f.to_be_bytes());
                }
            }
            SetFrequencyHopRequest::DwellTime { timeout_ms } => {
                if !(20..=60000).contains(timeout_ms) {
                    return Err(EncodeError::InvalidParameter("dwell time 20..60000ms"));
                }
                let _ = out.push(0x02);
                let _ = out.extend_from_slice(&timeout_ms.to_be_bytes());
            }
        }
        Ok(())
    }
}

/// 0x96 设置 GPO（p.104-105，2 路输出）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetGpoRequest {
    /// (GPO 编号, 状态 0/1) 对。
    pub pairs: [(u8, u8); 2],
    /// 实际对数（1-2）。
    pub count: usize,
}

impl SetGpoRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.count == 0 || self.count > 2 {
            return Err(EncodeError::InvalidParameter("gpo pairs 1..=2"));
        }
        for &(num, status) in &self.pairs[..self.count] {
            let _ = out.push(num);
            let _ = out.push(status);
        }
        Ok(())
    }
}

/// 0x97 设置工作区域（p.106）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetRegionRequest {
    pub region: RegionCode,
}

impl SetRegionRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        let _ = out.push(self.region.value());
        Ok(())
    }
}

/// 0x9A 读写器配置键（p.108 键值表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderConfigKey {
    /// 0x00 天线端口是否作为标签缓冲唯一标识。
    AntennaAsBufferId,
    /// 0x06 是否记录最大 RSSI。
    MaxRssi,
    /// 0x08 bank 附加数据是否作为缓冲唯一标识。
    BankDataAsBufferId,
}

impl ReaderConfigKey {
    pub const fn value(self) -> u8 {
        match self {
            ReaderConfigKey::AntennaAsBufferId => 0x00,
            ReaderConfigKey::MaxRssi => 0x06,
            ReaderConfigKey::BankDataAsBufferId => 0x08,
        }
    }

    pub const fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0x00 => ReaderConfigKey::AntennaAsBufferId,
            0x06 => ReaderConfigKey::MaxRssi,
            0x08 => ReaderConfigKey::BankDataAsBufferId,
            _ => return None,
        })
    }
}

/// 0x9A 设置读写器配置（p.107-109）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetReaderConfigRequest {
    pub key: ReaderConfigKey,
    pub value: u8,
}

impl SetReaderConfigRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        let _ = out.push(0x01); // Option 恒 0x01
        let _ = out.push(self.key.value());
        let _ = out.push(self.value);
        Ok(())
    }
}

/// 0x9B 协议配置参数（p.110 协议参数选项表）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolConfigParam {
    /// 0x00 盘存 Session（0-3），无 Option 域。
    Session(u8),
    /// 0x01 Target：option=0x01 静态（value: 0=A/1=B），option=0x00 动态
    /// （value: 0=从 A 翻转，1=从 B 翻转）。
    Target { option: u8, value: u8 },
    /// 0x02 RF MODE（值 = MODE_ID + 100），无 Option 域。
    RfMode(u8),
    /// 0x12 Q：option=0x00 动态 Q（无 value）；option=0x01 静态 Q（0-15）。
    Q { option: u8, value: Option<u8> },
}

impl ProtocolConfigParam {
    pub const fn parameter(self) -> u8 {
        match self {
            ProtocolConfigParam::Session(_) => 0x00,
            ProtocolConfigParam::Target { .. } => 0x01,
            ProtocolConfigParam::RfMode(_) => 0x02,
            ProtocolConfigParam::Q { .. } => 0x12,
        }
    }
}

/// 0x9B 设置标签协议配置（p.109-112）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetProtocolConfigRequest {
    /// 恒 0x05。
    pub protocol: u8,
    pub param: ProtocolConfigParam,
}

impl SetProtocolConfigRequest {
    pub fn encode<const N: usize>(&self, out: &mut heapless::Vec<u8, N>) -> Result<(), EncodeError> {
        if self.protocol != 0x05 {
            return Err(EncodeError::InvalidParameter("only GEN2 supported"));
        }
        let _ = out.push(self.protocol);
        let _ = out.push(self.param.parameter());
        match self.param {
            ProtocolConfigParam::Session(v) => {
                if v > 3 {
                    return Err(EncodeError::InvalidParameter("session 0..=3"));
                }
                let _ = out.push(v);
            }
            ProtocolConfigParam::Target { option, value } => {
                let _ = out.push(option);
                let _ = out.push(value);
            }
            ProtocolConfigParam::RfMode(v) => {
                let _ = out.push(v);
            }
            ProtocolConfigParam::Q { option, value } => {
                if option == 0x01 {
                    let q = value.ok_or(EncodeError::InvalidParameter("static Q needs value"))?;
                    if q > 15 {
                        return Err(EncodeError::InvalidParameter("Q 0..=15"));
                    }
                    let _ = out.push(option);
                    let _ = out.push(q);
                } else {
                    let _ = out.push(0x00);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::builder::build_request;

    fn data_of(
        f: impl FnOnce(&mut heapless::Vec<u8, 248>) -> Result<(), EncodeError>,
    ) -> heapless::Vec<u8, 248> {
        let mut out = heapless::Vec::<u8, 248>::new();
        f(&mut out).unwrap();
        out
    }

    fn assert_frame(opcode: u8, data: &[u8], want_hex: &str) {
        let mut buf = crate::frame::FrameBuf::new();
        build_request(opcode, data, &mut buf).unwrap();
        let got: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(got, want_hex);
    }

    #[test]
    fn set_antenna_single_golden() {
        // p.100 例1: FF 03 91 00 01 01 62 87
        let d = data_of(|o| {
            SetAntennaRequest::AccessPort { tx: 1, rx: 1 }.encode(o)
        });
        assert_frame(0x91, &d, "FF 03 91 00 01 01 62 87 ");
    }

    #[test]
    fn set_antenna_power_golden() {
        // p.101 例3: FF 0B 91 03 02 03E8 0BB8 03 03E8 0BB8 F2 F5
        let mut cfgs = heapless::Vec::new();
        let _ = cfgs.push(AntennaPower {
            antenna: 2,
            read_power: 1000, // 0x03E8（手册例：读 10dBm）
            write_power: 3000,
            setting_time_us: 0,
        });
        let _ = cfgs.push(AntennaPower {
            antenna: 3,
            read_power: 1000,
            write_power: 3000,
            setting_time_us: 0,
        });
        let d = data_of(|o| SetAntennaRequest::Power { cfgs }.encode(o));
        assert_frame(
            0x91,
            &d,
            "FF 0B 91 03 02 03 E8 0B B8 03 03 E8 0B B8 F2 F5 ",
        );
    }

    #[test]
    fn set_protocol_golden() {
        // p.102: FF 02 93 00 05 51 7D
        let d = data_of(|o| {
            SetProtocolRequest { protocol: 0x0005 }.encode(o)
        });
        assert_frame(0x93, &d, "FF 02 93 00 05 51 7D ");
    }

    #[test]
    fn set_frequency_table_golden() {
        // p.104: FF 0C 95 00 0D F7 32 00 0D C8 52 00 0E 24 1E E5 24
        let mut freqs = heapless::Vec::new();
        let _ = freqs.push(915_250u32);
        let _ = freqs.push(903_250);
        let _ = freqs.push(926_750);
        let d = data_of(|o| SetFrequencyHopRequest::FrequencyTable(freqs).encode(o));
        assert_frame(
            0x95,
            &d,
            "FF 0C 95 00 0D F7 32 00 0D C8 52 00 0E 24 1E E5 24 ",
        );
    }

    #[test]
    fn set_gpo_golden() {
        // p.105 例1: FF 04 96 01 01 02 00 2F 68
        let d = data_of(|o| {
            SetGpoRequest {
                pairs: [(1, 1), (2, 0)],
                count: 2,
            }
            .encode(o)
        });
        assert_frame(0x96, &d, "FF 04 96 01 01 02 00 2F 68 ");
    }

    #[test]
    fn set_region_golden() {
        // p.106 实例1: FF 01 97 01 4B BC；实例2: FF 01 97 06 4B BB
        let d = data_of(|o| {
            SetRegionRequest {
                region: RegionCode::NorthAmerica,
            }
            .encode(o)
        });
        assert_frame(0x97, &d, "FF 01 97 01 4B BC ");
        let d = data_of(|o| {
            SetRegionRequest {
                region: RegionCode::China1,
            }
            .encode(o)
        });
        assert_frame(0x97, &d, "FF 01 97 06 4B BB ");
    }

    #[test]
    fn set_reader_config_golden() {
        // p.108: FF 03 9A 01 00 01 AF 5C
        let d = data_of(|o| {
            SetReaderConfigRequest {
                key: ReaderConfigKey::AntennaAsBufferId,
                value: 0x01,
            }
            .encode(o)
        });
        assert_frame(0x9A, &d, "FF 03 9A 01 00 01 AF 5C ");
    }

    #[test]
    fn set_protocol_config_golden() {
        // p.111 实例1 Session1: FF 03 9B 05 00 01 DC E9
        let d = data_of(|o| {
            SetProtocolConfigRequest {
                protocol: 0x05,
                param: ProtocolConfigParam::Session(1),
            }
            .encode(o)
        });
        assert_frame(0x9B, &d, "FF 03 9B 05 00 01 DC E9 ");
        // 实例2 静态 Q=3: FF 04 9B 05 12 01 03 80 AC
        let d = data_of(|o| {
            SetProtocolConfigRequest {
                protocol: 0x05,
                param: ProtocolConfigParam::Q {
                    option: 0x01,
                    value: Some(3),
                },
            }
            .encode(o)
        });
        assert_frame(0x9B, &d, "FF 04 9B 05 12 01 03 80 AC ");
        // 实例3 Target B: FF 04 9B 05 01 01 01 A2 FC
        let d = data_of(|o| {
            SetProtocolConfigRequest {
                protocol: 0x05,
                param: ProtocolConfigParam::Target {
                    option: 0x01,
                    value: 0x01,
                },
            }
            .encode(o)
        });
        assert_frame(0x9B, &d, "FF 04 9B 05 01 01 01 A2 FC ");
    }

    #[test]
    fn rejects_bad_values() {
        let mut out = heapless::Vec::<u8, 248>::new();
        assert!(SetProtocolRequest { protocol: 0x0006 }.encode(&mut out).is_err());
        assert!(
            SetFrequencyHopRequest::DwellTime { timeout_ms: 10 }
                .encode(&mut out)
                .is_err()
        );
        assert!(
            SetProtocolConfigRequest {
                protocol: 0x05,
                param: ProtocolConfigParam::Session(9),
            }
            .encode(&mut out)
            .is_err()
        );
        assert!(
            SetProtocolConfigRequest {
                protocol: 0x05,
                param: ProtocolConfigParam::Q {
                    option: 0x01,
                    value: Some(16),
                },
            }
            .encode(&mut out)
            .is_err()
        );
    }
}
