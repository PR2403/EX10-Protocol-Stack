//! 异步盘存例程：0xAA48 启动 → 收 10s 通知包（标签/心跳/轮询）→ 0xAA49 停止。
//!
//! ```sh
//! cargo run --example async_inventory -- [--port /dev/ttyUSB0]
//! ```

use std::collections::HashSet;
use std::time::{Duration, Instant};

use uhf_ex10::command::async_inventory::{
    AsyncControlFlags, Notification, parse_notification,
};
use uhf_ex10::command::inventory::SearchFlags;
use uhf_ex10::transport::{TransportError, UhfReader};
use uhf_ex10::types::metadata::MetadataFlags;
use uhf_ex10::types::select::TagSingulation;

/// 收通知包时长。
const RUN_SECONDS: u64 = 10;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/ttyUSB0".into());
    let mut reader = UhfReader::open(&port, 115_200)?;

    // 扩展指令帧由 Request 结构体直接 encode 成完整帧，transact_frame 发送
    let req = uhf_ex10::command::async_inventory::StartAsyncRequest {
        metadata_flags: MetadataFlags::COMMON,
        control: AsyncControlFlags {
            heartbeat: true,
            single_ant_auto_stop: false,
            polling_notify: true,
        },
        search_flags: SearchFlags {
            mode_bits: 0x03,
            ..Default::default()
        },
        singulation: TagSingulation::default(),
        embedded: None,
    };
    let mut frame = uhf_ex10::frame::FrameBuf::new();
    req.encode(&mut frame)?;
    reader.transact_frame(
        &frame,
        uhf_ex10::frame::EXT_OPCODE,
        Some(0xAA48),
        Duration::from_secs(5),
    )?;
    println!("异步盘存已启动，接收 {RUN_SECONDS}s 通知…");

    // 通知包接收循环
    let deadline = Instant::now() + Duration::from_secs(RUN_SECONDS);
    let (mut tags, mut hearts, mut rounds) = (0u32, 0u32, 0u32);
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut interrupted = false;

    while Instant::now() < deadline {
        let wait = (deadline - Instant::now()).min(Duration::from_millis(500));
        match reader.next_notification(wait) {
            Ok(rx) => {
                let view = rx.view();
                match parse_notification(&view)? {
                    Notification::Tag(t) => {
                        tags += 1;
                        seen.insert(t.epc_id.to_vec());
                        print!("  [标签] EPC=");
                        for b in t.epc_id {
                            print!("{b:02X}");
                        }
                        if let Some(rssi) = t.metadata.rssi_dbm {
                            print!("  RSSI={rssi}dBm");
                        }
                        println!();
                    }
                    Notification::Heartbeat { .. } => {
                        hearts += 1;
                        println!("  [心跳]");
                    }
                    Notification::PollingRound { ant_work_count } => {
                        rounds += 1;
                        println!("  [轮询完成] 第 {ant_work_count} 轮");
                    }
                }
            }
            // 超时 = 这 500ms 没有通知，继续等
            Err(TransportError::Timeout) => {}
            // 设备主动打断（状态 0xAA49/0xAA59）
            Err(TransportError::Status(s)) if s.value() == 0xAA49 || s.value() == 0xAA59 => {
                println!("设备已自动停止盘存（{s}）");
                interrupted = true;
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }

    if !interrupted {
        reader.stop_async_inventory()?;
        println!("已发送 0xAA49 停止。");
    }
    println!(
        "统计：通知 {tags} 条 / 唯一 EPC {} 个 / 心跳 {hearts} / 轮询 {rounds}",
        seen.len()
    );
    Ok(())
}
