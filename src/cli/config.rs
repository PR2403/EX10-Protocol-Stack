//! `config` 命令组：第 7/8 章获取与设置指令。

use std::time::Duration;

use uhf_ex10::command::get::{
    self, AntennaConfigResponse, AntennaQueryOption, FrequencyHopResponse,
};
use uhf_ex10::command::set::{
    AntennaPower, ProtocolConfigParam, ReaderConfigKey, SetAntennaRequest, SetFrequencyHopRequest,
    SetGpoRequest, SetProtocolConfigRequest, SetProtocolRequest, SetReaderConfigRequest,
    SetRegionRequest,
};
use uhf_ex10::types::region::default_hopping_table;

use super::args::{self, AntennaQueryArg};
use super::{
    AntennaCmd, ConfigCmd, Ctx, FrequencyCmd, ProtocolCmd, ProtocolParamCmd, Reader,
    ReaderConfigCmd, RegionCmd, Res,
};

pub fn run(r: &mut Reader, ctx: &Ctx, cmd: ConfigCmd) -> Res {
    match cmd {
        ConfigCmd::Region(c) => region(r, ctx, c),
        ConfigCmd::Protocol(c) => protocol(r, ctx, c),
        ConfigCmd::Antenna(c) => antenna(r, ctx, c),
        ConfigCmd::Power { range } => power(r, ctx, range),
        ConfigCmd::Frequency(c) => frequency(r, ctx, c),
        ConfigCmd::Gpi => gpi(r, ctx),
        ConfigCmd::Gpo { pairs } => gpo(r, ctx, &pairs),
        ConfigCmd::Reader(c) => reader_config(r, ctx, c),
        ConfigCmd::ProtocolParam(c) => protocol_param(r, ctx, c),
    }
}

fn send_ok(r: &mut Reader, ctx: &Ctx, opcode: u8, data: &[u8]) -> Res {
    let _ = r.transact(opcode, data, ctx.timeout(Duration::ZERO))?;
    Ok(())
}

fn region(r: &mut Reader, ctx: &Ctx, cmd: RegionCmd) -> Res {
    match cmd {
        RegionCmd::Get => {
            let region = r.get_region()?;
            println!("{region}");
            let table = default_hopping_table(region);
            if !table.is_empty() {
                let list = table
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(", ");
                println!("默认跳频表（{} 频点）：{list}", table.len());
            }
        }
        RegionCmd::List => {
            let rx = r.transact(0x71, &[], ctx.timeout(Duration::ZERO))?;
            let regions = get::parse_available_regions(&rx.view())?;
            println!("可用区域（{}）：", regions.len());
            for region in regions.iter() {
                println!("  {}", *region);
            }
        }
        RegionCmd::Set { region } => {
            let mut d = heapless::Vec::<u8, 8>::new();
            SetRegionRequest { region }.encode(&mut d)?;
            send_ok(r, ctx, 0x97, &d)?;
            println!("已设置工作区域：{region}");
        }
    }
    Ok(())
}

fn protocol(r: &mut Reader, ctx: &Ctx, cmd: ProtocolCmd) -> Res {
    match cmd {
        ProtocolCmd::Get => {
            let rx = r.transact(0x63, &[], ctx.timeout(Duration::ZERO))?;
            let p = get::parse_protocol(&rx.view())?;
            println!("当前协议：{p:#06X}");
        }
        ProtocolCmd::List => {
            let rx = r.transact(0x70, &[], ctx.timeout(Duration::ZERO))?;
            let p = get::parse_protocol(&rx.view())?;
            println!("可用协议：{p:#06X}");
        }
        ProtocolCmd::Set => {
            let mut d = heapless::Vec::<u8, 8>::new();
            SetProtocolRequest { protocol: 0x0005 }.encode(&mut d)?;
            send_ok(r, ctx, 0x93, &d)?;
            println!("已设置协议：0x0005 (GEN2)");
        }
    }
    Ok(())
}

fn antenna(r: &mut Reader, ctx: &Ctx, cmd: AntennaCmd) -> Res {
    match cmd {
        AntennaCmd::Get { option } => {
            let option = match option {
                AntennaQueryArg::Access => AntennaQueryOption::Access,
                AntennaQueryArg::Inventory => AntennaQueryOption::Inventory,
                AntennaQueryArg::Power => AntennaQueryOption::Power,
                AntennaQueryArg::PowerTime => AntennaQueryOption::PowerAndTime,
                AntennaQueryArg::Connection => AntennaQueryOption::ConnectionStatus,
            };
            let mut d = heapless::Vec::<u8, 8>::new();
            get::encode_antenna_query(option, &mut d);
            let rx = r.transact(0x61, &d, ctx.timeout(Duration::ZERO))?;
            match get::parse_antenna_config(&rx.view(), option)? {
                AntennaConfigResponse::Access { tx, rx } => {
                    println!("标签访问天线：TX={tx} RX={rx}");
                }
                AntennaConfigResponse::Inventory(ports) => {
                    println!("盘存逻辑天线（{} 对）：", ports.len());
                    for (i, (tx, rx)) in ports.iter().enumerate() {
                        println!("  [{i}] TX={tx} RX={rx}");
                    }
                }
                AntennaConfigResponse::Power(cfgs) => {
                    println!("天线功率（{} 项）：", cfgs.len());
                    for c in cfgs.iter() {
                        let extra = c
                            .setting_time_us
                            .map(|t| format!("  setting_time={t}μs"))
                            .unwrap_or_default();
                        println!(
                            "  Ant {}: read={} ({}dBm) write={} ({}dBm){extra}",
                            c.antenna,
                            c.read_power,
                            c.read_power as f32 / 100.0,
                            c.write_power,
                            c.write_power as f32 / 100.0
                        );
                    }
                }
                AntennaConfigResponse::Connections(list) => {
                    println!("天线连接状态（{} 项）：", list.len());
                    for c in list.iter() {
                        println!(
                            "  Ant {}: {}",
                            c.antenna,
                            if c.connected {
                                "已连接"
                            } else {
                                "未连接"
                            }
                        );
                    }
                }
            }
        }
        AntennaCmd::SetPort { tx, rx } => {
            let mut d = heapless::Vec::<u8, 8>::new();
            SetAntennaRequest::AccessPort { tx, rx }.encode(&mut d)?;
            send_ok(r, ctx, 0x91, &d)?;
            println!("已设置标签访问天线：TX={tx} RX={rx}");
        }
        AntennaCmd::SetPorts { ports } => {
            let mut list = heapless::Vec::<(u8, u8), 32>::new();
            for p in &ports {
                let (tx, rx) = args::parse_pair_u8(p)?;
                list.push((tx, rx))
                    .map_err(|_| "逻辑天线对最多 32 组".to_string())?;
            }
            let mut d = heapless::Vec::<u8, 72>::new();
            SetAntennaRequest::InventoryPorts(list).encode(&mut d)?;
            send_ok(r, ctx, 0x91, &d)?;
            println!("已设置 {} 对盘存逻辑天线", ports.len());
        }
        AntennaCmd::SetPower {
            antenna,
            read,
            write,
            setting_time_us,
        } => {
            let mut cfgs = heapless::Vec::<AntennaPower, 32>::new();
            let _ = cfgs.push(AntennaPower {
                antenna,
                read_power: read,
                write_power: write,
                setting_time_us: setting_time_us.unwrap_or(0),
            });
            let req = match setting_time_us {
                Some(_) => SetAntennaRequest::PowerAndSettingTime { cfgs },
                None => SetAntennaRequest::Power { cfgs },
            };
            let mut d = heapless::Vec::<u8, 32>::new();
            req.encode(&mut d)?;
            send_ok(r, ctx, 0x91, &d)?;
            println!(
                "已设置天线 {antenna} 功率：read={read} ({}dBm) write={write} ({}dBm)",
                read as f32 / 100.0,
                write as f32 / 100.0
            );
        }
    }
    Ok(())
}

fn power(r: &mut Reader, ctx: &Ctx, range: bool) -> Res {
    let mut d = heapless::Vec::<u8, 4>::new();
    get::encode_power_query(range, &mut d);
    let rx = r.transact(0x62, &d, ctx.timeout(Duration::ZERO))?;
    let info = get::parse_power_info(&rx.view())?;
    let fmt = |v: u16| format!("{v} ({}dBm)", v as f32 / 100.0);
    println!("默认读功率：{}", fmt(info.default_read_power));
    if let (Some(max), Some(min)) = (info.max_power, info.min_power) {
        println!("最大功率：  {}", fmt(max));
        println!("最小功率：  {}", fmt(min));
    }
    Ok(())
}

fn frequency(r: &mut Reader, ctx: &Ctx, cmd: FrequencyCmd) -> Res {
    match cmd {
        FrequencyCmd::Get => {
            let rx = r.transact(0x65, &[], ctx.timeout(Duration::ZERO))?;
            match get::parse_frequency_hop(&rx.view())? {
                FrequencyHopResponse::Table(table) => {
                    println!("跳频表（{} 频点）：", table.len());
                    for (i, f) in table.iter().enumerate() {
                        println!("  [{i:02}] {f} kHz");
                    }
                }
                FrequencyHopResponse::DwellTime { timeout_ms } => {
                    println!("天线驻留时间：{timeout_ms} ms");
                }
            }
        }
        FrequencyCmd::Set { khz } => {
            let mut freqs = heapless::Vec::<u32, 50>::new();
            for f in &khz {
                freqs
                    .push(*f)
                    .map_err(|_| "跳频表最多 50 个频点".to_string())?;
            }
            let mut d = heapless::Vec::<u8, 248>::new();
            SetFrequencyHopRequest::FrequencyTable(freqs).encode(&mut d)?;
            send_ok(r, ctx, 0x95, &d)?;
            println!("已设置跳频表（{} 频点）", khz.len());
        }
        FrequencyCmd::Dwell => {
            let mut d = heapless::Vec::<u8, 4>::new();
            get::encode_dwell_time_query(&mut d);
            let rx = r.transact(0x65, &d, ctx.timeout(Duration::ZERO))?;
            match get::parse_frequency_hop(&rx.view())? {
                FrequencyHopResponse::DwellTime { timeout_ms } => {
                    println!("天线驻留时间：{timeout_ms} ms");
                }
                _ => println!("设备返回了非预期的跳频表"),
            }
        }
        FrequencyCmd::SetDwell { ms } => {
            let mut d = heapless::Vec::<u8, 8>::new();
            SetFrequencyHopRequest::DwellTime { timeout_ms: ms }.encode(&mut d)?;
            send_ok(r, ctx, 0x95, &d)?;
            println!("已设置天线驻留时间：{ms} ms");
        }
    }
    Ok(())
}

fn gpi(r: &mut Reader, ctx: &Ctx) -> Res {
    let rx = r.transact(0x66, &[], ctx.timeout(Duration::ZERO))?;
    let levels = get::parse_gpi(&rx.view())?;
    println!("GPI 电平（{} 路）：", levels.len());
    for (i, v) in levels.iter().enumerate() {
        println!("  GPI{i}: {v}");
    }
    Ok(())
}

fn gpo(r: &mut Reader, ctx: &Ctx, entries: &[String]) -> Res {
    let mut pairs = [(0u8, 0u8); 2];
    for (i, entry) in entries.iter().enumerate() {
        let (num, status) = args::parse_pair_u8(entry)?;
        if !(1..=2).contains(&num) {
            return Err(format!("GPO 编号必须为 1-2，得到 {num}").into());
        }
        if status > 1 {
            return Err(format!("GPO 状态必须为 0/1，得到 {status}").into());
        }
        pairs[i] = (num, status);
    }
    let mut d = heapless::Vec::<u8, 8>::new();
    SetGpoRequest {
        pairs,
        count: entries.len(),
    }
    .encode(&mut d)?;
    send_ok(r, ctx, 0x96, &d)?;
    println!("已设置 {} 路 GPO", entries.len());
    Ok(())
}

fn reader_config(r: &mut Reader, ctx: &Ctx, cmd: ReaderConfigCmd) -> Res {
    match cmd {
        ReaderConfigCmd::Get { key } => {
            let mut d = heapless::Vec::<u8, 8>::new();
            get::encode_reader_config_query(key, &mut d);
            let rx = r.transact(0x6A, &d, ctx.timeout(Duration::ZERO))?;
            let resp = get::parse_reader_config(&rx.view())?;
            println!("读写器配置：Key=0x{:02X} Value={}", resp.key, resp.value);
        }
        ReaderConfigCmd::Set { key, value } => {
            let key = ReaderConfigKey::from_u8(key)
                .ok_or_else(|| format!("配置键 0x{key:02X} 不支持（仅 0x00/0x06/0x08）"))?;
            let mut d = heapless::Vec::<u8, 8>::new();
            SetReaderConfigRequest { key, value }.encode(&mut d)?;
            send_ok(r, ctx, 0x9A, &d)?;
            println!("已设置读写器配置：Key={:#04X} Value={value}", key.value());
        }
    }
    Ok(())
}

fn protocol_param(r: &mut Reader, ctx: &Ctx, cmd: ProtocolParamCmd) -> Res {
    match cmd {
        ProtocolParamCmd::Get {
            protocol,
            parameter,
        } => {
            let mut d = heapless::Vec::<u8, 8>::new();
            get::encode_protocol_config_query(protocol, parameter, &mut d);
            let rx = r.transact(0x6B, &d, ctx.timeout(Duration::ZERO))?;
            let resp = get::parse_protocol_config(&rx.view())?;
            println!(
                "协议参数：Parameter=0x{:02X} Option={:?} Value={}",
                resp.parameter, resp.option, resp.value
            );
        }
        ProtocolParamCmd::Session { protocol, session } => {
            set_protocol_param(r, ctx, protocol, ProtocolConfigParam::Session(session))?;
        }
        ProtocolParamCmd::Target {
            protocol,
            option,
            value,
        } => {
            set_protocol_param(
                r,
                ctx,
                protocol,
                ProtocolConfigParam::Target { option, value },
            )?;
        }
        ProtocolParamCmd::RfMode { protocol, value } => {
            set_protocol_param(r, ctx, protocol, ProtocolConfigParam::RfMode(value))?;
        }
        ProtocolParamCmd::Q {
            protocol,
            option,
            value,
        } => {
            set_protocol_param(r, ctx, protocol, ProtocolConfigParam::Q { option, value })?;
        }
    }
    Ok(())
}

fn set_protocol_param(r: &mut Reader, ctx: &Ctx, protocol: u8, param: ProtocolConfigParam) -> Res {
    let mut d = heapless::Vec::<u8, 8>::new();
    SetProtocolConfigRequest { protocol, param }.encode(&mut d)?;
    send_ok(r, ctx, 0x9B, &d)?;
    println!("已设置协议参数（Parameter=0x{:02X}）", param.parameter());
    Ok(())
}
