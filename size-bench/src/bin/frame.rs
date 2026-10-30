//! 帧层基准:CRC×3 + 帧构建 + 流式解码器(含 RawFrame::parse)。

#![no_std]
#![no_main]

use uhf_ex10::frame::builder::{build_extended, build_request};
use uhf_ex10::frame::crc::{crc16_ex10, sub_crc, tag_crc};
use uhf_ex10::frame::decoder::StreamDecoder;
use uhf_ex10::frame::RawFrame;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    // 帧构建
    let mut buf = uhf_ex10::frame::FrameBuf::new();
    let n = build_request(0x10, &[0x00], &mut buf).ok().unwrap_or(0);
    // 扩展帧构建
    let _ = build_extended(0xAA40, &[0x04, 0x00], &mut buf);
    // 流式解码器:喂入刚构建的帧(含重同步路径)
    let mut dec = StreamDecoder::new();
    dec.feed_slice(&buf[..n]);
    dec.feed(0xFF); // 触发重同步扫描路径
    // 完整帧解析
    let parsed = RawFrame::parse(&buf[..n]);
    // 三种 CRC
    let (a, b, c) = (crc16_ex10(&buf), tag_crc(&buf), sub_crc(&buf));
    let x = a ^ b;
    let y = c as u16;
    match parsed {
        Ok(f) => (f.opcode as u16 ^ x ^ y) as i32,
        Err(_) => (x ^ y) as i32,
    }
}
