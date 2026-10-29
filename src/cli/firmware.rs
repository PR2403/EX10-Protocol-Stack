//! `firmware` / `raw` 命令组：Bootloader 层 Flash 操作与原始帧调试。

use std::time::Duration;

use uhf_ex10::command::bootloader::{
    ReadFlashRequest, VerifyFirmwareRequest, WriteFlashRequest, check_crc,
};

use super::args;
use super::format;
use super::{Ctx, FirmwareCmd, RawCmd, Reader, Res, ensure_boot};

pub fn crc_offline(data_hex: &str) -> Res {
    let data = args::parse_hex_bytes(data_hex)?;
    if data.is_empty() || !data.len().is_multiple_of(4) {
        return Err("数据必须非空且字节数为 4 的倍数".into());
    }
    println!(
        "CHECK_DATA_LEN={} CHECKCRC=0x{:08X}",
        data.len() / 4,
        check_crc(&data)
    );
    Ok(())
}

pub fn run(r: &mut Reader, ctx: &Ctx, cmd: FirmwareCmd) -> Res {
    match cmd {
        FirmwareCmd::Read { address, words } => {
            ensure_boot(r)?;
            let req = ReadFlashRequest {
                read_addr: address,
                read_len_words: words,
            };
            let mut d = heapless::Vec::<u8, 16>::new();
            req.encode(&mut d)?;
            let rx = r.transact(0x02, &d, ctx.timeout(Duration::ZERO))?;
            println!(
                "Flash @ 0x{address:08X}（{words} 字）：{}",
                format::hex(&rx.data)
            );
        }
        FirmwareCmd::Write {
            address,
            data,
            last,
        } => {
            ensure_boot(r)?;
            let data = args::parse_hex_bytes(&data)?;
            if data.is_empty() || !data.len().is_multiple_of(4) {
                return Err("写入数据必须非空且字节数为 4 的倍数".into());
            }
            if data.len() > 240 {
                return Err("单次写入数据最多 240 字节（60 字）".into());
            }
            let req = WriteFlashRequest {
                fin_flag: if last { 0xFF } else { 0x00 },
                write_addr: address,
                write_data: &data,
            };
            let mut d = heapless::Vec::<u8, 248>::new();
            req.encode(&mut d)?;
            let _ = r.transact(0x01, &d, ctx.timeout(Duration::from_secs(2)))?;
            println!(
                "已写入 {} 字节 @ 0x{address:08X}{}",
                data.len(),
                if last { "（最后一块）" } else { "" }
            );
        }
        FirmwareCmd::Verify {
            address,
            data,
            length_words,
            check_crc: explicit_crc,
        } => {
            ensure_boot(r)?;
            let (words, crc) = match data {
                Some(data) => {
                    if length_words.is_some() || explicit_crc.is_some() {
                        return Err(
                            "--data 已包含全部数据，不能与 --length-words/--check-crc 同时使用"
                                .into(),
                        );
                    }
                    let data = args::parse_hex_bytes(&data)?;
                    if data.is_empty() || !data.len().is_multiple_of(4) {
                        return Err("数据必须非空且字节数为 4 的倍数".into());
                    }
                    ((data.len() / 4) as u32, check_crc(&data))
                }
                None => {
                    let words = length_words
                        .ok_or("缺少 --data 时需要提供 --length-words（写入总字节数/4）")?;
                    let crc =
                        explicit_crc.ok_or("缺少 --data 时需要提供 --check-crc（CHECKCRC）")?;
                    (words, crc)
                }
            };
            let req = VerifyFirmwareRequest {
                check_addr: address,
                check_data_len: words,
                check_crc: crc,
            };
            let mut d = heapless::Vec::<u8, 32>::new();
            req.encode(&mut d)?;
            let _ = r.transact(0x08, &d, ctx.timeout(Duration::from_secs(2)))?;
            println!(
                "固件校验成功：Addr=0x{address:08X} CHECK_DATA_LEN={words} CHECKCRC=0x{crc:08X}"
            );
        }
        FirmwareCmd::Crc { data } => crc_offline(&data)?,
    }
    Ok(())
}

pub fn run_raw(r: &mut Reader, ctx: &Ctx, cmd: RawCmd) -> Res {
    match cmd {
        RawCmd::Cmd {
            opcode,
            data,
            timeout_secs,
        } => {
            let data = args::parse_hex_bytes(&data)?;
            let rx = r.transact(
                opcode,
                &data,
                ctx.timeout(Duration::from_secs(timeout_secs)),
            )?;
            println!(
                "Rx: Op=0x{:02X} Status=0x{:04X} Data={}",
                rx.opcode,
                rx.status,
                format::hex(&rx.data)
            );
            let view = rx.view();
            if view.is_extended() {
                println!("    SubCode=0x{:04X}", view.subcode().unwrap_or(0));
                println!("    SubData={}", format::hex(view.ext_payload()?));
            }
        }
        RawCmd::Ext {
            subcode,
            data,
            timeout_secs,
        } => {
            let data = args::parse_hex_bytes(&data)?;
            let rx = r.transact_ext(
                subcode,
                &data,
                ctx.timeout(Duration::from_secs(timeout_secs)),
            )?;
            let view = rx.view();
            println!(
                "Rx: SubCode=0x{:04X} Status=0x{:04X} SubData={}",
                subcode,
                rx.status,
                format::hex(view.ext_payload()?)
            );
        }
    }
    Ok(())
}
