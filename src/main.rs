//! EX10 UHF 读卡器 demo CLI（M4）。
//!
//! 用法：
//! ```text
//! uhf-demo [--port /dev/ttyUSB0] [--baud 115200] <CMD>
//!
//! CMD:
//!   info                0x0C + 0x03 + 0x10 + 0x67 + 0x72 汇总（Boot 层自动 0x04）
//!   inventory <ms>      设天线 → 0x22 盘存 → 0x29 读缓冲
//!   async-inventory <s> 0xAA48 异步盘存，打印实时通知，到期 0xAA49 停止
//!   read-tid            0x28 读 TID 起始 6 字
//!   write-user <hex>    0x24 写 USER bank 地址 0（数据为偶数长度 hex）
//!   echo-detect         0xAA4A 回波检测（当前区域全频点，最长 30s）
//! ```

use std::process::ExitCode;
use std::time::Duration;

use uhf_ex10::command::async_inventory::{Notification, parse_notification};
use uhf_ex10::command::bootloader::{RunStage, VersionInfo};
use uhf_ex10::transport::UhfReader;
use uhf_ex10::types::bank::MemBank;
use uhf_ex10::types::metadata::MetadataFlags;

fn main() -> ExitCode {
    env_logger::init();

    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut port = String::from("/dev/ttyUSB0");
    let mut baud: u32 = 115_200;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--port" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--port 需要一个参数");
                    return ExitCode::FAILURE;
                }
                port = args.remove(i);
            }
            "--baud" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("--baud 需要一个参数");
                    return ExitCode::FAILURE;
                }
                baud = match args.remove(i).parse() {
                    Ok(v) => v,
                    Err(_) => {
                        eprintln!("波特率无效");
                        return ExitCode::FAILURE;
                    }
                };
            }
            _ => i += 1,
        }
    }

    let reader = match UhfReader::open(&port, baud) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("打开 {port} 失败: {e}");
            eprintln!("提示: Linux 需 root 或 dialout 组权限: sudo usermod -aG dialout $USER");
            return ExitCode::FAILURE;
        }
    };

    let result = match args.first().map(String::as_str) {
        Some("info") => cmd_info(reader),
        Some("inventory") => {
            let ms = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1000u16);
            cmd_inventory(reader, ms)
        }
        Some("async-inventory") => {
            let secs = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(10u64);
            cmd_async_inventory(reader, secs)
        }
        Some("read-tid") => cmd_read_tid(reader),
        Some("write-user") => match args.get(1) {
            Some(h) => cmd_write_user(reader, h),
            None => {
                eprintln!("用法: write-user <hex>，如 write-user AABBCCDD");
                return ExitCode::FAILURE;
            }
        },
        Some("echo-detect") => cmd_echo_detect(reader),
        other => {
            eprintln!("未知命令: {other:?}");
            eprintln!(
                "可用: info | inventory <ms> | async-inventory <s> | read-tid | write-user <hex> | echo-detect"
            );
            return ExitCode::FAILURE;
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::FAILURE
        }
    }
}

type Reader = UhfReader<uhf_ex10::transport::serial::SerialLink>;
type Res = Result<(), Box<dyn std::error::Error>>;

/// info：0x0C →（Boot 则 0x04）→ 0x03 + 0x10 + 0x67 + 0x72。
fn cmd_info(mut r: Reader) -> Res {
    let stage = r.get_run_stage()?;
    println!("Stage:      {stage:?}");
    if stage == RunStage::Boot {
        println!("处于 Bootloader 层，发送 0x04 进入 APP…");
        r.boot_firmware()?;
    }
    let v = r.get_version()?;
    print_version(&v);
    match r.get_serial() {
        Ok(s) => println!(
            "Serial:     {} / {}",
            bcd_digits(&s.raw_year),
            bcd_digits(&s.raw_serial)
        ),
        Err(e) => println!("Serial:     读取失败 ({e})"),
    }
    match r.get_region() {
        Ok(region) => print!("Region:     {region}"),
        Err(_) => print!("Region:     未知"),
    }
    match r.get_temperature() {
        Ok(t) => println!("  Temp: {t}°C"),
        Err(_) => println!(),
    }
    Ok(())
}

fn print_version(v: &VersionInfo) {
    println!(
        "Boot:       {}  Hw: {} / {}ANT / {} / rev{}",
        nibble_ver(&v.boot_ver, 3),
        v.hw.chip,
        v.hw.antenna_ports(),
        cert_region_name(v.hw.cert_region),
        v.hw.revision
    );
    println!(
        "FwDate:     {}-{}-{}  Fw: {}",
        bcd2(v.fw_date[0]),
        bcd2(v.fw_date[1]),
        bcd2(v.fw_date[2]),
        nibble_ver(&v.fw_ver, 4)
    );
    println!("Protocol:   {:#010X}", v.protocol);
}

/// 0x91 → 0x22 盘存 → 0x29 取缓存并打印。
fn cmd_inventory(mut r: Reader, ms: u16) -> Res {
    r.set_access_antenna(1, 1)?;
    let n = r.inventory(ms)?;
    println!("盘存 {ms}ms：发现 {n} 个标签");
    if n == 0 {
        return Ok(());
    }
    let tags = r.read_tag_buffer(MetadataFlags(0x0015))?; // ReadCount|AntID|Timestamp
    for (idx, t) in tags.iter().enumerate() {
        println!(
            "[{idx:02}] EPC={} PC={:04X} RC={:?} Ant={:?} Ts={:?}",
            hex(&t.epc),
            t.pc,
            t.read_count,
            t.antenna_id,
            t.timestamp_ms
        );
        if let Some(tid) = &t.tid {
            println!("     FASTID TID={}", hex(tid));
        }
    }
    Ok(())
}

/// 0xAA48 异步盘存：收通知打印，到期 0xAA49。
fn cmd_async_inventory(mut r: Reader, secs: u64) -> Res {
    use uhf_ex10::command::async_inventory::AsyncControlFlags;
    r.set_access_antenna(1, 1)?;
    r.start_async_inventory(AsyncControlFlags {
        heartbeat: true,
        single_ant_auto_stop: false,
        polling_notify: true,
    })?;
    println!("异步盘存已启动（{secs}s）…");

    let deadline = std::time::Instant::now() + Duration::from_secs(secs);
    let mut tag_count = 0u32;
    while std::time::Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(std::time::Instant::now());
        let wait = remain.min(Duration::from_millis(500));
        match r.next_notification(wait) {
            Ok(rx) => {
                let view = rx.view();
                match parse_notification(&view) {
                    Ok(Notification::Tag(t)) => {
                        tag_count += 1;
                        println!(
                            "#{tag_count} EPC={} RSSI={:?}dBm Ant={:?} Freq={:?}kHz",
                            hex(t.epc_id),
                            t.metadata.rssi_dbm,
                            t.metadata.antenna_id,
                            t.metadata.frequency_khz
                        );
                        if let Some(tid) = t.tid {
                            println!("    FASTID TID={}", hex(tid));
                        }
                    }
                    Ok(Notification::Heartbeat { search_flags }) => {
                        println!("<心跳> SearchFlags={search_flags:04X}");
                    }
                    Ok(Notification::PollingRound { ant_work_count }) => {
                        println!("<轮询完成> 轮次={ant_work_count}");
                    }
                    Err(e) => log::warn!("通知解析失败: {e}"),
                }
            }
            Err(uhf_ex10::transport::TransportError::Timeout) => {}
            Err(e) => {
                eprintln!("异步盘存中断: {e}");
                return Err(e.into());
            }
        }
    }
    r.stop_async_inventory()?;
    println!("已停止。共收到 {tag_count} 条标签通知。");
    Ok(())
}

/// 0x28 读 TID 6 字。
fn cmd_read_tid(mut r: Reader) -> Res {
    let data = r.read_memory(MemBank::Tid, 0, 6)?;
    println!("TID: {}", hex(&data));
    Ok(())
}

/// 0x24 写 USER bank。
fn cmd_write_user(mut r: Reader, hex_str: &str) -> Res {
    let data = parse_hex(hex_str)?;
    if data.is_empty() || !data.len().is_multiple_of(2) {
        return Err("数据长度必须为偶数且非空".into());
    }
    r.write_memory(MemBank::User, 0, &data)?;
    println!("写入成功: {} 字节 @ USER/0", data.len());
    Ok(())
}

/// 0xAA4A 回波检测。
fn cmd_echo_detect(mut r: Reader) -> Res {
    let region = r
        .get_region()
        .unwrap_or(uhf_ex10::types::region::RegionCode::NorthAmerica);
    println!("回波检测：天线 1，区域 {region}，全频点（最长 30s）…");
    let results = r.echo_detect(1, region, &[])?;
    for e in results.iter() {
        println!(
            "  {:>7}kHz  VL={:>3}  RL={:5.1}dB  VSWR={:.2}",
            e.frequency_khz,
            e.return_loss_raw,
            e.return_loss_raw as f32 / 10.0,
            e.vswr()
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 显示工具
// ---------------------------------------------------------------------------

/// 4 字节版本 → nibble 对点分（取前 n 段）。
fn nibble_ver(b: &[u8; 4], segments: usize) -> String {
    let mut parts = vec![format!("{}.{}", b[0] >> 4, b[0] & 0x0F)];
    parts.push(format!("{}", b[1]));
    if segments >= 4 {
        parts.push(format!("{}", b[2]));
    }
    parts.join(".")
}

/// BCD 字节 → 两位十进制。
fn bcd2(b: u8) -> String {
    format!("{:X}{:X}", b >> 4, b & 0x0F)
}

/// 低 nibble 拼接为数字串（序列号规则，p.30）。
fn bcd_digits(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:X}", b & 0x0F)).collect()
}

fn cert_region_name(code: u8) -> &'static str {
    match code {
        0x00 => "CHINA",
        0x01 => "FCC",
        0x02 => "JAPAN",
        0x03 => "CE_LOW",
        0x04 => "KOREA",
        0x05 => "CE_HIGH",
        0x06 => "HK",
        0x07 => "TAIWAN",
        0x08 => "MALAYSIA",
        0x09 => "SOUTH_AFRICA",
        0x0A => "BRAZIL",
        0x0B => "THAILAND",
        0x0C => "SINGAPORE",
        0x0D => "AUSTRALIA",
        0x0E => "INDIA",
        0x0F => "URUGUAY",
        0x10 => "VIETNAM",
        0x11 => "ISRAEL",
        0x12 => "PHILIPPINES",
        0x13 => "INDONESIA",
        0x14 => "NEW_ZEALAND",
        0x15 => "PERU",
        0x16 => "RUSSIA",
        _ => "UNKNOWN",
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn parse_hex(s: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !s.len().is_multiple_of(2) {
        return Err("hex 长度必须为偶数".into());
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| e.into()))
        .collect()
}
