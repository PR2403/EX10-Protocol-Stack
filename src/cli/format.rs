//! CLI 输出格式化：版本、标签、通知、元数据。

use uhf_ex10::command::async_inventory::Notification;
use uhf_ex10::command::bootloader::VersionInfo;
use uhf_ex10::command::inventory::TagEpcAndMeta;
use uhf_ex10::types::metadata::{Metadata, MetadataFlags};

/// 紧凑十六进制（大写、无分隔）。
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

/// 4 字节版本 → nibble 对点分（取前 `segments` 段）。
pub fn nibble_ver(b: &[u8; 4], segments: usize) -> String {
    let mut parts = vec![format!("{}.{}", b[0] >> 4, b[0] & 0x0F)];
    parts.push(format!("{}", b[1]));
    if segments >= 4 {
        parts.push(format!("{}", b[2]));
    }
    parts.join(".")
}

/// BCD 字节 → 两位十六进制数字字符串。
pub fn bcd2(b: u8) -> String {
    format!("{:X}{:X}", b >> 4, b & 0x0F)
}

/// 低 nibble 拼接为数字串（序列号规则，p.30）。
pub fn bcd_digits(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:X}", b & 0x0F)).collect()
}

/// 硬件认证区域码名称（p.23）。
pub fn cert_region_name(code: u8) -> &'static str {
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

pub fn print_version(v: &VersionInfo) {
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

/// 元数据标志的中文名列表。
pub fn describe_metadata_flags(flags: MetadataFlags) -> String {
    let names: [(MetadataFlags, &str); 8] = [
        (MetadataFlags::READ_COUNT, "ReadCount"),
        (MetadataFlags::RSSI, "RSSI"),
        (MetadataFlags::ANTENNA_ID, "AntennaId"),
        (MetadataFlags::FREQUENCY, "Frequency"),
        (MetadataFlags::TIMESTAMP, "Timestamp"),
        (MetadataFlags::PHASE, "Phase"),
        (MetadataFlags::PROTOCOL_ID, "ProtocolId"),
        (MetadataFlags::TAG_DATA, "TagData"),
    ];
    let mut parts = Vec::new();
    for (f, name) in names {
        if flags.contains(f) {
            parts.push(name);
        }
    }
    if parts.is_empty() {
        "无".to_string()
    } else {
        parts.join("|")
    }
}

/// 打印元数据摘要。
pub fn print_metadata(md: &Metadata<'_>, flags: MetadataFlags) {
    println!("元数据（{}）：", describe_metadata_flags(flags));
    if let Some(v) = md.read_count {
        println!("  ReadCount:   {v}");
    }
    if let Some(v) = md.rssi_dbm {
        println!("  RSSI:        {v} dBm");
    }
    if let Some(v) = md.antenna_id {
        println!("  AntennaId:   {v}");
    }
    if let Some(v) = md.frequency_khz {
        println!("  Frequency:   {v} kHz");
    }
    if let Some(v) = md.timestamp_ms {
        println!("  Timestamp:   {v} ms");
    }
    if let Some(v) = md.phase {
        println!("  Phase:       0x{v:04X}");
    }
    if let Some(v) = md.protocol_id {
        println!("  ProtocolId:  {v:#04X}");
    }
    if let Some(bits) = md.tag_data_len_bits {
        println!("  TagData:     {} bit → {}", bits, hex(md.tag_data));
    }
}

/// 打印一条 0x29 缓冲标签。
///
/// `flags` 用于解释块尾 2 字节：启用 Phase 标志时为起始相位，否则为 TagCRC。
pub fn print_tag(idx: Option<usize>, t: &TagEpcAndMeta<'_>, flags: Option<MetadataFlags>) {
    let prefix = match idx {
        Some(i) => format!("[{i:03}] "),
        None => String::new(),
    };
    let mut line = format!("{prefix}EPC={} PC={:04X}", hex(t.epc_id), t.pc);
    if let Some(v) = t.metadata.read_count {
        line.push_str(&format!(" RC={v}"));
    }
    if let Some(v) = t.metadata.rssi_dbm {
        line.push_str(&format!(" RSSI={v}dBm"));
    }
    if let Some(v) = t.metadata.antenna_id {
        line.push_str(&format!(" Ant={v}"));
    }
    if let Some(v) = t.metadata.frequency_khz {
        line.push_str(&format!(" Freq={v}kHz"));
    }
    if let Some(v) = t.metadata.timestamp_ms {
        line.push_str(&format!(" Ts={v}ms"));
    }
    if let Some(v) = t.metadata.phase {
        line.push_str(&format!(" Phase=0x{v:04X}"));
    }
    match flags {
        Some(f) if f.contains(MetadataFlags::PHASE) => {
            // 手册 p.41：启用相位时尾域为起始相位 PHASE2；但真机 Fw 2.0.7.3
            // 在 0x29 中仍返回 TagCRC（与 PC+EPC 的 ISO6C CRC 完全一致），故双名标注。
            line.push_str(&format!(" PHASE2/TagCRC=0x{:04X}", t.tag_crc));
        }
        Some(_) => line.push_str(&format!(" TagCRC=0x{:04X}", t.tag_crc)),
        None => line.push_str(&format!(" Tail=0x{:04X}", t.tag_crc)),
    }
    println!("{line}");
    if let Some(tid) = t.tid {
        println!("      FASTID TID={}", hex(tid));
    }
    if !t.metadata.tag_data.is_empty() {
        println!("      TagData={}", hex(t.metadata.tag_data));
    }
}

/// 打印一条异步盘存通知。
pub fn print_notification(n: &Notification<'_>, seq: u32) {
    match n {
        Notification::Tag(t) => {
            println!("#{seq} 标签通知");
            print_tag(None, t, None);
        }
        Notification::Heartbeat { search_flags } => {
            println!("#{seq} <心跳> SearchFlags={search_flags:04X}");
        }
        Notification::PollingRound { ant_work_count } => {
            println!("#{seq} <轮询完成> 轮次={ant_work_count}");
        }
    }
}
