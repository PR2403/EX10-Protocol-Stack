//! L2 帧层：通用帧/扩展帧格式、三种 CRC、帧构建器、流式解码器。
//!
//! 线格式（手册 §3.1 p.14-15，全部大端）：
//! ```text
//! 请求: FF | Len | Op | Data[N]     | CRC16     N = Len
//! 响应: FF | Len | Op | Status(2) | Data[N] | CRC16
//! ```
//! 扩展帧（手册 §3.2 p.16）：请求 Data = "Moduletech"(10B) + SubCode(2B) + SubData + SubCRC(1B) + 0xBB；
//! 响应 Data = "Moduletech"(10B) + SubCode(2B) + SubData（无 SubCRC/Terminator）。

pub mod builder;
pub mod crc;
pub mod decoder;

use crate::error::DecodeError;

/// 帧头（不参与 CRC 计算）。
pub const FRAME_HEADER: u8 = 0xFF;
/// 扩展指令 Opcode（手册 §3.2 p.16）。
pub const EXT_OPCODE: u8 = 0xAA;
/// 扩展指令 Marker：ASCII "Moduletech"。
pub const EXT_MARKER: [u8; 10] = *b"Moduletech";
/// 扩展请求帧终止符。
pub const EXT_TERMINATOR: u8 = 0xBB;
/// 单帧总字节数硬上限（协议 §3 p.14）。
pub const MAX_FRAME_LEN: usize = 255;

/// 帧缓冲：解码器驻留 / 指令编码用（帧 ≤255，缓冲留 1 字节余量便于容错）。
pub type FrameBuf = heapless::Vec<u8, 256>;

/// 一条已通过 CRC 校验的完整帧（设备→主机形态：Op 之后固定有 2 字节 Status）。
///
/// `data` 借用自解码器缓冲或调用方字节切片；对扩展帧（opcode=0xAA），
/// `data` 仍含 Marker 壳，用 [`RawFrame::ext_payload`] 剥壳取 SubData。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawFrame<'a> {
    /// DataLength 域 = `data` 的字节数。
    pub length: u8,
    /// Opcode / Command Code。
    pub opcode: u8,
    /// Status Code（设备→主机帧必有；通知包固定 0x0000）。
    pub status: u16,
    /// Data 域原始字节。
    pub data: &'a [u8],
}

impl<'a> RawFrame<'a> {
    /// 解析一条完整帧并校验 CRC（用于黄金向量测试等已知完整字节的场景）。
    pub fn parse(buf: &'a [u8]) -> Result<RawFrame<'a>, DecodeError> {
        if buf.len() < 7 || buf[0] != FRAME_HEADER {
            return Err(DecodeError::BadFrame);
        }
        let length = buf[1] as usize;
        let total = 7 + length;
        if buf.len() != total {
            return Err(DecodeError::BadFrame);
        }
        let got = u16::from_be_bytes([buf[total - 2], buf[total - 1]]);
        if crc::crc16_ex10(&buf[1..total - 2]) != got {
            return Err(DecodeError::BadCrc);
        }
        Ok(RawFrame {
            length: buf[1],
            opcode: buf[2],
            status: u16::from_be_bytes([buf[3], buf[4]]),
            data: &buf[5..5 + length],
        })
    }

    /// 是否为扩展指令响应帧（opcode=0xAA 且 Data 以 "Moduletech" Marker 开头）。
    ///
    /// 响应侧 Data 字段 = Marker(10) + SubCode(2) + SubData(N)，**无前导 0xAA、
    /// 无 SubCRC/Terminator**（手册 §3.2 p.16、p.65/163 接收实例）。
    pub fn is_extended(&self) -> bool {
        self.opcode == EXT_OPCODE && self.data.len() >= 12 && self.data[0..10] == EXT_MARKER
    }

    /// 扩展帧 SubCode（非扩展帧返回 `None`）。
    pub fn subcode(&self) -> Option<u16> {
        if self.is_extended() {
            Some(u16::from_be_bytes([self.data[10], self.data[11]]))
        } else {
            None
        }
    }

    /// 剥扩展帧壳，返回 SubData（非扩展帧直接返回 `data`）。
    pub fn ext_payload(&self) -> Result<&'a [u8], DecodeError> {
        if !self.is_extended() {
            return Ok(self.data);
        }
        Ok(&self.data[12..])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手册 p.27 响应帧（真机同构帧见 AGENTS.md 向量 `B0 31`）。
    #[test]
    fn parse_response_frame() {
        let buf = [
            0xFF, 0x00, 0x09, 0x00, 0x00, 0x15, 0xE9,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        assert_eq!(f.opcode, 0x09);
        assert_eq!(f.status, 0x0000);
        assert!(f.data.is_empty());
        assert!(!f.is_extended());
    }

    #[test]
    fn parse_rejects_bad_crc() {
        let mut buf = [0xFF, 0x00, 0x09, 0x00, 0x00, 0x15, 0xE9];
        buf[5] ^= 0xFF;
        assert_eq!(RawFrame::parse(&buf), Err(DecodeError::BadCrc));
    }

    #[test]
    fn parse_rejects_wrong_length() {
        let buf = [0xFF, 0x00, 0x09, 0x00, 0x00, 0x15, 0xE9, 0x00];
        assert_eq!(RawFrame::parse(&buf), Err(DecodeError::BadFrame));
    }

    /// 手册 p.65 停止异步盘存的响应帧。
    #[test]
    fn parse_extended_response() {
        let buf = [
            0xFF, 0x0C, 0xAA, 0x00, 0x00, 0x4D, 0x6F, 0x64, 0x75, 0x6C, 0x65, 0x74, 0x65, 0x63,
            0x68, 0xAA, 0x49, 0x0F, 0x22,
        ];
        let f = RawFrame::parse(&buf).unwrap();
        assert!(f.is_extended());
        assert_eq!(f.subcode(), Some(0xAA49));
        assert!(f.ext_payload().unwrap().is_empty());
    }
}
