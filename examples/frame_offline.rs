//! 帧层离线例程（无需设备）：构建请求帧、解析响应帧、体验流式解码器。
//!
//! ```sh
//! cargo run --example frame_offline
//! ```

use uhf_ex10::command::inventory::parse_single_inventory;
use uhf_ex10::frame::FrameBuf;
use uhf_ex10::frame::builder::{build_extended, build_request};
use uhf_ex10::frame::decoder::{Outcome, StreamDecoder};

fn hex(buf: &[u8]) -> String {
    buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ---- 1. 构建普通指令帧：0x03 获取版本 ------------------------------
    let mut frame = FrameBuf::new();
    build_request(0x03, &[], &mut frame)?;
    println!("0x03 请求帧: {}", hex(&frame));
    // 已验证向量：FF 00 03 1D 0C（AGENTS.md 真机实测）
    assert_eq!(&*frame, &[0xFF, 0x00, 0x03, 0x1D, 0x0C]);

    // ---- 2. 构建扩展指令帧：0xAA40 查询持久化参数 ----------------------
    let mut ext = FrameBuf::new();
    // SubData = KEY(1B)；KEY=0x04 查询当前区域
    build_extended(0xAA40, &[0x04], &mut ext)?;
    println!("0xAA40 请求帧: {}", hex(&ext));

    // ---- 3. 手册 p.44 实例 2：0x21 单标签盘存响应帧（含元数据，CRC=83D0）---
    let resp: &[u8] = &[
        0xFF, 0x16, 0x21, 0x00, 0x00, // Header Len Op Status
        0x10, 0x00, 0x14, // Option(BIT4=带元数据) + MetadataFlags=0x0014
        0x01, 0x00, 0xBB, 0x5F, 0x04, // AntID=1 + Timestamp(ms)
        0x01, 0x23, 0x45, 0x67, 0x89, 0xAB, 0xCD, 0xEF, 0x01, 0x23, 0x45, 0x67, // PC+EPC
        0xE6, 0xC8, // TagCRC
        0x83, 0xD0, // 帧尾 CRC-16（真机向量）
    ];
    // 演示：用流式解码器逐字节喂入（模拟串口粘包/断包场景）。
    // 注意：Outcome::Frame 借用解码器内部缓冲，须在本次迭代内完成解析。
    let mut dec = StreamDecoder::new();
    let mut epc = None;
    for &b in resp {
        if let Outcome::Frame(f) = dec.feed(b) {
            println!("收到帧: Op={:#04X} Status={:#06X}", f.opcode, f.status);
            let r = parse_single_inventory(&f)?;
            epc = Some(r.epc.to_vec());
        }
    }
    let epc = epc.expect("解码器应产出一帧");
    print!("单标签盘存 EPC=");
    for b in &epc {
        print!("{b:02X}");
    }
    println!();
    Ok(())
}
