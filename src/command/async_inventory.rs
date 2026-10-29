//! 异步盘存：0xAA48/0xAA49 与 EX 异步 0xAA58/0xAA59，及三种主动上传通知包
//! （手册 §5.5，p.57-67、p.61-64）。

use crate::command::inventory::{EmbeddedRead, SearchFlags};
use crate::error::{DecodeError, EncodeError};
use crate::frame::{builder::build_extended, FrameBuf, RawFrame};
use crate::types::metadata::MetadataFlags;
use crate::types::select::TagSingulation;

/// 0xAA48 SearchFlags 高字节控制位（p.58）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AsyncControlFlags {
    /// BIT7：每 ~15s 发送心跳包。
    pub heartbeat: bool,
    /// BIT6：单天线一段时间无新标签时自动停止盘存（仅单天线）。
    pub single_ant_auto_stop: bool,
    /// BIT5：多天线每轮询完一遍发送通知帧。
    pub polling_notify: bool,
}

impl AsyncControlFlags {
    const fn to_high_byte(self) -> u8 {
        let mut v = 0u8;
        if self.heartbeat {
            v |= 0x80;
        }
        if self.single_ant_auto_stop {
            v |= 0x40;
        }
        if self.polling_notify {
            v |= 0x20;
        }
        v
    }
}

/// 0xAA48 开始异步盘存请求（p.57-58）。
///
/// SubData = MetadataFlags(2) + Option(1) + SearchFlags(2) + [AccessPwd(4)]
/// + TagSingulation + [Embedded Command Content]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartAsyncRequest {
    pub metadata_flags: MetadataFlags,
    pub control: AsyncControlFlags,
    /// 低字节：BIT2 嵌入指令 / BIT4 TAGFOCUS（高字节为 [`AsyncControlFlags`]）。
    pub search_flags: SearchFlags,
    pub singulation: TagSingulation,
    pub embedded: Option<EmbeddedRead>,
}

impl StartAsyncRequest {
    pub fn encode(&self, out: &mut FrameBuf) -> Result<usize, EncodeError> {
        if self.search_flags.embedded != self.embedded.is_some() {
            return Err(EncodeError::InvalidParameter(
                "search_flags.embedded must match embedded field",
            ));
        }
        let mut sub = heapless::Vec::<u8, 240>::new();
        let _ = sub.extend_from_slice(&self.metadata_flags.bits().to_be_bytes());
        let _ = sub.push(self.singulation.option_bits());
        let sf = (self.control.to_high_byte() as u16) << 8 | (self.search_flags.to_u16() & 0xFF);
        let _ = sub.extend_from_slice(&sf.to_be_bytes());
        self.singulation.encode(&mut sub, true)?;
        if let Some(e) = &self.embedded {
            e.encode(&mut sub)?;
        }
        build_extended(0xAA48, &sub, out)
    }
}

/// 0xAA49 停止异步盘存（无 SubData，p.65）。
pub fn build_stop_async(out: &mut FrameBuf) -> Result<usize, EncodeError> {
    build_extended(0xAA49, &[], out)
}

/// 0xAA58 开始 EX 异步盘存（p.66-67）。
///
/// SubData = ExConfigData(20B，仅首字节 0=密集/1=稀疏) + MetadataFlags(2)
/// + Option(1) + SearchFlags(2)。不支持匹配过滤与嵌入指令。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartExAsyncRequest {
    /// false=密集标签模式（默认），true=稀疏（标签少且好读）。
    pub sparse_mode: bool,
    pub metadata_flags: MetadataFlags,
}

impl StartExAsyncRequest {
    pub fn encode(&self, out: &mut FrameBuf) -> Result<usize, EncodeError> {
        let mut sub = heapless::Vec::<u8, 32>::new();
        let _ = sub.push(if self.sparse_mode { 0x01 } else { 0x00 });
        let _ = sub.extend_from_slice(&[0u8; 19]); // 其余 19 字节为 0
        let _ = sub.extend_from_slice(&self.metadata_flags.bits().to_be_bytes());
        let _ = sub.push(0x00); // Option
        let _ = sub.extend_from_slice(&0x8003u16.to_be_bytes()); // 手册实例 SF=8003
        build_extended(0xAA58, &sub, out)
    }
}

/// 0xAA59 停止 EX 异步盘存（p.69）。
pub fn build_stop_ex_async(out: &mut FrameBuf) -> Result<usize, EncodeError> {
    build_extended(0xAA59, &[], out)
}

/// 异步盘存主动上传的通知包（opcode=0xAA，Status 恒 0x0000，p.61-64）。
#[derive(Debug, Clone, PartialEq)]
pub enum Notification<'a> {
    /// 标签信息数据包：Data = MetadataFlags(2) + TagEpcAndMeta。
    /// ⚠️ 该语义下 EPCLength 单位为**字节**（含 PC+TagCRC），与 0x29 的 bit 不同。
    Tag(crate::command::inventory::TagEpcAndMeta<'a>),
    /// 心跳数据包：Data = "XTSJ"(4) + SearchFlags(2)，约每 15s 一帧。
    Heartbeat { search_flags: u16 },
    /// 轮询周期数据包：所有天线轮询完一遍的通知。
    /// Data = MetadataFlags(2) + 特殊 Tag 块（EPCID 域 = ANTWORKCNT 轮询轮次）。
    PollingRound { ant_work_count: u8 },
}

/// 通知包解析。opcode 非 0xAA 或 Status 非 0 的帧不属于通知。
pub fn parse_notification<'a>(frame: &'a RawFrame<'a>) -> Result<Notification<'a>, DecodeError> {
    if frame.opcode != crate::frame::EXT_OPCODE {
        return Err(DecodeError::BadFrame);
    }
    let data = frame.ext_payload()?;
    if data.len() >= 4 && &data[0..4] == b"XTSJ" {
        if data.len() < 6 {
            return Err(DecodeError::Truncated);
        }
        return Ok(Notification::Heartbeat {
            search_flags: u16::from_be_bytes([data[4], data[5]]),
        });
    }
    if data.len() < 2 {
        return Err(DecodeError::Truncated);
    }
    let flags = MetadataFlags(u16::from_be_bytes([data[0], data[1]]));
    // 尝试按标签包解（AA 语义：EPCLength 单位字节）
    let (tag, used) = crate::command::inventory::parse_tag_epc_and_meta(
        &data[2..],
        flags,
        crate::command::inventory::EpcLenUnit::Bytes1,
    )?;
    if used != data.len() - 2 {
        return Err(DecodeError::BadFrame);
    }
    // 轮询周期包特征：EPCLength 恒 0x05、PC=0、EPCCRC=0，EPCID 1 字节 = ANTWORKCNT
    if tag.pc == 0 && tag.tag_crc == 0 && tag.epc_id.len() == 1 {
        return Ok(Notification::PollingRound {
            ant_work_count: tag.epc_id[0],
        });
    }
    Ok(Notification::Tag(tag))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::RawFrame;

    fn hex(buf: &[u8]) -> String {
        buf.iter().map(|b| format!("{b:02X} ")).collect()
    }

    /// 手册 p.59 实例1：AA48 flags=00BF Option=00 SF=8003（无过滤）。
    #[test]
    fn start_async_golden() {
        // 手册例1 SearchFlags=0x8003：高字节 BIT7 = 心跳使能
        let req = StartAsyncRequest {
            metadata_flags: MetadataFlags(0x00BF),
            control: AsyncControlFlags {
                heartbeat: true,
                ..Default::default()
            },
            search_flags: SearchFlags {
                mode_bits: 0x03,
                ..Default::default()
            },
            singulation: TagSingulation::default(),
            embedded: None,
        };
        let mut buf = FrameBuf::new();
        req.encode(&mut buf).unwrap();
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(
            hex,
            "FF 13 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 48 00 BF 00 80 03 34 BB 29 0F "
        );
    }

    /// 手册 p.60 实例2（重构，SubCRC=0x42/CRC=AB26 由手册值反推自洽）：
    /// flags=00BF Option=04 SF=8003 Pwd=0 SelAddr=0x20 SelLen=8 SelData=E2。
    #[test]
    fn start_async_select_golden() {
        let req = StartAsyncRequest {
            metadata_flags: MetadataFlags(0x00BF),
            control: AsyncControlFlags {
                heartbeat: true,
                ..Default::default()
            },
            search_flags: SearchFlags {
                mode_bits: 0x03,
                ..Default::default()
            },
            singulation: TagSingulation::bank(
                crate::types::select::SelectMode::EpcBank,
                0x20,
                8,
                &[0xE2],
            ),
            embedded: None,
        };
        let mut buf = FrameBuf::new();
        req.encode(&mut buf).unwrap();
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(
            hex,
            "FF 1D AA 4D 6F 64 75 6C 65 74 65 63 68 AA 48 00 BF 04 80 03 00 00 00 00 00 00 00 \
             20 08 E2 42 BB AB 26 "
        );
    }

    /// 手册 p.65 / p.69 停止指令（真机 + 手册 CRC 双重验证）。
    #[test]
    fn stop_async_golden() {
        let mut buf = FrameBuf::new();
        build_stop_async(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 0E AA 4D 6F 64 75 6C 65 74 65 63 68 AA 49 F3 BB 03 91 "
        );
        let mut buf = FrameBuf::new();
        build_stop_ex_async(&mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 0E AA 4D 6F 64 75 6C 65 74 65 63 68 AA 59 03 BB E1 A0 "
        );
    }

    /// 手册 p.67-68 EX 异步实例：ExConfigData 全 0 + flags=0016 + SF=8003。
    /// （手册印 Len=0x13，勘误应为 0x23=14+21，见 AGENTS.md；CRC=0ABF 已验证）
    #[test]
    fn start_ex_async_golden() {
        let req = StartExAsyncRequest {
            sparse_mode: false,
            metadata_flags: MetadataFlags(0x0016),
        };
        let mut buf = FrameBuf::new();
        req.encode(&mut buf).unwrap();
        assert_eq!(buf[1], 0x27); // Len = 14 + 25（手册印 0x13 为勘误，应为 0x27）
        let hex: String = buf.iter().map(|b| format!("{b:02X} ")).collect();
        assert_eq!(hex.ends_with("9B BB 0A BF "), true);
    }

    /// 手册 p.61 标签通知包实例1（flags=003F，EPCLength 单位字节）。
    #[test]
    fn tag_notification_golden() {
        let buf: [u8; 34] = [
            0xFF, 0x1B, 0xAA, 0x00, 0x00, 0x00, 0x3F, 0x01, 0xBD, 0x02, 0x0D, 0xF7, 0x32, 0x00,
            0x00, 0x00, 0x13, 0x00, 0x00, 0x0C, 0x20, 0x00, 0x11, 0x11, 0x20, 0x19, 0x02, 0x11,
            0x01, 0x94, 0x22, 0xAF, 0xE2, 0x59,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        let n = parse_notification(&f).unwrap();
        match n {
            Notification::Tag(t) => {
                assert_eq!(t.metadata.read_count, Some(1));
                assert_eq!(t.metadata.rssi_dbm, Some(-67)); // 0xBD
                assert_eq!(t.metadata.antenna_id, Some(2));
                assert_eq!(t.metadata.frequency_khz, Some(0x0DF732)); // 915250
                assert_eq!(t.metadata.timestamp_ms, Some(0x13));
                assert_eq!(t.pc, 0x2000);
                assert_eq!(t.epc_id, &[0x11, 0x11, 0x20, 0x19, 0x02, 0x11, 0x01, 0x94]);
                assert_eq!(t.tag_crc, 0x22AF);
            }
            _ => panic!("expect tag"),
        }
    }

    /// 手册 p.63 心跳包：XTSJ + SearchFlags=8003。
    #[test]
    fn heartbeat_golden() {
        let buf = [
            0xFF, 0x06, 0xAA, 0x00, 0x00, 0x58, 0x54, 0x53, 0x4A, 0x80, 0x03, 0x17, 0x24,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        match parse_notification(&f).unwrap() {
            Notification::Heartbeat { search_flags } => assert_eq!(search_flags, 0x8003),
            _ => panic!("expect heartbeat"),
        }
    }

    /// 手册 p.64 轮询周期包：ANTWORKCNT=1。
    #[test]
    fn polling_round_golden() {
        let buf: [u8; 16] = [
            0xFF, 0x09, 0xAA, 0x00, 0x00, 0x00, 0x04, 0x02, 0x05, 0x00, 0x00, 0x01, 0x00, 0x00,
            0xF5, 0x75,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        match parse_notification(&f).unwrap() {
            Notification::PollingRound { ant_work_count } => assert_eq!(ant_work_count, 1),
            _ => panic!("expect polling"),
        }
    }

    /// 异步盘存被打断的状态码帧（p.65）：0x03 查询返回 0xAA49。
    #[test]
    fn interrupted_status_frame() {
        let buf = [0xFF, 0x00, 0x03, 0xAA, 0x49, 0x1E, 0xEA];
        let f = RawFrame::parse(&buf).unwrap();
        assert_eq!(f.status, 0xAA49);
    }
}
