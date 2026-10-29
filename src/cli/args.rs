//! CLI 通用值解析与可复用参数组。
//!
//! 所有解析函数返回 `Result<T, String>`，可直接交给 clap 的 `value_parser`。

use clap::{Args, ValueEnum};
use uhf_ex10::types::bank::MemBank;
use uhf_ex10::types::baud::BaudRate;
use uhf_ex10::types::metadata::MetadataFlags;
use uhf_ex10::types::region::RegionCode;
use uhf_ex10::types::select::{SelectMode, TagSingulation};

fn strip_hex_prefix(s: &str) -> &str {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s)
}

/// 解析十六进制字节串（允许空格/下划线/冒号分隔，可带 0x 前缀）。
pub fn parse_hex_bytes(s: &str) -> Result<Vec<u8>, String> {
    let mut cleaned = String::with_capacity(s.len());
    for c in strip_hex_prefix(s.trim()).chars() {
        if c.is_ascii_whitespace() || c == '_' || c == ':' {
            continue;
        }
        if !c.is_ascii_hexdigit() {
            return Err(format!("非法十六进制字符: {c:?}"));
        }
        cleaned.push(c);
    }
    if !cleaned.len().is_multiple_of(2) {
        return Err(format!(
            "十六进制长度必须为偶数（当前 {} 位）",
            cleaned.len()
        ));
    }
    (0..cleaned.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|e| format!("非法十六进制数据: {e}"))
        })
        .collect()
}

/// 始终按十六进制解析 u32（`0x` 前缀可选）。
pub fn parse_hex_u32(s: &str) -> Result<u32, String> {
    let t = strip_hex_prefix(s.trim());
    if t.is_empty() {
        return Err("缺少十六进制数值".into());
    }
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("非法十六进制数值: {s}"));
    }
    u32::from_str_radix(t, 16).map_err(|e| format!("十六进制数值溢出: {e}"))
}

pub fn parse_hex_u16(s: &str) -> Result<u16, String> {
    u16::try_from(parse_hex_u32(s)?).map_err(|_| "数值超出 u16 范围".to_string())
}

pub fn parse_hex_u8(s: &str) -> Result<u8, String> {
    u8::try_from(parse_hex_u32(s)?).map_err(|_| "数值超出 u8 范围".to_string())
}

/// `0x` 前缀为十六进制，否则按十进制。
pub fn parse_num_u32(s: &str) -> Result<u32, String> {
    let t = s.trim();
    if t.starts_with("0x") || t.starts_with("0X") {
        parse_hex_u32(t)
    } else {
        t.parse::<u32>().map_err(|e| format!("非法数值 {t:?}: {e}"))
    }
}

pub fn parse_num_u16(s: &str) -> Result<u16, String> {
    u16::try_from(parse_num_u32(s)?).map_err(|_| "数值超出 u16 范围".to_string())
}

pub fn parse_num_u8(s: &str) -> Result<u8, String> {
    u8::try_from(parse_num_u32(s)?).map_err(|_| "数值超出 u8 范围".to_string())
}

/// 区域码：支持名称（na/china1/celow/…）或数值（0x06 / 6）。
pub fn parse_region(s: &str) -> Result<RegionCode, String> {
    let key = s.trim().to_ascii_lowercase();
    let named = match key.as_str() {
        "na" | "fcc" | "us" | "north-america" | "northamerica" => RegionCode::NorthAmerica,
        "cn" | "china" | "china1" | "china-1" => RegionCode::China1,
        "eu" | "ce" | "celow" | "ce-low" | "europe" => RegionCode::CeLow,
        "cehigh" | "ce-high" => RegionCode::CeHigh,
        "in" | "india" => RegionCode::India,
        "kr" | "korea" => RegionCode::Korea,
        "jp" | "japan" | "japan1" | "japan-1" => RegionCode::JapanNoLbt4,
        "japan2" | "japan-2" => RegionCode::Japan2Lbt6,
        "japan3" | "japan-3" => RegionCode::Japan3NoLbt19,
        "hk" | "hongkong" | "hong-kong" => RegionCode::HongKong,
        "tw" | "taiwan" => RegionCode::Taiwan,
        "my" | "malaysia" => RegionCode::Malaysia,
        "za" | "south-africa" | "southafrica" => RegionCode::SouthAfrica,
        "br" | "brazil" => RegionCode::Brazil,
        "th" | "thailand" => RegionCode::Thailand,
        "sg" | "singapore" => RegionCode::Singapore,
        "au" | "australia" => RegionCode::Australia,
        "uy" | "uruguay" => RegionCode::Uruguay,
        "vn" | "vietnam" => RegionCode::Vietnam,
        "il" | "israel" => RegionCode::Israel,
        "ph" | "philippines" => RegionCode::Philippines,
        "id" | "indonesia" => RegionCode::Indonesia,
        "nz" | "newzealand" | "new-zealand" => RegionCode::NewZealand,
        "pe" | "peru" => RegionCode::Peru,
        "ru" | "russia" => RegionCode::Russia,
        "full" | "fullband" | "full-band" => RegionCode::FullBand,
        _ => {
            let v = parse_num_u8(&key)?;
            return RegionCode::from_u8(v).ok_or_else(|| format!("未知区域码 0x{v:02X}"));
        }
    };
    Ok(named)
}

/// 0xAA40 KEY=0x97 的设置值：区域码或 `factory`（0x5A 恢复出厂）。
pub fn parse_region_or_factory(s: &str) -> Result<u8, String> {
    if s.trim().eq_ignore_ascii_case("factory") {
        return Ok(0x5A);
    }
    Ok(parse_region(s)?.value())
}

/// 元数据标志：十六进制（如 `0x003F`）或逗号分隔名称
/// （`read-count,rssi,ant,freq,timestamp,phase,protocol,tag-data`，另有
/// `common`=0x003F、`all`=0x00FF）。
pub fn parse_metadata_flags(s: &str) -> Result<MetadataFlags, String> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(MetadataFlags::NONE);
    }
    let looks_numeric = t.starts_with("0x")
        || t.starts_with("0X")
        || (t.chars().all(|c| c.is_ascii_hexdigit()) && t.chars().any(|c| c.is_ascii_digit()));
    if looks_numeric {
        return Ok(MetadataFlags(parse_hex_u16(t)?));
    }
    let mut bits = 0u16;
    for part in t.split([',', '|']) {
        let p = part.trim().to_ascii_lowercase();
        if p.is_empty() {
            continue;
        }
        bits |= match p.as_str() {
            "none" => 0x0000,
            "read-count" | "rc" => 0x0001,
            "rssi" => 0x0002,
            "ant" | "antenna" | "antenna-id" => 0x0004,
            "freq" | "frequency" => 0x0008,
            "ts" | "timestamp" => 0x0010,
            "phase" => 0x0020,
            "protocol" | "protocol-id" => 0x0040,
            "tag-data" | "data" => 0x0080,
            "common" => 0x003F,
            "all" => 0x00FF,
            other => return Err(format!("未知元数据标志: {other}")),
        };
    }
    Ok(MetadataFlags(bits))
}

/// 波特率（附录 2 值表）。
pub fn parse_baud_rate(s: &str) -> Result<BaudRate, String> {
    let v: u32 = s
        .trim()
        .parse()
        .map_err(|e| format!("非法波特率 {s:?}: {e}"))?;
    BaudRate::from_u32(v).ok_or_else(|| {
        format!("波特率 {v} 不在 9600/19200/38400/57600/115200/230400/460800/921600 之中")
    })
}

/// 0x9A/0x6A 读写器配置键：名称或数值。
pub fn parse_reader_key(s: &str) -> Result<u8, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "antenna-as-id" | "antenna" => Ok(0x00),
        "max-rssi" | "rssi" => Ok(0x06),
        "bank-data-as-id" | "bank" => Ok(0x08),
        _ => parse_num_u8(s),
    }
}

/// 解析 `a:b` 形式的 u8 对（如天线 `1:1`、GPO `1:0`）。
pub fn parse_pair_u8(s: &str) -> Result<(u8, u8), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return Err(format!("格式应为 <a:b>，得到 {s:?}"));
    }
    Ok((parse_num_u8(parts[0])?, parse_num_u8(parts[1])?))
}

/// 解析 Select 过滤项 `<bank:addr:bits:hex>`，bank 为 epc/tid/user。
pub fn parse_select_spec(s: &str) -> Result<(SelectMode, u32, u16, Vec<u8>), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 4 {
        return Err(format!(
            "Select 过滤格式应为 <bank:addr:bits:hex>，得到 {s:?}"
        ));
    }
    let mode = match parts[0].trim().to_ascii_lowercase().as_str() {
        "epc" | "epc-bank" | "epcbank" => SelectMode::EpcBank,
        "tid" => SelectMode::Tid,
        "user" => SelectMode::User,
        other => return Err(format!("Select bank 必须是 epc/tid/user，得到 {other:?}")),
    };
    let address = parse_num_u32(parts[1])?;
    let bits = parse_num_u16(parts[2])?;
    let data = parse_hex_bytes(parts[3])?;
    let need = (bits as usize).div_ceil(8);
    if need != data.len() {
        return Err(format!(
            "Select 位数 {bits} 需要 {need} 字节数据，实际 {} 字节",
            data.len()
        ));
    }
    Ok((mode, address, bits, data))
}

/// 0x22 嵌入读 `<bank:addr:words>`。
pub fn parse_embed_read(s: &str) -> Result<(MemBank, u32, u8), String> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("嵌入读格式应为 <bank:addr:words>，得到 {s:?}"));
    }
    let bank = parse_bank_name(parts[0])?;
    Ok((bank, parse_num_u32(parts[1])?, parse_num_u8(parts[2])?))
}

fn parse_bank_name(s: &str) -> Result<MemBank, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "reserved" | "res" => Ok(MemBank::Reserved),
        "epc" => Ok(MemBank::Epc),
        "tid" => Ok(MemBank::Tid),
        "user" => Ok(MemBank::User),
        other => Err(format!("未知存储区 {other:?}（reserved/epc/tid/user）")),
    }
}

/// 存储区参数（clap ValueEnum）。
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum BankArg {
    Reserved,
    Epc,
    Tid,
    User,
}

impl From<BankArg> for MemBank {
    fn from(b: BankArg) -> Self {
        match b {
            BankArg::Reserved => MemBank::Reserved,
            BankArg::Epc => MemBank::Epc,
            BankArg::Tid => MemBank::Tid,
            BankArg::User => MemBank::User,
        }
    }
}

/// 0x61 查询项（clap ValueEnum）。
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum AntennaQueryArg {
    /// 标签访问操作使用的天线
    Access,
    /// 盘存使用的逻辑天线
    Inventory,
    /// 所有天线读写功率
    Power,
    /// 功率 + 天线配置时间
    PowerTime,
    /// 天线连接状态
    Connection,
}

/// 标签选择过滤组（各标签访问指令共用）。
#[derive(Args, Debug, Clone)]
pub struct SelectionArgs {
    /// AccessPassword（十六进制）；无过滤时按 Select-Option=0x05 携带
    #[arg(long, value_parser = parse_hex_u32, value_name = "HEX32")]
    pub access_password: Option<u32>,

    /// 按 EPC 值过滤（完整 EPC 十六进制）
    #[arg(long, value_name = "HEX")]
    pub epc: Option<String>,

    /// bank 过滤 <bank:addr:bits:hex>；bank=epc|tid|user；特殊值 `multi` 使用
    /// 多标签过滤（需先 `multi-tag set`）
    #[arg(long, value_name = "BANK:ADDR:BITS:HEX")]
    pub select: Option<String>,

    /// 反选过滤（Select-Option BIT3）
    #[arg(long)]
    pub invert: bool,

    /// SelectDataLength 使用 2 字节（选择数据 >255 位时）
    #[arg(long)]
    pub long_length: bool,
}

impl SelectionArgs {
    /// 构建完整 TagSingulation（含密码/反选/长度扩展）。
    pub fn build(&self) -> Result<TagSingulation, String> {
        let mut s = self.build_base()?;
        if let Some(pwd) = self.access_password {
            if s.mode == SelectMode::None {
                s.mode = SelectMode::LockedNoFilter;
            }
            s.access_password = pwd;
        }
        self.finish(s)
    }

    /// 仅构建过滤部分（0x21/0x25/0x26 没有选择密码域，密码走指令自有字段）。
    pub fn build_filter_only(&self) -> Result<TagSingulation, String> {
        if self.access_password.is_some() {
            return Err("该指令不支持选择过滤密码，请用指令自身的 --password 字段".to_string());
        }
        let s = self.build_base()?;
        self.finish(s)
    }

    fn build_base(&self) -> Result<TagSingulation, String> {
        if self.epc.is_some() && self.select.is_some() {
            return Err("--epc 与 --select 不能同时使用".into());
        }
        if let Some(sel) = &self.select {
            if sel.trim().eq_ignore_ascii_case("multi") {
                return Ok(TagSingulation {
                    mode: SelectMode::MultiTag,
                    invert: self.invert,
                    ..Default::default()
                });
            }
            let (mode, address, bits, data) = parse_select_spec(sel)?;
            let mut s = TagSingulation::bank(mode, address, bits, &data);
            s.invert = self.invert;
            s.long_length = self.long_length;
            self.check_data_len(&s)?;
            Ok(s)
        } else if let Some(epc_hex) = &self.epc {
            let data = parse_hex_bytes(epc_hex)?;
            if data.is_empty() {
                return Err("--epc 不能为空".into());
            }
            let mut s = TagSingulation::epc_value(&data).map_err(|e| e.to_string())?;
            s.invert = self.invert;
            s.long_length = self.long_length;
            self.check_data_len(&s)?;
            Ok(s)
        } else {
            if self.invert || self.long_length {
                return Err("--invert/--long-length 需要配合 --epc 或 --select 使用".into());
            }
            Ok(TagSingulation::default())
        }
    }

    fn check_data_len(&self, s: &TagSingulation) -> Result<(), String> {
        if s.mode.has_data() && s.data_len_bits > 255 && !s.long_length {
            return Err("选择数据超过 255 位，请加 --long-length".into());
        }
        Ok(())
    }

    fn finish(&self, mut s: TagSingulation) -> Result<TagSingulation, String> {
        if s.mode == SelectMode::None && (self.invert || self.long_length) {
            return Err("--invert/--long-length 需要配合 --epc 或 --select 使用".into());
        }
        self.check_data_len(&s)?;
        s.invert = self.invert;
        s.long_length = self.long_length;
        Ok(s)
    }
}
