//! `system` 命令组：运行阶段/版本/序列号/温度/启动/复位。

use std::time::Duration;

use uhf_ex10::command::get;

use super::format;
use super::{Ctx, Reader, Res, SystemCmd, ensure_app};

/// `info`：0x0C →（Boot 则 0x04）→ 0x03 + 0x10 + 0x67 + 0x72（+ 0x63/0x61）。
pub fn info(r: &mut Reader, ctx: &Ctx) -> Res {
    ensure_app(r, ctx)?;

    let stage = r.get_run_stage()?;
    println!("Stage:      {stage:?}");

    let v = r.get_version()?;
    format::print_version(&v);

    match r.get_serial() {
        Ok(s) => println!(
            "Serial:     {} / {}",
            format::bcd_digits(&s.raw_year),
            format::bcd_digits(&s.raw_serial)
        ),
        Err(e) => println!("Serial:     读取失败 ({e})"),
    }

    match r.get_region() {
        Ok(region) => {
            let temp = r
                .get_temperature()
                .map(|t| format!("  Temp: {t}°C"))
                .unwrap_or_default();
            println!("Region:     {region}{temp}");
        }
        Err(_) => println!("Region:     未知"),
    }

    if let Ok(rx) = r.transact(0x63, &[], ctx.timeout(Duration::ZERO))
        && let Ok(protocol) = get::parse_protocol(&rx.view())
    {
        println!("ProtoCfg:   {protocol:#06X}");
    }
    if let Ok((tx, rx_ant)) = r.get_access_antenna() {
        println!("AccessAnt:  TX={tx} RX={rx_ant}");
    }
    Ok(())
}

pub fn run(r: &mut Reader, _ctx: &Ctx, cmd: SystemCmd) -> Res {
    match cmd {
        SystemCmd::Stage => {
            let stage = r.get_run_stage()?;
            println!("{stage:?}");
        }
        SystemCmd::Version => {
            let v = r.get_version()?;
            format::print_version(&v);
        }
        SystemCmd::Serial => {
            let s = r.get_serial()?;
            println!("Year:   {}", format::bcd_digits(&s.raw_year));
            println!("Serial: {}", format::bcd_digits(&s.raw_serial));
        }
        SystemCmd::Temperature => {
            println!("{}°C", r.get_temperature()?);
        }
        SystemCmd::Boot => {
            let v = r.boot_firmware()?;
            println!("已进入 APP 层");
            format::print_version(&v);
        }
        SystemCmd::Reset => {
            r.reset_to_bootloader()?;
            println!("已回到 Bootloader 层（已等待 250ms）");
        }
    }
    Ok(())
}
