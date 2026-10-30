//! 标签访问例程：读 TID → 写/读 USER 区 → 功率调整。
//!
//! ```sh
//! cargo run --example tag_access -- [--port /dev/ttyUSB0]
//! ```
//!
//! ⚠️ 写 USER 区会永久修改标签数据；`WRITE` 设为 false 可只读不写。

use uhf_ex10::transport::UhfReader;
use uhf_ex10::types::bank::MemBank;

/// 是否执行写入演示（false = 只读）。
const WRITE: bool = false;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/ttyUSB0".into());
    let mut reader = UhfReader::open(&port, 115_200)?;

    // 0x28 读 TID Bank 前 4 字（8 字节）：厂商 ID + 模型
    let tid = reader.read_memory(MemBank::Tid, 0x0000_0002, 4)?;
    print!("TID: ");
    for b in &tid {
        print!("{b:02X}");
    }
    println!();

    // 0x28 读 USER 区前 2 字
    let user = reader.read_memory(MemBank::User, 0, 2)?;
    print!("USER(读前): ");
    for b in &user {
        print!("{b:02X}");
    }
    println!();

    // 0x24 写 USER 区（16 进制串）
    if WRITE {
        let payload: &[u8] = &[0xCA, 0xFE];
        reader.write_memory(MemBank::User, 0, payload)?;
        println!("USER(写入): CAFE");

        let back = reader.read_memory(MemBank::User, 0, 2)?;
        print!("USER(读回): ");
        for b in &back {
            print!("{b:02X}");
        }
        println!();
    }

    // 0x91 天线 1 功率设置为读 30dBm(3000)/写 30dBm(单位 0.01dBm)
    reader.set_antenna_power(1, 3000, 3000)?;
    println!("天线 1 功率已设为 30dBm");
    Ok(())
}
