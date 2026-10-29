//! 工作区域码（附录 3，p.171-173）与各区域默认跳频表。

/// Region Code（附录 3 两张表；中国版仅支持 0x01/0x06/0x08/0xFF）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RegionCode {
    /// 0x01 North America / FCC（902-928）
    NorthAmerica = 0x01,
    /// 0x04 INDIA（865-867）
    India = 0x04,
    /// 0x06 China 1（920-925）
    China1 = 0x06,
    /// 0x08 CE_LOW / European（865-867）
    CeLow = 0x08,
    /// 0x09 KOREA（917-921）
    Korea = 0x09,
    /// 0x0B 日本（无 LBT 4 频点，916-921）
    JapanNoLbt4 = 0x0B,
    /// 0x0C CE_HIGH
    CeHigh = 0x0C,
    /// 0x0D 香港（920-925）
    HongKong = 0x0D,
    /// 0x0E 台湾（920-927）
    Taiwan = 0x0E,
    /// 0x0F 马来西亚（919-923）
    Malaysia = 0x0F,
    /// 0x10 南非（915-918）
    SouthAfrica = 0x10,
    /// 0x11 巴西（902-927）
    Brazil = 0x11,
    /// 0x12 泰国（920-925）
    Thailand = 0x12,
    /// 0x13 新加坡（920-925）
    Singapore = 0x13,
    /// 0x14 澳大利亚（920-925）
    Australia = 0x14,
    /// 0x16 乌拉圭
    Uruguay = 0x16,
    /// 0x17 越南
    Vietnam = 0x17,
    /// 0x18 以色列
    Israel = 0x18,
    /// 0x19 菲律宾
    Philippines = 0x19,
    /// 0x1A 印度尼西亚
    Indonesia = 0x1A,
    /// 0x1B 新西兰
    NewZealand = 0x1B,
    /// 0x1C 秘鲁
    Peru = 0x1C,
    /// 0x1D 俄罗斯
    Russia = 0x1D,
    /// 0x1F 日本2（LBT 6 频点，916-921）
    Japan2Lbt6 = 0x1F,
    /// 0x20 日本3（无 LBT 19 频点，916-924，最大功率 24dBm）
    Japan3NoLbt19 = 0x20,
    /// 0xFF 全频带（840-960，中国版独有）
    FullBand = 0xFF,
}

impl RegionCode {
    pub const fn from_u8(v: u8) -> Option<Self> {
        Some(match v {
            0x01 => RegionCode::NorthAmerica,
            0x04 => RegionCode::India,
            0x06 => RegionCode::China1,
            0x08 => RegionCode::CeLow,
            0x09 => RegionCode::Korea,
            0x0B => RegionCode::JapanNoLbt4,
            0x0C => RegionCode::CeHigh,
            0x0D => RegionCode::HongKong,
            0x0E => RegionCode::Taiwan,
            0x0F => RegionCode::Malaysia,
            0x10 => RegionCode::SouthAfrica,
            0x11 => RegionCode::Brazil,
            0x12 => RegionCode::Thailand,
            0x13 => RegionCode::Singapore,
            0x14 => RegionCode::Australia,
            0x16 => RegionCode::Uruguay,
            0x17 => RegionCode::Vietnam,
            0x18 => RegionCode::Israel,
            0x19 => RegionCode::Philippines,
            0x1A => RegionCode::Indonesia,
            0x1B => RegionCode::NewZealand,
            0x1C => RegionCode::Peru,
            0x1D => RegionCode::Russia,
            0x1F => RegionCode::Japan2Lbt6,
            0x20 => RegionCode::Japan3NoLbt19,
            0xFF => RegionCode::FullBand,
            _ => return None,
        })
    }

    pub const fn value(self) -> u8 {
        self as u8
    }

    /// 区域中文名（附录 3）。
    pub const fn describe(self) -> &'static str {
        match self {
            RegionCode::NorthAmerica => "North America(902-928)",
            RegionCode::India => "India(865-867)",
            RegionCode::China1 => "China 1(920-925)",
            RegionCode::CeLow => "European / CE_LOW(865-867)",
            RegionCode::Korea => "Korea(917-921)",
            RegionCode::JapanNoLbt4 => "Japan(无LBT 4频点 916-921)",
            RegionCode::CeHigh => "CE_HIGH",
            RegionCode::HongKong => "HongKong(920-925)",
            RegionCode::Taiwan => "Taiwan(920-927)",
            RegionCode::Malaysia => "Malaysia(919-923)",
            RegionCode::SouthAfrica => "SouthAfrica(915-918)",
            RegionCode::Brazil => "Brazil(902-927)",
            RegionCode::Thailand => "Thailand(920-925)",
            RegionCode::Singapore => "Singapore(920-925)",
            RegionCode::Australia => "Australia(920-925)",
            RegionCode::Uruguay => "Uruguay",
            RegionCode::Vietnam => "Vietnam",
            RegionCode::Israel => "Israel",
            RegionCode::Philippines => "Philippines",
            RegionCode::Indonesia => "Indonesia",
            RegionCode::NewZealand => "NewZealand",
            RegionCode::Peru => "Peru",
            RegionCode::Russia => "Russia",
            RegionCode::Japan2Lbt6 => "Japan2(LBT 6频点 916-921)",
            RegionCode::Japan3NoLbt19 => "Japan3(无LBT 19频点 916-924)",
            RegionCode::FullBand => "Full Band(840-960)",
        }
    }
}

impl core::fmt::Display for RegionCode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}({:#04X})", self.describe(), self.value())
    }
}

/// North America 902-928 默认跳频表（50 频点，kHz，附录 3 p.173）。
pub const FREQ_TABLE_NA: [u32; 50] = [
    915250, 902750, 927250, 915750, 903250, 926750, 908250, 918250, 923250, 905250, 916250,
    911250, 921250, 913250, 906250, 914250, 909250, 919250, 924250, 904250, 917250, 912250,
    922250, 920250, 907250, 914750, 909750, 919750, 924750, 904750, 912750, 907750, 917750,
    922750, 910250, 925250, 910750, 920750, 925750, 905750, 913750, 908750, 918750, 923750,
    903750, 916750, 911750, 921750, 926250, 906750,
];

/// China 1 920-925 默认跳频表（16 频点，kHz）。
pub const FREQ_TABLE_CHINA1: [u32; 16] = [
    922125, 920625, 924375, 922375, 920875, 924125, 922625, 921125, 923875, 921875, 923625,
    922875, 921375, 923125, 921625, 923375,
];

/// European 865-867 默认跳频表（4 频点，kHz）。
pub const FREQ_TABLE_CE_LOW: [u32; 4] = [865700, 866300, 866900, 867500];

/// 全频带默认跳频表（11 频点，kHz，840-960 采样点）。
pub const FREQ_TABLE_FULL: [u32; 11] = [
    910000, 860000, 960000, 900000, 870000, 940000, 920000, 890000, 950000, 930000, 880000,
];

/// 日本频段1（无 LBT 4 频点，kHz）。
pub const FREQ_TABLE_JAPAN1: [u32; 4] = [916800, 918000, 919200, 920400];

/// 日本频段2（LBT 6 频点，kHz）。
pub const FREQ_TABLE_JAPAN2: [u32; 6] = [916800, 918000, 919200, 920400, 920600, 920800];

/// 日本频段3（无 LBT 19 频点，kHz）。
pub const FREQ_TABLE_JAPAN3: [u32; 19] = [
    916800, 918000, 919200, 920400, 920600, 920800, 921000, 921200, 921400, 921600, 921800,
    922000, 922200, 922400, 922600, 922800, 923000, 923200, 923400,
];

/// 韩国 917-921 默认跳频表（6 频点，kHz）。
pub const FREQ_TABLE_KOREA: [u32; 6] = [917300, 917900, 918500, 919100, 919700, 920300];

/// 香港 920-925 默认跳频表（10 频点，kHz）。
pub const FREQ_TABLE_HK: [u32; 10] = [
    920250, 920750, 921250, 921750, 922250, 922750, 923250, 923750, 924250, 924750,
];

/// 台湾 920-927 默认跳频表（14 频点，kHz）。
pub const FREQ_TABLE_TAIWAN: [u32; 14] = [
    920750, 921250, 921750, 922250, 922750, 923250, 923750, 924250, 924750, 925250, 925750,
    926250, 926750, 927250,
];

/// 马来西亚 919-923 默认跳频表（8 频点，kHz）。
pub const FREQ_TABLE_MALAYSIA: [u32; 8] = [
    919250, 919750, 920250, 920750, 921250, 921750, 922250, 922750,
];

/// 南非 915-918 默认跳频表（17 频点，kHz）。
pub const FREQ_TABLE_SOUTH_AFRICA: [u32; 17] = [
    915600, 915800, 916000, 916200, 916400, 916600, 916800, 917000, 917200, 917400, 917600,
    917800, 918000, 918200, 918400, 918600, 918800,
];

/// 巴西 902-927 默认跳频表（50 频点，kHz）。
pub const FREQ_TABLE_BRAZIL: [u32; 50] = [
    902750, 903250, 903750, 904250, 904750, 905250, 905750, 906250, 906750, 907250, 907750,
    908250, 908750, 909250, 909750, 910250, 910750, 911250, 911750, 912250, 912750, 913250,
    913750, 914250, 914750, 915250, 915750, 916250, 916750, 917250, 917750, 918250, 918750,
    919250, 919750, 920250, 920750, 921250, 921750, 922250, 922750, 923250, 923750, 924250,
    924750, 925250, 925750, 926250, 926750, 927250,
];

/// 泰国/新加坡/澳大利亚 920-925 默认跳频表（10 频点，kHz）。
pub const FREQ_TABLE_920_925_10CH: [u32; 10] = [
    920250, 920750, 921250, 921750, 922250, 922750, 923250, 923750, 924250, 924750,
];

/// 印度 865-867 默认跳频表（4 频点，kHz）。
pub const FREQ_TABLE_INDIA: [u32; 4] = [865100, 865700, 866300, 866900];

/// 区域默认跳频表（执行设置区域指令后模块即采用此表，附录 3 p.173）。
pub fn default_hopping_table(region: RegionCode) -> &'static [u32] {
    match region {
        RegionCode::NorthAmerica => &FREQ_TABLE_NA,
        RegionCode::China1 => &FREQ_TABLE_CHINA1,
        RegionCode::CeLow | RegionCode::CeHigh => &FREQ_TABLE_CE_LOW,
        RegionCode::Korea => &FREQ_TABLE_KOREA,
        RegionCode::JapanNoLbt4 => &FREQ_TABLE_JAPAN1,
        RegionCode::Japan2Lbt6 => &FREQ_TABLE_JAPAN2,
        RegionCode::Japan3NoLbt19 => &FREQ_TABLE_JAPAN3,
        RegionCode::HongKong => &FREQ_TABLE_HK,
        RegionCode::Taiwan => &FREQ_TABLE_TAIWAN,
        RegionCode::Malaysia => &FREQ_TABLE_MALAYSIA,
        RegionCode::SouthAfrica => &FREQ_TABLE_SOUTH_AFRICA,
        RegionCode::Brazil => &FREQ_TABLE_BRAZIL,
        RegionCode::Thailand | RegionCode::Singapore | RegionCode::Australia => {
            &FREQ_TABLE_920_925_10CH
        }
        RegionCode::India => &FREQ_TABLE_INDIA,
        RegionCode::FullBand => &FREQ_TABLE_FULL,
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_codes() {
        assert_eq!(RegionCode::from_u8(0x06), Some(RegionCode::China1));
        assert_eq!(RegionCode::from_u8(0xFF), Some(RegionCode::FullBand));
        assert_eq!(RegionCode::from_u8(0x07), None);
    }

    #[test]
    fn tables_match_appendix3() {
        // p.104 跳频实例前 3 频点 915250/902750/927250 与 NA 表头一致
        assert_eq!(&FREQ_TABLE_NA[..3], &[915250, 902750, 927250]);
        assert_eq!(FREQ_TABLE_NA.len(), 50); // 指令最多 50 频点（p.104）
        assert_eq!(FREQ_TABLE_CHINA1.len(), 16);
        assert_eq!(default_hopping_table(RegionCode::CeLow), &FREQ_TABLE_CE_LOW);
    }
}
