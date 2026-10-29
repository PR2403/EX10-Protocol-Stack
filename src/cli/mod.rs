//! clap CLI 定义与命令分发。
//!
//! 命令分组对应协议手册章节：
//! - `system`：第 4 章系统/ Bootloader 常用指令
//! - `config`：第 7/8 章获取与设置指令
//! - `inventory`：第 5 章盘存（同步/单标签/缓冲/异步）
//! - `tag`：第 6 章标签访问（读/写/锁/灭活/块写）
//! - `multi-tag` / `echo` / `yilian`：第 10 章扩展指令
//! - `persist`：第 9 章 0xAA40 永久保存配置
//! - `firmware`：Bootloader 层 0x01/0x02/0x08
//! - `raw`：任意普通/扩展帧（调试用）

use std::process::ExitCode;
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use uhf_ex10::command::bootloader::RunStage;
use uhf_ex10::transport::{RESPONSE_TIMEOUT_BASE, UhfReader, serial::SerialLink};
use uhf_ex10::types::baud::BaudRate;
use uhf_ex10::types::metadata::MetadataFlags;
use uhf_ex10::types::region::RegionCode;

mod args;
mod config;
mod firmware;
mod format;
mod inventory;
mod misc;
mod persist;
mod system;
mod tag;
#[cfg(test)]
mod tests;

use args::{AntennaQueryArg, BankArg, SelectionArgs};

/// 串口读卡器类型别名。
pub type Reader = UhfReader<SerialLink>;
/// CLI 处理函数统一返回类型。
pub type Res<T = ()> = Result<T, Box<dyn std::error::Error>>;

/// 运行期上下文（全局选项）。
pub struct Ctx {
    /// 在 5s 基础应答超时上追加的时间。
    pub extra: Duration,
    /// Boot 层时是否自动 0x04 进入 APP。
    pub auto_boot: bool,
}

impl Ctx {
    /// 构造一次指令应答超时：5s 基础 + 全局追加 + 指令执行时间提示。
    pub fn timeout(&self, hint: Duration) -> Duration {
        RESPONSE_TIMEOUT_BASE + self.extra + hint
    }
}

/// Boot 层时自动进入 APP 层（0x04），并打印提示。
pub fn ensure_app(r: &mut Reader, ctx: &Ctx) -> Res {
    if !ctx.auto_boot {
        return Ok(());
    }
    if r.get_run_stage()? == RunStage::Boot {
        println!("处于 Bootloader 层，发送 0x04 进入 APP…");
        r.boot_firmware()?;
    }
    Ok(())
}

/// 固件指令前提：必须在 Bootloader 层。
pub fn ensure_boot(r: &mut Reader) -> Res {
    let stage = r.get_run_stage()?;
    if stage != RunStage::Boot {
        return Err(
            "固件指令仅在 Bootloader 层可用：请先执行 `uhf-demo system reset`（0x09）".into(),
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 顶层
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "uhf-demo",
    version,
    about = "EX10 系列 UHF RFID 读卡器命令行工具",
    long_about = "EX10 系列 UHF RFID 读卡器（E310/E510/E710/E910）完整 CLI。\n\
                  串口默认 /dev/ttyUSB0、115200 8N1；命令与协议手册章节对应。",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// 串口设备路径
    #[arg(
        short,
        long,
        global = true,
        env = "UHF_PORT",
        default_value = "/dev/ttyUSB0"
    )]
    pub port: String,

    /// 串口波特率
    #[arg(
        short,
        long,
        global = true,
        env = "UHF_BAUD",
        default_value_t = 115_200
    )]
    pub baud: u32,

    /// 在 5s 基础应答超时上额外追加的秒数
    #[arg(long, global = true, default_value_t = 0)]
    pub extra_timeout: u64,

    /// 关闭“Boot 层自动 0x04 进入 APP”的行为
    #[arg(long, global = true)]
    pub no_auto_boot: bool,

    /// 日志级别：-v=info，-vv=debug，-vvv=trace
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 设备摘要（运行阶段/版本/序列号/区域/温度）
    Info,

    /// 系统指令（第 4 章）
    #[command(subcommand)]
    System(SystemCmd),

    /// 配置读写（第 7/8 章：天线/功率/频率/区域/协议/GPI/GPO）
    #[command(subcommand)]
    Config(ConfigCmd),

    /// 标签盘存（第 5 章）
    #[command(subcommand)]
    Inventory(InventoryCmd),

    /// 标签访问（第 6 章）
    #[command(subcommand)]
    Tag(TagCmd),

    /// 0xAA4C 多标签匹配过滤
    #[command(subcommand)]
    MultiTag(MultiTagCmd),

    /// 0xAA4A 回波（驻波）检测
    #[command(subcommand)]
    Echo(EchoCmd),

    /// 0xAA50/0xAA51 宜链温度/亮灯标签
    #[command(subcommand)]
    Yilian(YilianCmd),

    /// 0xAA40 永久保存配置（第 9 章）
    #[command(subcommand)]
    Persist(PersistCmd),

    /// Bootloader 层固件操作（0x01/0x02/0x08）
    #[command(subcommand)]
    Firmware(FirmwareCmd),

    /// 原始帧发送（调试用）
    #[command(subcommand)]
    Raw(RawCmd),
}

// ---------------------------------------------------------------------------
// system
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
pub enum SystemCmd {
    /// 0x0C 获取运行阶段
    Stage,
    /// 0x03 获取版本信息
    Version,
    /// 0x10 获取序列号
    Serial,
    /// 0x72 获取模块温度
    Temperature,
    /// 0x04 从 Bootloader 进入 APP
    Boot,
    /// 0x09 回到 Bootloader 层（成功后自动等待 250ms）
    Reset,
}

// ---------------------------------------------------------------------------
// config
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
pub enum ConfigCmd {
    /// 0x67/0x71/0x97 工作区域
    #[command(subcommand)]
    Region(RegionCmd),
    /// 0x63/0x70/0x93 标签协议
    #[command(subcommand)]
    Protocol(ProtocolCmd),
    /// 0x61/0x91 天线配置
    #[command(subcommand)]
    Antenna(AntennaCmd),
    /// 0x62 功率信息
    Power {
        /// 同时查询最大/最小功率
        #[arg(long)]
        range: bool,
    },
    /// 0x65/0x95 跳频表与驻留时间
    #[command(subcommand)]
    Frequency(FrequencyCmd),
    /// 0x66 读取 GPI 电平
    Gpi,
    /// 0x96 设置 GPO 输出
    Gpo {
        /// GPO 设置项 <编号:状态>，如 1:1 2:0（1-2 项）
        #[arg(value_name = "N:STATE", required = true, num_args = 1..=2)]
        pairs: Vec<String>,
    },
    /// 0x6A/0x9A 读写器配置
    #[command(subcommand)]
    Reader(ReaderConfigCmd),
    /// 0x6B/0x9B 协议参数配置
    #[command(subcommand)]
    ProtocolParam(ProtocolParamCmd),
}

#[derive(Subcommand, Debug)]
pub enum RegionCmd {
    /// 0x67 获取当前区域
    Get,
    /// 0x71 获取可用区域列表
    List,
    /// 0x97 设置工作区域
    Set {
        /// 区域码或名称（0x01/na/china1/celow/…）
        #[arg(value_parser = args::parse_region)]
        region: RegionCode,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProtocolCmd {
    /// 0x63 获取当前协议
    Get,
    /// 0x70 获取可用协议
    List,
    /// 0x93 设置协议（仅 GEN2 0x0005）
    Set,
}

#[derive(Subcommand, Debug)]
pub enum AntennaCmd {
    /// 0x61 查询天线配置
    Get {
        /// 查询项
        #[arg(value_enum, default_value = "access")]
        option: AntennaQueryArg,
    },
    /// 0x91 设置标签访问天线（单对 TX/RX）
    SetPort {
        #[arg(long, default_value_t = 1)]
        tx: u8,
        #[arg(long, default_value_t = 1)]
        rx: u8,
    },
    /// 0x91 设置盘存逻辑天线（多对 TX/RX）
    SetPorts {
        /// 逻辑天线对 <tx:rx>，可多个
        #[arg(value_name = "TX:RX", required = true, num_args = 1..)]
        ports: Vec<String>,
    },
    /// 0x91 设置单天线读写功率
    SetPower {
        /// 逻辑天线号
        #[arg(long, default_value_t = 1)]
        antenna: u8,
        /// 读功率（0.01dBm，30dBm = 3000）
        #[arg(long, default_value_t = 3000)]
        read: u16,
        /// 写功率（0.01dBm，30dBm = 3000）
        #[arg(long, default_value_t = 3000)]
        write: u16,
        /// 天线配置时间（μs）；给出则使用 Option=0x04
        #[arg(long)]
        setting_time_us: Option<u16>,
    },
}

#[derive(Subcommand, Debug)]
pub enum FrequencyCmd {
    /// 0x65 获取跳频表
    Get,
    /// 0x95 设置跳频表
    Set {
        /// 频点（kHz），逗号分隔，最多 50 个
        #[arg(value_delimiter = ',', required = true, num_args = 1..)]
        khz: Vec<u32>,
    },
    /// 0x65 获取天线驻留时间
    Dwell,
    /// 0x95 设置天线驻留时间（20-60000ms）
    SetDwell { ms: u32 },
}

#[derive(Subcommand, Debug)]
pub enum ReaderConfigCmd {
    /// 0x6A 读取读写器配置
    Get {
        /// 配置键：antenna-as-id(0x00)/max-rssi(0x06)/bank-data-as-id(0x08) 或数值
        #[arg(value_parser = args::parse_reader_key)]
        key: u8,
    },
    /// 0x9A 设置读写器配置
    Set {
        /// 配置键：antenna-as-id(0x00)/max-rssi(0x06)/bank-data-as-id(0x08) 或数值
        #[arg(value_parser = args::parse_reader_key)]
        key: u8,
        #[arg(value_parser = args::parse_num_u8)]
        value: u8,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProtocolParamCmd {
    /// 0x6B 读取协议参数
    Get {
        #[arg(long, default_value_t = 5)]
        protocol: u8,
        /// 参数码（十六进制，如 0x12）
        #[arg(long, value_parser = args::parse_hex_u8)]
        parameter: u8,
    },
    /// 0x9B 设置盘存 Session（0-3）
    Session {
        #[arg(long, default_value_t = 5)]
        protocol: u8,
        #[arg(value_parser = args::parse_num_u8)]
        session: u8,
    },
    /// 0x9B 设置 Target
    Target {
        #[arg(long, default_value_t = 5)]
        protocol: u8,
        /// 0x01=静态 Target，0x00=动态 Target
        #[arg(long, value_parser = args::parse_hex_u8, default_value = "0x01")]
        option: u8,
        #[arg(value_parser = args::parse_hex_u8, default_value = "0x00")]
        value: u8,
    },
    /// 0x9B 设置 RF MODE（写入值 = MODE_ID + 100）
    RfMode {
        #[arg(long, default_value_t = 5)]
        protocol: u8,
        /// 写入值（手册：MODE_ID + 100）
        #[arg(value_parser = args::parse_num_u8)]
        value: u8,
    },
    /// 0x9B 设置 Q 值
    Q {
        #[arg(long, default_value_t = 5)]
        protocol: u8,
        /// 0x00=动态 Q（无值），0x01=静态 Q（0-15）
        #[arg(long, value_parser = args::parse_hex_u8, default_value = "0x00")]
        option: u8,
        /// 静态 Q 值（0-15），动态 Q 时省略
        #[arg(value_parser = args::parse_num_u8)]
        value: Option<u8>,
    },
}

// ---------------------------------------------------------------------------
// inventory
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
pub enum InventoryCmd {
    /// 0x22 同步盘存 + 0x29 读取并打印缓冲标签
    Run(InventoryRunArgs),
    /// 0x22 同步盘存（仅打印标签数）
    Sync(InventorySyncArgs),
    /// 0x21 单标签盘存
    Single(InventorySingleArgs),
    /// 0x29 读取标签缓冲区
    Buffer(InventoryBufferArgs),
    /// 0xAA48 异步盘存（实时打印通知，到期 0xAA49）
    Start(AsyncStartArgs),
    /// 0xAA49 停止异步盘存
    Stop,
    /// 0xAA58 EX 异步盘存（到期 0xAA59）
    ExStart(ExAsyncStartArgs),
    /// 0xAA59 停止 EX 异步盘存
    ExStop,
}

#[derive(Args, Debug)]
pub struct InventorySyncArgs {
    /// 盘存时长（ms）
    #[arg(long, default_value_t = 1000)]
    pub timeout_ms: u16,
    /// 设置标签访问天线（TX=RX=n）；省略则用设备当前配置
    #[arg(long)]
    pub antenna: Option<u8>,
    /// FASTID（Option BIT7）
    #[arg(long)]
    pub fastid: bool,
    /// SearchFlags 低 2 位原始值
    #[arg(long, default_value_t = 0, value_parser = args::parse_hex_u8)]
    pub mode_bits: u8,
    /// IMPINJ TAGFOCUS（SearchFlags BIT4）
    #[arg(long)]
    pub tagfocus: bool,
    /// 嵌入 0x28 读 <bank:addr:words>，如 tid:0:6
    #[arg(long, value_name = "BANK:ADDR:WORDS")]
    pub embed_read: Option<String>,
    #[command(flatten)]
    pub select: SelectionArgs,
}

#[derive(Args, Debug)]
pub struct InventoryRunArgs {
    #[command(flatten)]
    pub sync: InventorySyncArgs,
    /// 0x29 读取缓冲的元数据标志（十六进制或名称，默认 0x003F）
    #[arg(
        long,
        value_name = "FLAGS",
        value_parser = args::parse_metadata_flags,
        default_value = "0x003F"
    )]
    pub buffer_flags: MetadataFlags,
}

#[derive(Args, Debug)]
pub struct InventorySingleArgs {
    /// 盘存超时（ms）
    #[arg(long, default_value_t = 1000)]
    pub timeout_ms: u16,
    /// 设置标签访问天线（TX=RX=n）；省略则用设备当前配置
    #[arg(long)]
    pub antenna: Option<u8>,
    /// 返回元数据（Option BIT4）
    #[arg(long, value_name = "FLAGS", value_parser = args::parse_metadata_flags)]
    pub metadata: Option<MetadataFlags>,
    #[command(flatten)]
    pub select: SelectionArgs,
}

#[derive(Args, Debug)]
pub struct InventoryBufferArgs {
    /// 元数据标志（十六进制或名称，默认 0x003F）
    #[arg(
        long,
        value_name = "FLAGS",
        value_parser = args::parse_metadata_flags,
        default_value = "0x003F"
    )]
    pub flags: MetadataFlags,
    /// 重复读取上次 0x29 的结果（ReadOption=0x01）
    #[arg(long)]
    pub previous: bool,
    /// 最多打印条数（默认全部）
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Args, Debug)]
pub struct AsyncStartArgs {
    /// 盘存时长（秒）
    #[arg(long, default_value_t = 10)]
    pub duration: u64,
    /// 元数据标志（十六进制或名称，默认 0x003F）
    #[arg(
        long,
        value_name = "FLAGS",
        value_parser = args::parse_metadata_flags,
        default_value = "0x003F"
    )]
    pub metadata: MetadataFlags,
    /// 设置标签访问天线（TX=RX=n）；省略则用设备当前配置
    #[arg(long)]
    pub antenna: Option<u8>,
    /// SearchFlags 低 2 位原始值
    #[arg(long, default_value_t = 3, value_parser = args::parse_hex_u8)]
    pub mode_bits: u8,
    /// 嵌入 0x28 读 <bank:addr:words>（SearchFlags BIT2）
    #[arg(long, value_name = "BANK:ADDR:WORDS")]
    pub embed_read: Option<String>,
    /// IMPINJ TAGFOCUS（SearchFlags BIT4）
    #[arg(long)]
    pub tagfocus: bool,
    /// 心跳包（SearchFlags 高字节 BIT7，约每 15s）
    #[arg(long)]
    pub heartbeat: bool,
    /// 单天线无新标签自动停止（高字节 BIT6）
    #[arg(long)]
    pub auto_stop: bool,
    /// 每轮询完一遍发通知（高字节 BIT5）
    #[arg(long)]
    pub polling: bool,
    #[command(flatten)]
    pub select: SelectionArgs,
}

#[derive(Args, Debug)]
pub struct ExAsyncStartArgs {
    /// 盘存时长（秒）
    #[arg(long, default_value_t = 10)]
    pub duration: u64,
    /// 稀疏标签模式（ExConfigData 首字节=1）
    #[arg(long)]
    pub sparse: bool,
    /// 元数据标志（默认 0x0016 = RSSI|AntennaId|Timestamp，与手册实例一致）
    #[arg(
        long,
        value_name = "FLAGS",
        value_parser = args::parse_metadata_flags,
        default_value = "0x0016"
    )]
    pub metadata: MetadataFlags,
}

// ---------------------------------------------------------------------------
// tag
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
pub enum TagCmd {
    /// 0x28 读标签存储区
    Read {
        /// 存储区
        #[arg(value_enum, default_value = "tid")]
        bank: BankArg,
        /// 起始字地址
        #[arg(long, default_value_t = 0)]
        address: u32,
        /// 读取字数（1-96）
        #[arg(long, default_value_t = 6)]
        words: u8,
        /// 返回元数据（0x28 Option BIT4）
        #[arg(long, value_name = "FLAGS", value_parser = args::parse_metadata_flags)]
        metadata: Option<MetadataFlags>,
        /// 指令执行超时（ms）
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u16,
        #[command(flatten)]
        select: SelectionArgs,
    },
    /// 0x24 写标签存储区
    Write {
        /// 存储区
        #[arg(value_enum)]
        bank: BankArg,
        /// 起始字地址
        #[arg(long, default_value_t = 0)]
        address: u32,
        /// 写入数据（偶数长度十六进制，≤64 字节）
        data: String,
        /// 指令执行超时（ms）
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u16,
        /// 写入后回读（WriteReadFlag=0x84）
        #[arg(long)]
        read_after: bool,
        /// 回读存储区
        #[arg(long, value_enum, default_value = "user")]
        read_bank: BankArg,
        /// 回读起始字地址
        #[arg(long, default_value_t = 0)]
        read_address: u32,
        /// 回读字数
        #[arg(long, default_value_t = 6)]
        read_words: u8,
        #[command(flatten)]
        select: SelectionArgs,
    },
    /// 0x23 写标签 EPC
    WriteEpc {
        /// 新 EPC（偶数长度十六进制，≤62 字节）
        #[arg(value_name = "EPC")]
        new_epc: String,
        /// 指令执行超时（ms）
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u16,
        /// 写入后回读（WriteReadFlag=0x8C）
        #[arg(long)]
        read_after: bool,
        /// 回读存储区
        #[arg(long, value_enum, default_value = "tid")]
        read_bank: BankArg,
        /// 回读起始字地址
        #[arg(long, default_value_t = 0)]
        read_address: u32,
        /// 回读字数
        #[arg(long, default_value_t = 6)]
        read_words: u8,
        #[command(flatten)]
        select: SelectionArgs,
    },
    /// 0x25 锁标签（Mask/Action 低 10 位）
    Lock {
        /// AccessPassword（十六进制，不得为 0）
        #[arg(long, value_parser = args::parse_hex_u32)]
        password: u32,
        /// Mask（十六进制，低 10 位有效）
        #[arg(long, value_parser = args::parse_hex_u16)]
        mask: u16,
        /// Action（十六进制，低 10 位有效；R/W=1 锁定，Perm=1 永久锁）
        #[arg(long, value_parser = args::parse_hex_u16)]
        action: u16,
        /// 指令执行超时（ms）
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u16,
        #[command(flatten)]
        select: SelectionArgs,
    },
    /// 0x26 灭活（Kill）标签
    Kill {
        /// Kill Password（十六进制，不得为 0）
        #[arg(long, value_parser = args::parse_hex_u32)]
        password: u32,
        /// 指令执行超时（ms）
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u16,
        #[command(flatten)]
        select: SelectionArgs,
    },
    /// 0x2D 块写（1-48 字）
    BlockWrite {
        /// 存储区
        #[arg(value_enum)]
        bank: BankArg,
        /// 起始字地址
        #[arg(long, default_value_t = 0)]
        address: u32,
        /// 写入数据（偶数长度十六进制，≤96 字节）
        data: String,
        /// 指令执行超时（ms）
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u16,
        #[command(flatten)]
        select: SelectionArgs,
    },
}

// ---------------------------------------------------------------------------
// multi-tag / echo / yilian
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
pub enum MultiTagCmd {
    /// 0xAA4C 设置多标签匹配过滤表
    Set {
        /// 过滤项 <bank:addr:bits:hex>，bank=epc|tid|user；1-16 项
        #[arg(value_name = "BANK:ADDR:BITS:HEX", required = true, num_args = 1..=16)]
        entries: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum EchoCmd {
    /// 0xAA4A 回波（驻波）检测
    Detect {
        /// 逻辑天线号
        #[arg(long, default_value_t = 1)]
        antenna: u8,
        /// 频段（区域码/名称）；省略则读取当前区域
        #[arg(long, value_parser = args::parse_region)]
        band: Option<RegionCode>,
        /// 指定频点（kHz，逗号分隔）；省略则检测整个频段（最长 30s）
        #[arg(long = "freq", value_delimiter = ',', num_args = 1..)]
        frequencies: Vec<u32>,
    },
}

#[derive(Subcommand, Debug)]
pub enum YilianCmd {
    /// 0xAA50 读取宜链温度标签
    Temperature {
        /// 指令执行总时间（ms）
        #[arg(long, default_value_t = 1100)]
        time_all_ms: u16,
        /// 匹配过滤后等待（ms，≤300）
        #[arg(long, default_value_t = 0)]
        sel_wait_ms: u16,
        /// 读内存前等待（ms，≤300）
        #[arg(long, default_value_t = 100)]
        read_wait_ms: u16,
        /// 温度数据所在存储区
        #[arg(long, value_enum, default_value = "user")]
        bank: BankArg,
        /// 起始字地址
        #[arg(long, default_value_t = 0)]
        address: u32,
        /// 读取字数（间隔号为 [整数, 小数] 2 字节）
        #[arg(long, default_value_t = 1)]
        words: u8,
        /// 返回元数据
        #[arg(long, value_name = "FLAGS", value_parser = args::parse_metadata_flags)]
        metadata: Option<MetadataFlags>,
        #[command(flatten)]
        select: SelectionArgs,
    },
    /// 0xAA51 宜链亮灯
    Led {
        /// 点亮时长 / Timeout 域（ms）
        #[arg(long, default_value_t = 1000)]
        timeout_ms: u16,
        /// Option（0x07=多标签齐亮；BIT7 重定义 Timeout 语义）
        #[arg(long, value_parser = args::parse_hex_u8, default_value = "0x00")]
        option: u8,
    },
}

// ---------------------------------------------------------------------------
// persist
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
pub enum PersistCmd {
    /// 0xAA40 通用 GET
    Get {
        /// KEY（十六进制，如 0x06）
        #[arg(value_parser = args::parse_hex_u8)]
        key: u8,
        /// KEYDATA（十六进制，可空）
        #[arg(long, default_value = "")]
        keydata: String,
    },
    /// 0xAA40 通用 SET
    Set {
        /// KEY（十六进制，如 0x06）
        #[arg(value_parser = args::parse_hex_u8)]
        key: u8,
        /// KEYDATA（十六进制）
        keydata: String,
    },
    /// KEY=0x04 上电直达 APP
    #[command(subcommand)]
    PowerOnApp(PersistSwitchCmd),
    /// KEY=0x06 上电默认波特率
    #[command(subcommand)]
    Baud(BaudPersistCmd),
    /// KEY=0x97 默认工作区域
    #[command(subcommand)]
    Region(RegionPersistCmd),
    /// KEY=0x9A 默认读写器配置
    #[command(subcommand)]
    ReaderConfig(ReaderConfigPersistCmd),
    /// KEY=0x9B 默认协议配置
    #[command(subcommand)]
    ProtocolConfig(ProtocolConfigPersistCmd),
    /// KEY=0x91 保存天线功率配置（0x91 帧 Option=0x04）
    AntennaPower {
        /// <天线:读功率:写功率[:配置时间μs]>，功率单位 0.01dBm
        #[arg(value_name = "ANT:READ:WRITE[:US]", required = true, num_args = 1..)]
        entries: Vec<String>,
    },
    /// KEY=0x95 保存跳频表
    Frequency {
        /// 频点（kHz），逗号分隔，最多 50 个
        #[arg(value_delimiter = ',', required = true, num_args = 1..)]
        khz: Vec<u32>,
    },
    /// KEY=0xAA 恢复出厂设置
    FactoryReset,
    /// KEY=0xAB 准备升级（之后配合 `system reset`）
    PrepareUpgrade,
}

#[derive(Subcommand, Debug, Clone, Copy)]
pub enum PersistSwitchCmd {
    /// 读取当前设置
    Get,
    /// 启用（写入魔数 0xA5A55A5A）
    Enable,
    /// 取消（写入 0）
    Disable,
}

#[derive(Subcommand, Debug)]
pub enum BaudPersistCmd {
    /// 读取上电默认波特率
    Get,
    /// 设置上电默认波特率（设置后需重新上电生效）
    Set {
        /// 上电默认波特率（9600/19200/38400/57600/115200/230400/460800/921600）
        #[arg(value_name = "BAUD", value_parser = args::parse_baud_rate)]
        rate: BaudRate,
    },
}

#[derive(Subcommand, Debug)]
pub enum RegionPersistCmd {
    /// 读取默认区域
    Get,
    /// 设置默认区域（可用 `factory` 表示 0x5A 恢复出厂）
    Set {
        #[arg(value_parser = args::parse_region_or_factory)]
        region: u8,
    },
}

#[derive(Subcommand, Debug)]
pub enum ReaderConfigPersistCmd {
    /// 读取默认读写器配置
    Get {
        /// Option（手册 0x6A 查询 Option，默认 0x01）
        #[arg(long, value_parser = args::parse_hex_u8, default_value = "0x01")]
        option: u8,
        /// 配置键
        #[arg(value_parser = args::parse_reader_key)]
        key: u8,
    },
    /// 设置默认读写器配置
    Set {
        /// 配置键
        #[arg(value_parser = args::parse_reader_key)]
        key: u8,
        /// 值
        #[arg(value_parser = args::parse_num_u8)]
        value: u8,
    },
}

#[derive(Subcommand, Debug)]
pub enum ProtocolConfigPersistCmd {
    /// 读取默认协议配置
    Get {
        #[arg(long, default_value_t = 5)]
        protocol: u8,
        /// 参数码（十六进制，如 0x12）
        #[arg(long, value_parser = args::parse_hex_u8)]
        parameter: u8,
    },
    /// 设置默认协议配置
    Set {
        #[arg(long, default_value_t = 5)]
        protocol: u8,
        /// 参数码（十六进制，如 0x12）
        #[arg(long, value_parser = args::parse_hex_u8)]
        parameter: u8,
        /// Option（动态 Q 等场景）
        #[arg(long, value_parser = args::parse_hex_u8)]
        option: Option<u8>,
        /// 值
        #[arg(long, value_parser = args::parse_hex_u8)]
        value: Option<u8>,
    },
}

// ---------------------------------------------------------------------------
// firmware / raw
// ---------------------------------------------------------------------------

#[derive(Subcommand, Debug)]
pub enum FirmwareCmd {
    /// 0x02 读 Flash（Bootloader 层）
    Read {
        /// 起始地址（如 0x08008000）
        #[arg(value_parser = args::parse_num_u32)]
        address: u32,
        /// 读取字数（0-32）
        #[arg(default_value_t = 32, value_parser = args::parse_num_u8)]
        words: u8,
    },
    /// 0x01 写 Flash（Bootloader 层）
    Write {
        /// 烧录地址（首块 0x08008000）
        #[arg(value_parser = args::parse_num_u32)]
        address: u32,
        /// 数据（十六进制，字节数须为 4 的倍数）
        data: String,
        /// 标记为最后一块（FIN_FLAG=0xFF）
        #[arg(long)]
        last: bool,
    },
    /// 0x08 校验 Firmware
    Verify {
        /// 校验起址（必须 0x08008000）
        #[arg(long, default_value = "0x08008000", value_parser = args::parse_num_u32)]
        address: u32,
        /// 全部写入数据（十六进制）；给出则自动计算 CHECK_DATA_LEN/CHECKCRC
        #[arg(long)]
        data: Option<String>,
        /// 显式 CHECK_DATA_LEN（写入总字节数 / 4）
        #[arg(long)]
        length_words: Option<u32>,
        /// 显式 CHECKCRC（十六进制）
        #[arg(long, value_parser = args::parse_hex_u32)]
        check_crc: Option<u32>,
    },
    /// 离线计算 CHECKCRC（不需要设备）
    Crc {
        /// 写入数据（十六进制，字节数须为 4 的倍数）
        data: String,
    },
}

#[derive(Subcommand, Debug)]
pub enum RawCmd {
    /// 发送普通指令帧
    Cmd {
        /// Opcode（十六进制，如 0x03）
        #[arg(value_parser = args::parse_hex_u8)]
        opcode: u8,
        /// Data 域（十六进制）
        #[arg(long, default_value = "")]
        data: String,
        /// 额外超时（秒）
        #[arg(long, default_value_t = 5)]
        timeout_secs: u64,
    },
    /// 发送扩展指令帧（0xAA + Moduletech + SubCode）
    Ext {
        /// SubCode（十六进制，如 0xAA40）
        #[arg(value_parser = args::parse_hex_u16)]
        subcode: u16,
        /// SubData（十六进制）
        #[arg(long, default_value = "")]
        data: String,
        /// 额外超时（秒）
        #[arg(long, default_value_t = 5)]
        timeout_secs: u64,
    },
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

fn init_log(verbose: u8) {
    let level = match verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or(level)).init();
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    init_log(cli.verbose);

    // 离线命令：不需要打开串口。
    if let Command::Firmware(FirmwareCmd::Crc { data }) = &cli.command {
        return match firmware::crc_offline(data) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("错误: {e}");
                ExitCode::FAILURE
            }
        };
    }

    let mut reader = match UhfReader::open(&cli.port, cli.baud) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("打开 {} 失败: {e}", cli.port);
            eprintln!("提示: Linux 需 root 或 dialout 组权限: sudo usermod -aG dialout $USER");
            return ExitCode::FAILURE;
        }
    };

    let ctx = Ctx {
        extra: Duration::from_secs(cli.extra_timeout),
        auto_boot: !cli.no_auto_boot,
    };

    match dispatch(&mut reader, &ctx, cli.command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("错误: {e}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(reader: &mut Reader, ctx: &Ctx, command: Command) -> Res {
    match command {
        Command::Info => system::info(reader, ctx),
        Command::System(cmd) => system::run(reader, ctx, cmd),
        Command::Config(cmd) => {
            ensure_app(reader, ctx)?;
            config::run(reader, ctx, cmd)
        }
        Command::Inventory(cmd) => {
            ensure_app(reader, ctx)?;
            inventory::run(reader, ctx, cmd)
        }
        Command::Tag(cmd) => {
            ensure_app(reader, ctx)?;
            tag::run(reader, ctx, cmd)
        }
        Command::MultiTag(cmd) => {
            ensure_app(reader, ctx)?;
            misc::run_multi_tag(reader, ctx, cmd)
        }
        Command::Echo(cmd) => {
            ensure_app(reader, ctx)?;
            misc::run_echo(reader, ctx, cmd)
        }
        Command::Yilian(cmd) => {
            ensure_app(reader, ctx)?;
            misc::run_yilian(reader, ctx, cmd)
        }
        Command::Persist(cmd) => {
            ensure_app(reader, ctx)?;
            persist::run(reader, ctx, cmd)
        }
        Command::Firmware(cmd) => firmware::run(reader, ctx, cmd),
        Command::Raw(cmd) => firmware::run_raw(reader, ctx, cmd),
    }
}
