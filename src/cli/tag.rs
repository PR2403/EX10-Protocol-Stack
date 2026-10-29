//! `tag` 命令组：第 6 章标签访问 0x23/0x24/0x25/0x26/0x28/0x2D。

use std::time::Duration;

use uhf_ex10::command::access::{
    BlockWriteRequest, KillTagRequest, LockTagRequest, ReadAfterWrite, ReadTagRequest,
    WriteEpcRequest, WriteTagRequest, parse_read_tag, parse_write_read,
};
use uhf_ex10::transport::RxFrame;
use uhf_ex10::types::metadata::MetadataFlags;

use super::args::{self, BankArg};
use super::format;
use super::{Ctx, Reader, Res, TagCmd};

pub fn run(r: &mut Reader, ctx: &Ctx, cmd: TagCmd) -> Res {
    match cmd {
        TagCmd::Read {
            bank,
            address,
            words,
            metadata,
            timeout_ms,
            select,
        } => read(r, ctx, bank, address, words, metadata, timeout_ms, &select),
        TagCmd::Write {
            bank,
            address,
            data,
            timeout_ms,
            read_after,
            read_bank,
            read_address,
            read_words,
            select,
        } => write(
            r,
            ctx,
            bank,
            address,
            &data,
            timeout_ms,
            read_after,
            read_bank,
            read_address,
            read_words,
            &select,
        ),
        TagCmd::WriteEpc {
            new_epc,
            timeout_ms,
            read_after,
            read_bank,
            read_address,
            read_words,
            select,
        } => write_epc(
            r,
            ctx,
            &new_epc,
            timeout_ms,
            read_after,
            read_bank,
            read_address,
            read_words,
            &select,
        ),
        TagCmd::Lock {
            password,
            mask,
            action,
            timeout_ms,
            select,
        } => lock(r, ctx, password, mask, action, timeout_ms, &select),
        TagCmd::Kill {
            password,
            timeout_ms,
            select,
        } => kill(r, ctx, password, timeout_ms, &select),
        TagCmd::BlockWrite {
            bank,
            address,
            data,
            timeout_ms,
            select,
        } => block_write(r, ctx, bank, address, &data, timeout_ms, &select),
    }
}

fn op_timeout(ctx: &Ctx, ms: u16) -> Duration {
    ctx.timeout(Duration::from_millis(ms as u64))
}

fn read_after(flag: bool, bank: BankArg, address: u32, words: u8) -> Option<ReadAfterWrite> {
    flag.then(|| ReadAfterWrite {
        mem_bank: bank.into(),
        address,
        word_count: words,
    })
}

#[allow(clippy::too_many_arguments)]
fn read(
    r: &mut Reader,
    ctx: &Ctx,
    bank: BankArg,
    address: u32,
    words: u8,
    metadata: Option<MetadataFlags>,
    timeout_ms: u16,
    select: &args::SelectionArgs,
) -> Res {
    let singulation = select.build()?;
    let mem_bank = bank.into();
    let req = ReadTagRequest::new(timeout_ms, metadata, mem_bank, address, words, singulation);
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;
    let rx = r.transact(0x28, &d, op_timeout(ctx, timeout_ms))?;
    let view = rx.view();
    let resp = parse_read_tag(&view)?;
    if let (Some(md), Some(flags)) = (&resp.metadata, resp.metadata_flags) {
        format::print_metadata(md, flags);
    }
    println!(
        "读取 {} @ {:#X} {} 字：{}",
        mem_bank,
        address,
        words,
        format::hex(resp.read_data)
    );
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write(
    r: &mut Reader,
    ctx: &Ctx,
    bank: BankArg,
    address: u32,
    data_hex: &str,
    timeout_ms: u16,
    read_after_flag: bool,
    read_bank: BankArg,
    read_address: u32,
    read_words: u8,
    select: &args::SelectionArgs,
) -> Res {
    let data = args::parse_hex_bytes(data_hex)?;
    if data.is_empty() || !data.len().is_multiple_of(2) {
        return Err("写入数据必须为非空偶数长度十六进制".into());
    }
    if data.len() > 64 {
        return Err("0x24 单次最多写 64 字节".into());
    }
    let singulation = select.build()?;
    let raw = read_after(read_after_flag, read_bank, read_address, read_words);
    let req = WriteTagRequest {
        timeout_ms,
        read_after_write: raw,
        singulation,
        address,
        mem_bank: bank.into(),
        data: &data,
    };
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;
    let rx = r.transact(0x24, &d, op_timeout(ctx, timeout_ms))?;
    if raw.is_some() {
        print_write_read(&rx)?;
    } else {
        println!(
            "写入成功：{} 字节 @ {}/{}",
            data.len(),
            bank_name(bank),
            address
        );
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn write_epc(
    r: &mut Reader,
    ctx: &Ctx,
    epc_hex: &str,
    timeout_ms: u16,
    read_after_flag: bool,
    read_bank: BankArg,
    read_address: u32,
    read_words: u8,
    select: &args::SelectionArgs,
) -> Res {
    let epc = args::parse_hex_bytes(epc_hex)?;
    if epc.is_empty() || !epc.len().is_multiple_of(2) {
        return Err("新 EPC 必须为非空偶数长度十六进制".into());
    }
    if epc.len() > 62 {
        return Err("EPC 最长 62 字节（496 位）".into());
    }
    let singulation = select.build()?;
    let raw = read_after(read_after_flag, read_bank, read_address, read_words);
    let req = WriteEpcRequest {
        timeout_ms,
        read_after_write: raw,
        singulation,
        epc: &epc,
    };
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;
    let rx = r.transact(0x23, &d, op_timeout(ctx, timeout_ms))?;
    if raw.is_some() {
        print_write_read(&rx)?;
    } else {
        println!("已写入 EPC：{}", format::hex(&epc));
    }
    Ok(())
}

fn lock(
    r: &mut Reader,
    ctx: &Ctx,
    password: u32,
    mask: u16,
    action: u16,
    timeout_ms: u16,
    select: &args::SelectionArgs,
) -> Res {
    let singulation = select.build_filter_only()?;
    let req = LockTagRequest {
        timeout_ms,
        singulation,
        access_password: password,
        mask_bits: mask,
        action_bits: action,
    };
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;
    let _ = r.transact(0x25, &d, op_timeout(ctx, timeout_ms))?;
    println!("锁定成功：Mask=0x{mask:04X} Action=0x{action:04X}");
    Ok(())
}

fn kill(
    r: &mut Reader,
    ctx: &Ctx,
    password: u32,
    timeout_ms: u16,
    select: &args::SelectionArgs,
) -> Res {
    let singulation = select.build_filter_only()?;
    let req = KillTagRequest {
        timeout_ms,
        singulation,
        kill_password: password,
    };
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;
    let _ = r.transact(0x26, &d, op_timeout(ctx, timeout_ms))?;
    println!("灭活指令执行成功");
    Ok(())
}

fn block_write(
    r: &mut Reader,
    ctx: &Ctx,
    bank: BankArg,
    address: u32,
    data_hex: &str,
    timeout_ms: u16,
    select: &args::SelectionArgs,
) -> Res {
    let data = args::parse_hex_bytes(data_hex)?;
    if data.is_empty() || !data.len().is_multiple_of(2) {
        return Err("块写数据必须为非空偶数长度十六进制".into());
    }
    if data.len() > 96 {
        return Err("0x2D 块写最多 96 字节（48 字）".into());
    }
    let word_count = (data.len() / 2) as u8;
    let singulation = select.build()?;
    let req = BlockWriteRequest {
        timeout_ms,
        singulation,
        mem_bank: bank.into(),
        address,
        word_count,
        data: &data,
    };
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;
    let _ = r.transact(0x2D, &d, op_timeout(ctx, timeout_ms))?;
    println!(
        "块写成功：{word_count} 字 @ {}/{}",
        bank_name(bank),
        address
    );
    Ok(())
}

fn print_write_read(rx: &RxFrame) -> Res {
    let view = rx.view();
    let resp = parse_write_read(&view)?;
    println!(
        "写入成功；回读 WriteReadFlag=0x{:02X} 数据={}",
        resp.write_read_flag,
        format::hex(resp.read_data)
    );
    Ok(())
}

fn bank_name(bank: BankArg) -> &'static str {
    match bank {
        BankArg::Reserved => "Reserved",
        BankArg::Epc => "EPC",
        BankArg::Tid => "TID",
        BankArg::User => "USER",
    }
}
