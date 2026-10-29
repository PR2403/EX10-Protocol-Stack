//! 三种 CRC（全部自实现，禁止引入标准 CRC 库）。
//!
//! 1. [`crc16_ex10`]：帧 CRC，手册附录 1 的**私有非标准**逐位算法。
//!    ⚠️ 2026-10-29 真机 + 手册 40+ 帧双重验证：数据位插入寄存器 **bit0**
//!    （标准 CRC 经 `reg ^= byte<<8` 从 bit8 进入），不等价于任何标准参数
//!    组合（CCITT-FALSE/MODBUS/XMODEM 均不匹配，65536 poly 暴力搜索无解），
//!    `crc` crate 不可用。
//! 2. [`tag_crc`]：附录 5，PC+EPC 的 ISO6C 标准 CRC（取反输出）。
//! 3. [`sub_crc`]：附录 7，扩展指令累加和（SubCode 起至 SubData 末尾）。

/// 帧 CRC-16（手册附录 1 逐位直译）。
///
/// 输入 = 帧内从 Len 起到 Data 末尾（**帧头 0xFF 不参与**）；
/// 输出大端附加帧尾。
pub fn crc16_ex10(data: &[u8]) -> u16 {
    let mut reg: u32 = 0xFFFF;
    for &byte in data {
        let mut mask = 0x80u8;
        while mask != 0 {
            let xor_flag = reg & 0x8000 != 0;
            reg = (reg << 1) & 0xFFFF;
            if byte & mask != 0 {
                reg |= 1; // ★ 关键差异：数据位进 bit0
            }
            if xor_flag {
                reg ^= 0x1021;
            }
            mask >>= 1;
        }
    }
    reg as u16
}

/// TagCRC（手册附录 5）：ISO6C 标准算法，输入 = PC+EPC，输出取反。
pub fn tag_crc(data: &[u8]) -> u16 {
    let mut crc: u32 = 0xFFFF;
    for &byte in data {
        let mut mask = 0x80u8;
        while mask != 0 {
            if crc & 0x8000 != 0 {
                crc = ((crc << 1) ^ 0x1021) & 0xFFFF;
            } else {
                crc = (crc << 1) & 0xFFFF;
            }
            if byte & mask != 0 {
                crc ^= 0x1021;
            }
            mask >>= 1;
        }
    }
    !(crc as u16)
}

/// SubCRC（手册附录 7）：SubCode(2B) 起至 SubData 末尾逐字节累加和低 8 位。
///
/// 注意计算范围**含 SubCode 本身**：`AA 40 04 00` → `0xEE`（真机实测）。
pub fn sub_crc(data: &[u8]) -> u8 {
    let mut sum: u8 = 0;
    for &b in data {
        sum = sum.wrapping_add(b);
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真机版本响应帧 26 字节（AGENTS.md 权威向量）。
    const RESP_BODY: [u8; 24] = [
        0x14, 0x03, 0x00, 0x00, 0x18, 0x06, 0x04, 0x00, 0xA9, 0x00, 0x00, 0x01, 0x20, 0x20, 0x07,
        0x03, 0x20, 0x07, 0x03, 0x00, 0x00, 0x00, 0x00, 0x10,
    ];

    #[test]
    fn frame_crc_golden() {
        // `FF 00 03` -> `1D 0C`（附录 1 + 真机 OK）
        assert_eq!(crc16_ex10(&[0x00, 0x03]), 0x1D0C);
        // 真机响应帧 -> `B0 31`
        assert_eq!(crc16_ex10(&RESP_BODY), 0xB031);
        // 反向验证非标准性：同数据的 CCITT-FALSE = 0x3F5D ≠ 0xB031
        assert_ne!(crc16_ex10(&RESP_BODY), 0x3F5D);
    }

    #[test]
    fn tag_crc_golden() {
        // PC=3000 + EPC=050000000000000000002354 -> 4AC8（附录 5）
        let pcepc = [
            0x30, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x23, 0x54,
        ];
        assert_eq!(tag_crc(&pcepc), 0x4AC8);
    }

    #[test]
    fn sub_crc_golden() {
        assert_eq!(sub_crc(&[0xAA, 0x40, 0x04, 0x00]), 0xEE);
        assert_eq!(sub_crc(&[0xAA, 0x40, 0xAA, 0x01]), 0x95);
        assert_eq!(sub_crc(&[0xAA, 0x49]), 0xF3);
        assert_eq!(sub_crc(&[0xAA, 0x59]), 0x03);
        assert_eq!(sub_crc(&[0xAA, 0x40, 0x06, 0x00]), 0xF0);
        assert_eq!(sub_crc(&[0xAA, 0x40, 0xAB, 0x01]), 0x96);
        assert_eq!(sub_crc(&[0xAA, 0x4A, 0x0B, 0xB8, 0x01, 0x01, 0x00]), 0xB9);
    }
}
