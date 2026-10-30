//! 最小上手例程：打开串口 →（Boot 层则自动进 APP）→ 读版本 → 盘存 → 读缓冲区。
//!
//! ```sh
//! cargo run --example quick_start -- [--port /dev/ttyUSB0]
//! ```

use uhf_ex10::command::bootloader::RunStage;
use uhf_ex10::transport::UhfReader;
use uhf_ex10::types::metadata::MetadataFlags;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/ttyUSB0".into());

    // 打开串口（115200 8N1 出厂默认）
    let mut reader = UhfReader::open(&port, 115_200)?;

    // Bootloader 层时先进 APP 层，盘存类指令仅在 APP 层可用（手册 §4）
    if reader.get_run_stage()? == RunStage::Boot {
        println!("处于 Bootloader 层，发送 0x04 进入 APP…");
        reader.boot_firmware()?;
    }

    // 0x03 版本信息
    let v = reader.get_version()?;
    println!(
        "芯片: {}  固件: {}.{}.{}.{}  天线口: {}",
        v.hw.chip,
        v.fw_ver[0],
        v.fw_ver[1],
        v.fw_ver[2],
        v.fw_ver[3],
        v.hw.antenna_ports()
    );

    // 0x22 同步盘存 500ms
    let found = reader.inventory(500)?;
    println!("盘存 500ms：发现 {found} 个标签");

    // 0x29 读缓冲区（带常用元数据：RSSI/天线/频点/读次/时间戳）
    if found > 0 {
        let tags = reader.read_tag_buffer(MetadataFlags::COMMON)?;
        for (i, t) in tags.iter().enumerate() {
            print!("  [{}] EPC=", i);
            for b in &t.epc {
                print!("{b:02X}");
            }
            if let Some(rssi) = t.rssi_dbm {
                print!("  RSSI={rssi}dBm");
            }
            if let Some(ant) = t.antenna_id {
                print!("  ANT={ant}");
            }
            println!();
        }
    }
    Ok(())
}
