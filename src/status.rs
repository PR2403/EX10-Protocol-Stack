//! 状态码（手册"状态码"章 p.164-167）。
//!
//! 响应帧 Op 之后固定携带 2 字节 Status Code；0x0000 表示成功，
//! 非 0 值经 [`Status::from_u16`] 映射为枚举，未知值落到 [`Status::Unknown`]。

/// 用宏批量定义状态码枚举：variant = value => 中文描述。
macro_rules! define_status {
    ($( $variant:ident = $value:literal => $desc:literal ),+ $(,)?) => {
        /// 设备状态码。
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Status {
            $(
                #[doc = concat!("`", stringify!($value), "` ", $desc)]
                $variant,
            )+
            /// 未收录的非 0 状态码（手册：其它非 0 表示命令执行失败）。
            Unknown(u16),
        }

        impl Status {
            /// 从 16 位原始值映射。
            pub const fn from_u16(v: u16) -> Self {
                match v {
                    $( $value => Status::$variant, )*
                    other => Status::Unknown(other),
                }
            }

            /// 原始 16 位值。
            pub const fn value(self) -> u16 {
                match self {
                    $( Status::$variant => $value, )*
                    Status::Unknown(v) => v,
                }
            }

            /// 中文含义（手册 p.164-167）。
            pub const fn describe(self) -> &'static str {
                match self {
                    $( Status::$variant => $desc, )*
                    Status::Unknown(_) => "其它非 0 表示命令执行失败",
                }
            }
        }

        impl From<u16> for Status {
            fn from(v: u16) -> Self {
                Status::from_u16(v)
            }
        }
    };
}

define_status! {
    Success                 = 0x0000 => "成功",
    DataLengthMismatch      = 0x0100 => "Data 的实际长度与 Data Length 字段的值不同",
    CommandUnavailable      = 0x0101 => "命令不可用，常见于模块供电不稳定导致模块软复位",
    InvalidParameter        = 0x0105 => "参数值不可用",
    BaudRateUnavailable     = 0x010A => "波特率不可用",
    RegionUnavailable       = 0x010B => "不可用的区域选择",
    AppFirmwareCrcError     = 0x0200 => "App Firmware 层程序 CRC 不正确",
    FlashError              = 0x0302 => "Flash 未定义错误，闪存写入失败",
    NoTagFound              = 0x0400 => "未发现标签，常见于功率不足或者过滤不匹配",
    ProtocolUnavailable     = 0x0402 => "协议不可用",
    EmbeddedReadAfterWriteFailed = 0x0404 => "写嵌入读时写成功读失败",
    GeneralTagError         = 0x040A => "常规标签错误（读/写锁定，kill 命令）",
    ReadLengthExceeded      = 0x040B => "读取存储区域超出限制的长度（例如，一次只能读取 96 个字）",
    InvalidKillPassword     = 0x040C => "不可用的销毁密码",
    Gen2ProtocolError       = 0x0420 => "GEN2 协议错误",
    MemoryOutOfBounds       = 0x0423 => "存储区越界，错误的 PC",
    MemoryLocked            = 0x0424 => "锁定的存储区域",
    InsufficientPower       = 0x042B => "能量不足",
    NonSpecificError        = 0x042F => "非特定错误",
    UnknownDeviceError      = 0x0430 => "未知错误",
    InvalidFrequency        = 0x0500 => "频率值不可用",
    OverTemperature         = 0x0504 => "温度过高",
    HighReturnLoss          = 0x0505 => "回波损耗过大，常见于金属反射的环境，也较少发生在频点、天线等原因",
    SystemError             = 0x7F00 => "系统不知道的错误，严重错误",
    YilianTempOutOfRange    = 0x500F => "宜链温度标签检测温度超出量程",
    YilianTempUnstable      = 0x50FF => "宜链温度标签检测温度不稳定",
    OemWriteFailed          = 0xAA02 => "写 OEM 寄存器失败",
    OemReadFailed           = 0xAA03 => "读 OEM 寄存器失败",
    CommandFailed           = 0xAA04 => "该命令执行失败",
    OemFormatFailed         = 0xAA2A => "OEM 格式化失败",
    CarrierOpFailed         = 0xAA31 => "载波开启或关闭操作失败",
    SaveConfigOemFailed     = 0xAA40 => "保存配置命令写 OEM 寄存器失败",
    VswrDetectFailed        = 0xAA4A => "驻波检测失败",
    ResetFailed             = 0xAA4B => "复位 EX10 或上下电 EX10 等操作失败",
    NoMultiTagFilter        = 0xAA4C => "未有有效的设置多标签匹配过滤数据",
    SessionTargetAFailed    = 0xAA4D => "设置 SESSION2/SESSION3 TARGET 为 A 失败",
    UserMemOpFailed         = 0xAA55 => "用户自定义存储区命令读写执行失败",
    UserCmdFailed           = 0xAA56 => "用户自定义命令命令配置或读取执行失败",
    NotInAppLayer           = 0xEE01 => "芯片未执行到应用层错误",
    FwUpgradeStartFailed    = 0xEE02 => "芯片固件启动升级失败",
    FwUpgradeContinueFailed = 0xEE03 => "芯片固件继续升级失败",
    FwUpgradeEndFailed      = 0xEE04 => "芯片固件结束升级失败",
    FlashInitFailed         = 0xFF11 => "FLASH 初始化失败",
    GpioInitFailed          = 0xFF12 => "GPIO 初始化失败",
    TimerInitFailed         = 0xFF13 => "定时器初始化失败",
    SpiInitFailed           = 0xFF14 => "SPI 初始化失败",
    AntennaInitFailed       = 0xFF15 => "天线控制初始化失败",
    BandInitFailed          = 0xFF16 => "频段初始化失败",
    Ex10InitFailed          = 0xFF17 => "EX10 初始化失败",
    FirmwareAbnormal        = 0xFF1F => "固件异常",
    HwVersionError          = 0xFFFF => "硬件版本号错误",
}

impl core::fmt::Display for Status {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:#06X} {}", self.value(), self.describe())
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Status {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(Status::from_u16(0x0000), Status::Success);
        assert_eq!(Status::from_u16(0x0400).value(), 0x0400);
        assert_eq!(Status::from_u16(0x0101), Status::CommandUnavailable);
    }

    #[test]
    fn unknown_values() {
        let s = Status::from_u16(0x1234);
        assert_eq!(s, Status::Unknown(0x1234));
        assert_eq!(s.value(), 0x1234);
    }
}
