//! 帧构建器：通用请求帧 + 扩展指令帧（手册 §3.1/§3.2）。

use super::{crc, EXT_MARKER, EXT_OPCODE, EXT_TERMINATOR, FRAME_HEADER, MAX_FRAME_LEN, FrameBuf};
use crate::error::EncodeError;

/// 构建**通用请求帧**（手册 §3.1 p.14-15）：
/// `FF | Len | Op | Data[N] | CRC16`，帧头 0xFF 不参与 CRC。
///
/// 编码前会清空 `out`；返回写入的总字节数（`4 + N + 2`）。
pub fn build_request(opcode: u8, data: &[u8], out: &mut FrameBuf) -> Result<usize, EncodeError> {
    // 总长 = FF(1) + Len(1) + Op(1) + Data(N) + CRC(2) = N + 4
    let total = 4 + data.len();
    if total > MAX_FRAME_LEN {
        return Err(EncodeError::FrameTooLong);
    }
    out.clear();
    // 容量已由 total<=255<=256 保证，push 不会失败
    let _ = out.push(FRAME_HEADER);
    let _ = out.push(data.len() as u8);
    let _ = out.push(opcode);
    for &b in data {
        let _ = out.push(b);
    }
    finish(out);
    Ok(total)
}

/// 构建**扩展指令帧**（手册 §3.2 p.16 + p.59/65/143/144/163 实例帧交叉验证）：
/// `FF | Len | AA | "Moduletech"(10B) | SubCode(2B) | SubData(N) | SubCRC(1B) | 0xBB | CRC16`
///
/// - Len = `14 + N`（**不计 Terminator 0xBB**，但 CRC 覆盖到 0xBB）
/// - SubCRC = SubCode 起至 SubData 末尾的累加和低 8 位（附录 7，**含 SubCode 本身**）
/// - 返回写入的总字节数（`19 + N`）
pub fn build_extended(
    subcode: u16,
    subdata: &[u8],
    out: &mut FrameBuf,
) -> Result<usize, EncodeError> {
    let total = 19 + subdata.len();
    if total > MAX_FRAME_LEN {
        return Err(EncodeError::FrameTooLong);
    }
    let sub = [(subcode >> 8) as u8, subcode as u8];

    out.clear();
    let _ = out.push(FRAME_HEADER);
    let _ = out.push((14 + subdata.len()) as u8);
    let _ = out.push(EXT_OPCODE);
    for &b in &EXT_MARKER {
        let _ = out.push(b);
    }
    let mut acc: u8 = 0;
    for &b in &sub {
        acc = acc.wrapping_add(b);
        let _ = out.push(b);
    }
    for &b in subdata {
        acc = acc.wrapping_add(b);
        let _ = out.push(b);
    }
    let _ = out.push(acc); // SubCRC
    let _ = out.push(EXT_TERMINATOR);
    finish(out);
    Ok(total)
}

/// 在缓冲末尾追加大端 CRC（对 out[1..] 计算）并补齐。
fn finish(out: &mut FrameBuf) {
    let c = crc::crc16_ex10(&out[1..]);
    let _ = out.push((c >> 8) as u8);
    let _ = out.push(c as u8);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(buf: &FrameBuf) -> String {
        buf.iter().map(|b| format!("{b:02X} ")).collect()
    }

    #[test]
    fn build_get_version() {
        // 手册 p.24: FF 00 03 1D 0C
        let mut buf = FrameBuf::new();
        build_request(0x03, &[], &mut buf).unwrap();
        assert_eq!(&buf[..], &[0xFF, 0x00, 0x03, 0x1D, 0x0C]);
    }

    #[test]
    fn build_stop_async() {
        // 手册 p.65: FF 0E AA ..Moduletech.. AA 49 F3 BB 03 91
        let mut buf = FrameBuf::new();
        build_extended(0xAA49, &[], &mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 0E AA 4D 6F 64 75 6C 65 74 65 63 68 AA 49 F3 BB 03 91 "
        );
    }

    #[test]
    fn build_power_on_to_app() {
        // 手册 p.143: FF 14 AA ..Moduletech.. AA 40 04 01 A5 A5 5A 5A ED BB 12 39
        let mut buf = FrameBuf::new();
        build_extended(0xAA40, &[0x04, 0x01, 0xA5, 0xA5, 0x5A, 0x5A], &mut buf).unwrap();
        assert_eq!(buf.len(), 25);
        assert_eq!(buf[1], 20); // Len = 14+N，不计 BB
        assert_eq!(buf[21], 0xED); // SubCRC = AA+40+04+01+A5+A5+5A+5A = 0x2ED
        assert_eq!(buf[22], 0xBB);
        assert_eq!(&buf[23..25], &[0x12, 0x39]);
    }

    #[test]
    fn build_baudrate_get() {
        // 手册 p.144: FF 10 AA ..Moduletech.. AA 40 06 00 F0 BB A0 51
        let mut buf = FrameBuf::new();
        build_extended(0xAA40, &[0x06, 0x00], &mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 10 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 40 06 00 F0 BB A0 51 "
        );
    }

    #[test]
    fn build_start_async_golden() {
        // 手册 p.59 实例1：AA 48 MetadataFlags=00BF Option=00 SearchFlags=8003
        // SubCRC = 0x34（累加和），CRC = 290F
        let mut buf = FrameBuf::new();
        build_extended(0xAA48, &[0x00, 0xBF, 0x00, 0x80, 0x03], &mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 13 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 48 00 BF 00 80 03 34 BB 29 0F "
        );
    }

    #[test]
    fn build_led_light_golden() {
        // 手册 p.163：0xAA51 Timeout=2020 Option=87，SubCRC=C2，CRC=DC50
        let mut buf = FrameBuf::new();
        build_extended(0xAA51, &[0x20, 0x20, 0x87], &mut buf).unwrap();
        assert_eq!(
            hex(&buf),
            "FF 11 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 51 20 20 87 C2 BB DC 50 "
        );
    }

    #[test]
    fn build_ext_oversize_rejected() {
        let mut buf = FrameBuf::new();
        let big = [0u8; 240];
        assert_eq!(
            build_extended(0xAA48, &big, &mut buf),
            Err(EncodeError::FrameTooLong)
        );
    }

    #[test]
    fn rejects_oversize() {
        let mut buf = FrameBuf::new();
        let big = [0u8; 252];
        assert_eq!(
            build_request(0x10, &big, &mut buf),
            Err(EncodeError::FrameTooLong)
        );
        // 251 字节 data 恰好到达上限（255 = MAX_FRAME_LEN）
        let ok = [0u8; 251];
        assert!(build_request(0x10, &ok, &mut buf).is_ok());
    }
}
