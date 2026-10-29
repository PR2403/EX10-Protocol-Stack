//! `persist` 命令组：0xAA40 永久保存配置（第 9 章）。

use std::time::Duration;

use uhf_ex10::command::persist::{
    POWER_ON_TO_APP_MAGIC, build_baudrate_get, build_baudrate_set, build_factory_reset, build_get,
    build_power_on_to_app_disable, build_power_on_to_app_enable, build_power_on_to_app_get,
    build_prepare_upgrade, build_protocol_config_get, build_protocol_config_set,
    build_reader_config_get, build_reader_config_set, build_region_get, build_region_set,
    build_set, keydata_from_frame, parse_persist,
};
use uhf_ex10::command::set::{AntennaPower, SetAntennaRequest, SetFrequencyHopRequest};
use uhf_ex10::frame::{EXT_OPCODE, FrameBuf, builder::build_request};
use uhf_ex10::transport::RxFrame;
use uhf_ex10::types::baud::BaudRate;
use uhf_ex10::types::region::RegionCode;

use super::args;
use super::format;
use super::{
    BaudPersistCmd, Ctx, PersistCmd, PersistSwitchCmd, ProtocolConfigPersistCmd, Reader,
    ReaderConfigPersistCmd, RegionPersistCmd, Res,
};

pub fn run(r: &mut Reader, ctx: &Ctx, cmd: PersistCmd) -> Res {
    match cmd {
        PersistCmd::Get { key, keydata } => {
            let keydata = args::parse_hex_bytes(&keydata)?;
            if keydata.len() > 234 {
                return Err("KEYDATA 最长 234 字节（扩展帧 255 字节上限）".into());
            }
            let mut frame = FrameBuf::new();
            build_get(key, &keydata, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
        }
        PersistCmd::Set { key, keydata } => {
            let keydata = args::parse_hex_bytes(&keydata)?;
            if keydata.len() > 234 {
                return Err("KEYDATA 最长 234 字节（扩展帧 255 字节上限）".into());
            }
            let mut frame = FrameBuf::new();
            build_set(key, &keydata, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
        }
        PersistCmd::PowerOnApp(action) => power_on_app(r, ctx, action)?,
        PersistCmd::Baud(action) => baud(r, ctx, action)?,
        PersistCmd::Region(action) => region(r, ctx, action)?,
        PersistCmd::ReaderConfig(action) => reader_config(r, ctx, action)?,
        PersistCmd::ProtocolConfig(action) => protocol_config(r, ctx, action)?,
        PersistCmd::AntennaPower { entries } => antenna_power(r, ctx, &entries)?,
        PersistCmd::Frequency { khz } => frequency(r, ctx, &khz)?,
        PersistCmd::FactoryReset => {
            let mut frame = FrameBuf::new();
            build_factory_reset(&mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
            println!("已恢复出厂设置");
        }
        PersistCmd::PrepareUpgrade => {
            let mut frame = FrameBuf::new();
            build_prepare_upgrade(&mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
            println!("已准备升级；下一步运行 `uhf-demo system reset` 回到 Bootloader 层");
        }
    }
    Ok(())
}

fn send(r: &mut Reader, ctx: &Ctx, frame: &FrameBuf) -> Res<RxFrame> {
    Ok(r.transact_frame(frame, EXT_OPCODE, Some(0xAA40), ctx.timeout(Duration::ZERO))?)
}

fn print_raw(rx: &RxFrame) -> Res {
    let view = rx.view();
    let resp = parse_persist(&view)?;
    println!(
        "KEY=0x{:02X} GETORSET={} KEYDATA={}",
        resp.key,
        resp.getorset,
        format::hex(resp.keydata)
    );
    Ok(())
}

fn power_on_app(r: &mut Reader, ctx: &Ctx, action: PersistSwitchCmd) -> Res {
    let mut frame = FrameBuf::new();
    let label = match action {
        PersistSwitchCmd::Get => {
            build_power_on_to_app_get(&mut frame)?;
            "GET"
        }
        PersistSwitchCmd::Enable => {
            build_power_on_to_app_enable(&mut frame)?;
            "SET 启用"
        }
        PersistSwitchCmd::Disable => {
            build_power_on_to_app_disable(&mut frame)?;
            "SET 取消"
        }
    };
    let rx = send(r, ctx, &frame)?;
    let view = rx.view();
    let resp = parse_persist(&view)?;
    println!(
        "上电直达 APP（{label}）：KEYDATA={}",
        format::hex(resp.keydata)
    );
    if matches!(action, PersistSwitchCmd::Get) && resp.keydata.len() == 4 {
        let v = u32::from_be_bytes([
            resp.keydata[0],
            resp.keydata[1],
            resp.keydata[2],
            resp.keydata[3],
        ]);
        println!(
            "当前状态：{}（0x{v:08X}）",
            if v == POWER_ON_TO_APP_MAGIC {
                "已启用"
            } else {
                "未启用"
            }
        );
    }
    Ok(())
}

fn baud(r: &mut Reader, ctx: &Ctx, action: BaudPersistCmd) -> Res {
    let mut frame = FrameBuf::new();
    match action {
        BaudPersistCmd::Get => {
            build_baudrate_get(&mut frame)?;
            let rx = send(r, ctx, &frame)?;
            let view = rx.view();
            let resp = parse_persist(&view)?;
            if resp.keydata.len() == 4 {
                let raw = u32::from_be_bytes([
                    resp.keydata[0],
                    resp.keydata[1],
                    resp.keydata[2],
                    resp.keydata[3],
                ]);
                match BaudRate::from_u32(raw) {
                    Some(b) => println!("上电默认波特率：{}", b.baud()),
                    None => println!("上电默认波特率：未知（0x{raw:08X}）"),
                }
            } else {
                println!("KEYDATA={}", format::hex(resp.keydata));
            }
        }
        BaudPersistCmd::Set { rate } => {
            build_baudrate_set(rate, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
            println!("已设置上电默认波特率：{}（重新上电生效）", rate.baud());
        }
    }
    Ok(())
}

fn region(r: &mut Reader, ctx: &Ctx, action: RegionPersistCmd) -> Res {
    let mut frame = FrameBuf::new();
    match action {
        RegionPersistCmd::Get => {
            build_region_get(&mut frame)?;
            let rx = send(r, ctx, &frame)?;
            let view = rx.view();
            let resp = parse_persist(&view)?;
            if let Some(&v) = resp.keydata.first() {
                match (RegionCode::from_u8(v), v) {
                    (Some(region), _) => println!("默认区域：{region}"),
                    (None, 0x5A) => println!("默认区域：恢复出厂（0x5A）"),
                    (None, _) => println!("默认区域：未知（0x{v:02X}）"),
                }
            } else {
                println!("KEYDATA={}", format::hex(resp.keydata));
            }
        }
        RegionPersistCmd::Set { region } => {
            build_region_set(region, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
            println!("已设置默认区域：0x{region:02X}");
        }
    }
    Ok(())
}

fn reader_config(r: &mut Reader, ctx: &Ctx, action: ReaderConfigPersistCmd) -> Res {
    let mut frame = FrameBuf::new();
    match action {
        ReaderConfigPersistCmd::Get { option, key } => {
            build_reader_config_get(option, key, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            let view = rx.view();
            let resp = parse_persist(&view)?;
            println!(
                "默认读写器配置：Key=0x{key:02X} KEYDATA={}",
                format::hex(resp.keydata)
            );
        }
        ReaderConfigPersistCmd::Set { key, value } => {
            build_reader_config_set(key, value, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
            println!("已设置默认读写器配置：Key=0x{key:02X} Value={value}");
        }
    }
    Ok(())
}

fn protocol_config(r: &mut Reader, ctx: &Ctx, action: ProtocolConfigPersistCmd) -> Res {
    let mut frame = FrameBuf::new();
    match action {
        ProtocolConfigPersistCmd::Get {
            protocol,
            parameter,
        } => {
            build_protocol_config_get(protocol, parameter, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            let view = rx.view();
            let resp = parse_persist(&view)?;
            println!(
                "默认协议配置：Parameter=0x{parameter:02X} KEYDATA={}",
                format::hex(resp.keydata)
            );
        }
        ProtocolConfigPersistCmd::Set {
            protocol,
            parameter,
            option,
            value,
        } => {
            let mut kd = heapless::Vec::<u8, 8>::new();
            let _ = kd.push(protocol);
            let _ = kd.push(parameter);
            if let Some(o) = option {
                let _ = kd.push(o);
            }
            if let Some(v) = value {
                let _ = kd.push(v);
            }
            build_protocol_config_set(&kd, &mut frame)?;
            let rx = send(r, ctx, &frame)?;
            print_raw(&rx)?;
            println!(
                "已设置默认协议配置：Protocol={protocol} Parameter=0x{parameter:02X} Option={option:?} Value={value:?}"
            );
        }
    }
    Ok(())
}

fn antenna_power(r: &mut Reader, ctx: &Ctx, entries: &[String]) -> Res {
    let mut cfgs = heapless::Vec::<AntennaPower, 32>::new();
    for entry in entries {
        let cfg = parse_power_entry(entry)?;
        cfgs.push(cfg)
            .map_err(|_| "天线功率配置最多 32 项".to_string())?;
    }
    let mut d = heapless::Vec::<u8, 248>::new();
    SetAntennaRequest::PowerAndSettingTime { cfgs }.encode(&mut d)?;
    let mut normal = FrameBuf::new();
    build_request(0x91, &d, &mut normal)?;
    let keydata = keydata_from_frame(&normal)?;
    let mut frame = FrameBuf::new();
    build_set(0x91, keydata, &mut frame)?;
    let rx = send(r, ctx, &frame)?;
    print_raw(&rx)?;
    println!("已永久保存 {} 个天线的功率配置", entries.len());
    Ok(())
}

fn parse_power_entry(s: &str) -> Result<AntennaPower, String> {
    let parts: Vec<&str> = s.split(':').collect();
    if !(3..=4).contains(&parts.len()) {
        return Err(format!(
            "天线功率格式应为 <ant:读功率:写功率[:配置时间μs]>，得到 {s:?}"
        ));
    }
    Ok(AntennaPower {
        antenna: args::parse_num_u8(parts[0])?,
        read_power: args::parse_num_u16(parts[1])?,
        write_power: args::parse_num_u16(parts[2])?,
        setting_time_us: if parts.len() == 4 {
            args::parse_num_u16(parts[3])?
        } else {
            0
        },
    })
}

fn frequency(r: &mut Reader, ctx: &Ctx, khz: &[u32]) -> Res {
    let mut freqs = heapless::Vec::<u32, 50>::new();
    for f in khz {
        freqs
            .push(*f)
            .map_err(|_| "跳频表最多 50 个频点".to_string())?;
    }
    let mut d = heapless::Vec::<u8, 248>::new();
    SetFrequencyHopRequest::FrequencyTable(freqs).encode(&mut d)?;
    let mut normal = FrameBuf::new();
    build_request(0x95, &d, &mut normal)?;
    let keydata = keydata_from_frame(&normal)?;
    let mut frame = FrameBuf::new();
    build_set(0x95, keydata, &mut frame)?;
    let rx = send(r, ctx, &frame)?;
    print_raw(&rx)?;
    println!("已永久保存跳频表（{} 频点）", khz.len());
    Ok(())
}
