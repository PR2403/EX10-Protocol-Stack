//! 永久保存配置指令 0xAA40（手册第 9 章，p.130-148）。
//!
//! SubData = KEY(1) + GETORSET(1) + KEYDATA(N)；GET 0 / SET 1。
//! KEY=0x91/0x95 设置时 KEYDATA 为对应普通指令帧**去掉帧头 0xFF 与 CRC** 的部分
//! （即 Len+Op+Data，p.133/135）。

use crate::error::{DecodeError, EncodeError};
use crate::frame::{builder::build_extended, RawFrame};
use crate::types::baud::BaudRate;

/// 上电直达 APP 的魔数（p.142）。
pub const POWER_ON_TO_APP_MAGIC: u32 = 0xA5A5_5A5A;

/// 构建 0xAA40 读取指令（GETORSET=0）。
pub fn build_get(key: u8, keydata: &[u8], out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    let mut sub = heapless::Vec::<u8, 240>::new();
    let _ = sub.push(key);
    let _ = sub.push(0x00);
    let _ = sub.extend_from_slice(keydata);
    build_extended(0xAA40, &sub, out)
}

/// 构建 0xAA40 设置指令（GETORSET=1）。
pub fn build_set(key: u8, keydata: &[u8], out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    let mut sub = heapless::Vec::<u8, 240>::new();
    let _ = sub.push(key);
    let _ = sub.push(0x01);
    let _ = sub.extend_from_slice(keydata);
    build_extended(0xAA40, &sub, out)
}

/// 从一条完整普通指令帧提取 0xAA40 KEY=0x91/0x95 所需的 KEYDATA
/// （去掉帧头 0xFF 与尾部 CRC，保留 Len+Op+Data，p.133）。
pub fn keydata_from_frame(frame: &[u8]) -> Result<&[u8], EncodeError> {
    if frame.len() < 6 || frame[0] != crate::frame::FRAME_HEADER {
        return Err(EncodeError::InvalidParameter("bad frame"));
    }
    let len = frame[1] as usize;
    if frame.len() != 5 + len {
        return Err(EncodeError::InvalidParameter("frame length mismatch"));
    }
    // KEYDATA = Len + Op + Data（帧头 0xFF 与尾部 CRC 之外）
    Ok(&frame[1..3 + len])
}

/// 0xAA40 响应解析：SubData = KEY(1)+GETORSET(1)+[KEYDATA]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PersistResponse<'a> {
    pub key: u8,
    pub getorset: u8,
    pub keydata: &'a [u8],
}

pub fn parse_persist<'a>(frame: &'a RawFrame<'a>) -> Result<PersistResponse<'a>, DecodeError> {
    let data = frame.ext_payload()?;
    if data.len() < 2 {
        return Err(DecodeError::Truncated);
    }
    Ok(PersistResponse {
        key: data[0],
        getorset: data[1],
        keydata: &data[2..],
    })
}

// ---------------------------------------------------------------------------
// 便捷封装（每条都对应手册实例帧，见测试）
// ---------------------------------------------------------------------------

/// KEY=0x04 读取是否上电直达 APP。
pub fn build_power_on_to_app_get(out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    build_get(0x04, &[], out)
}

/// KEY=0x04 设置上电直达 APP（魔数 0xA5A55A5A）。
pub fn build_power_on_to_app_enable(out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    build_set(0x04, &POWER_ON_TO_APP_MAGIC.to_be_bytes(), out)
}

/// KEY=0x04 取消上电直达 APP（非魔数即取消）。
pub fn build_power_on_to_app_disable(out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    build_set(0x04, &0u32.to_be_bytes(), out)
}

/// KEY=0x06 读取上电默认波特率。
pub fn build_baudrate_get(out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    build_get(0x06, &[], out)
}

/// KEY=0x06 设置上电默认波特率。
pub fn build_baudrate_set(
    baud: BaudRate,
    out: &mut crate::frame::FrameBuf,
) -> Result<usize, EncodeError> {
    build_set(0x06, &baud.keydata(), out)
}

/// KEY=0x97 读取默认工作区域。
pub fn build_region_get(out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    build_get(0x97, &[], out)
}

/// KEY=0x97 设置默认工作区域（0x5A=恢复出厂）。
pub fn build_region_set(
    region: u8,
    out: &mut crate::frame::FrameBuf,
) -> Result<usize, EncodeError> {
    build_set(0x97, &[region], out)
}

/// KEY=0x9A 读取默认读写器配置（keydata = Option+Key 2 字节）。
pub fn build_reader_config_get(
    option: u8,
    key: u8,
    out: &mut crate::frame::FrameBuf,
) -> Result<usize, EncodeError> {
    build_get(0x9A, &[option, key], out)
}

/// KEY=0x9A 设置默认读写器配置（Option 恒 0x01）。
pub fn build_reader_config_set(
    key: u8,
    value: u8,
    out: &mut crate::frame::FrameBuf,
) -> Result<usize, EncodeError> {
    build_set(0x9A, &[0x01, key, value], out)
}

/// KEY=0x9B 读取默认协议配置（keydata = Protocol+Parameter）。
pub fn build_protocol_config_get(
    protocol: u8,
    parameter: u8,
    out: &mut crate::frame::FrameBuf,
) -> Result<usize, EncodeError> {
    build_get(0x9B, &[protocol, parameter], out)
}

/// KEY=0x9B 设置默认协议配置（keydata = Protocol+Parameter+[Option]+Value）。
pub fn build_protocol_config_set(
    keydata: &[u8],
    out: &mut crate::frame::FrameBuf,
) -> Result<usize, EncodeError> {
    build_set(0x9B, keydata, out)
}

/// KEY=0xAA 恢复出厂（p.146）。
pub fn build_factory_reset(out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    build_set(0xAA, &[], out)
}

/// KEY=0xAB 准备升级（发 0x09 回 Boot 前需先发此指令，p.147；0x09 成功后等 250ms）。
pub fn build_prepare_upgrade(out: &mut crate::frame::FrameBuf) -> Result<usize, EncodeError> {
    build_set(0xAB, &[], out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(buf: &[u8]) -> String {
        buf.iter().map(|b| format!("{b:02X} ")).collect()
    }

    /// 手册 p.143：FF 14 AA ..Moduletech.. AA 40 04 01 A5 A5 5A 5A ED BB 12 39

    #[test]
    fn power_on_to_app_golden() {
        let mut buf = crate::frame::FrameBuf::new();
        build_power_on_to_app_enable(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 14 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 40 04 01 A5 A5 5A 5A ED BB 12 39 "
        );
    }

    /// 手册 p.144：FF 10 AA ..Moduletech.. AA 40 06 00 F0 BB A0 51
    #[test]
    fn baudrate_get_golden() {
        let mut buf = crate::frame::FrameBuf::new();
        build_baudrate_get(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 10 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 40 06 00 F0 BB A0 51 "
        );
    }

    /// 手册 p.146 恢复出厂 / p.147 准备升级（帧均真机验证）。
    #[test]
    fn factory_reset_and_upgrade_golden() {
        let mut buf = crate::frame::FrameBuf::new();
        build_factory_reset(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 10 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 40 AA 01 95 BB 8D 63 "
        );
        let mut buf = crate::frame::FrameBuf::new();
        build_prepare_upgrade(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 10 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 40 AB 01 96 BB BD 52 "
        );
    }

    /// 手册 p.133-134：KEY=0x91 设置 4 天线功率 3300。
    /// KEYDATA = 完整 0x91 帧（Len+Op+Data）= `1D 91 04 <4 组天线功率>`。
    #[test]
    fn antenna_power_persist_golden() {
        // 先组 0x91 帧（Option=0x04 功率+SettingTime）
        let mut d = heapless::Vec::<u8, 64>::new();
        let _ = d.push(0x04);
        for ant in 1..=4u8 {
            let _ = d.push(ant);
            let _ = d.extend_from_slice(&3300u16.to_be_bytes());
            let _ = d.extend_from_slice(&3300u16.to_be_bytes());
            let _ = d.extend_from_slice(&0u16.to_be_bytes());
        }
        let mut frame = crate::frame::FrameBuf::new();
        crate::frame::builder::build_request(0x91, &d, &mut frame).unwrap();
        let keydata = keydata_from_frame(&frame).unwrap();
        assert_eq!(keydata[0], frame[1]); // Len
        assert_eq!(keydata[1], 0x91);
        let mut buf = crate::frame::FrameBuf::new();
        build_set(0x91, keydata, &mut buf).unwrap();
        let hexs = hex(&buf);
        assert_eq!(
            hexs,
            "FF 2F AA 4D 6F 64 75 6C 65 74 65 63 68 AA 40 91 01 1D 91 04 01 0C E4 0C E4 00 00 \
             02 0C E4 0C E4 00 00 03 0C E4 0C E4 00 00 04 0C E4 0C E4 00 00 B8 BB CE FB "
        );
    }

    /// 0xAA40 响应解析（p.144 接收帧）。
    #[test]
    fn parse_persist_golden() {
        // FF 12 AA 00 00 MT AA 40 06 00 000E1000 + CRC 9C5F
        let buf: [u8; 25] = [
            0xFF, 0x12, 0xAA, 0x00, 0x00, 0x4D, 0x6F, 0x64, 0x75, 0x6C, 0x65, 0x74, 0x65, 0x63,
            0x68, 0xAA, 0x40, 0x06, 0x00, 0x00, 0x0E, 0x10, 0x00, 0x9C, 0x5F,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        assert_eq!(f.subcode(), Some(0xAA40));
        let r = parse_persist(&f).unwrap();
        assert_eq!(r.key, 0x06);
        assert_eq!(r.getorset, 0x00);
        assert_eq!(r.keydata, &[0x00, 0x0E, 0x10, 0x00]);
        assert_eq!(BaudRate::from_u32(0x000E_1000), Some(BaudRate::Baud921600));
    }
}
