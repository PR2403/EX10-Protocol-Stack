//! `multi-tag` / `echo` / `yilian` 命令组（第 10 章扩展指令）。

use std::time::Duration;

use uhf_ex10::command::misc::{
    EchoDetectRequest, LedLightRequest, MultiTagFilterRequest, SelTagEntry, TempRange,
    YilianTempRequest, convert_temperature, parse_echo_detect, parse_yilian_temp,
    temp_range_from_pc,
};
use uhf_ex10::frame::{EXT_OPCODE, FrameBuf};
use uhf_ex10::transport::ECHO_DETECT_TIMEOUT;
use uhf_ex10::types::metadata::MetadataFlags;
use uhf_ex10::types::region::RegionCode;
use uhf_ex10::types::select::SelectMode;

use super::args;
use super::format;
use super::{Ctx, EchoCmd, MultiTagCmd, Reader, Res, YilianCmd};

pub fn run_multi_tag(r: &mut Reader, ctx: &Ctx, cmd: MultiTagCmd) -> Res {
    match cmd {
        MultiTagCmd::Set { entries } => {
            let parsed = entries
                .iter()
                .map(|e| parse_filter_entry(e))
                .collect::<Result<Vec<_>, _>>()?;
            let req = MultiTagFilterRequest { entries: &parsed };
            let mut frame = FrameBuf::new();
            req.encode(&mut frame)?;
            r.transact_frame(
                &frame,
                EXT_OPCODE,
                Some(0xAA4C),
                ctx.timeout(Duration::ZERO),
            )?;
            println!("已设置多标签匹配过滤（{} 条）", parsed.len());
        }
    }
    Ok(())
}

fn parse_filter_entry(spec: &str) -> Result<SelTagEntry, Box<dyn std::error::Error>> {
    let (mode, address, bits, data) = args::parse_select_spec(spec)?;
    let bank = match mode {
        SelectMode::EpcBank => 1,
        SelectMode::Tid => 2,
        SelectMode::User => 3,
        _ => return Err(format!("多标签过滤不支持模式 {mode:?}").into()),
    };
    if bits > 255 {
        return Err("多标签过滤每条最多 255 位".into());
    }
    let mut buf = heapless::Vec::<u8, 32>::new();
    buf.extend_from_slice(&data)
        .map_err(|_| "多标签过滤数据最长 32 字节".to_string())?;
    Ok(SelTagEntry {
        bank,
        address,
        bits_len: bits as u8,
        data: buf,
    })
}

pub fn run_echo(r: &mut Reader, ctx: &Ctx, cmd: EchoCmd) -> Res {
    match cmd {
        EchoCmd::Detect {
            antenna,
            band,
            frequencies,
        } => {
            let band = match band {
                Some(b) => b,
                None => r.get_region().unwrap_or(RegionCode::NorthAmerica),
            };
            println!(
                "回波检测：天线 {antenna}，区域 {band}，{}（最长 30s）…",
                if frequencies.is_empty() {
                    "全频段".to_string()
                } else {
                    format!("{} 个指定频点", frequencies.len())
                }
            );
            let results = {
                let req = EchoDetectRequest {
                    power: 3000,
                    antenna,
                    band: band.value(),
                    frequencies: &frequencies,
                };
                let mut frame = FrameBuf::new();
                req.encode(&mut frame)?;
                let rx = r.transact_frame(
                    &frame,
                    EXT_OPCODE,
                    Some(0xAA4A),
                    ECHO_DETECT_TIMEOUT + ctx.extra,
                )?;
                let view = rx.view();
                let (_, _, _, results) = parse_echo_detect(&view)?;
                results
            };
            println!(
                "{:>10}  {:>4}  {:>10}  {:>6}",
                "频率(kHz)", "VL", "RL(dB)", "VSWR"
            );
            for e in results.iter() {
                println!(
                    "{:>10}  {:>4}  {:>10.1}  {:>6.2}",
                    e.frequency_khz,
                    e.return_loss_raw,
                    e.return_loss_raw as f32 / 10.0,
                    e.vswr()
                );
            }
        }
    }
    Ok(())
}

pub fn run_yilian(r: &mut Reader, ctx: &Ctx, cmd: YilianCmd) -> Res {
    match cmd {
        YilianCmd::Temperature {
            time_all_ms,
            sel_wait_ms,
            read_wait_ms,
            bank,
            address,
            words,
            metadata,
            select,
        } => {
            let singulation = select.build()?;
            let mem_bank: uhf_ex10::types::bank::MemBank = bank.into();
            let req = YilianTempRequest {
                time_all_ms,
                time_sel_wait_ms: sel_wait_ms,
                time_read_wait_ms: read_wait_ms,
                metadata_flags: metadata,
                mem_bank: mem_bank.value(),
                address,
                word_count: words,
                singulation,
            };
            let mut frame = FrameBuf::new();
            req.encode(&mut frame)?;
            let rx = r.transact_frame(
                &frame,
                EXT_OPCODE,
                Some(0xAA50),
                ctx.timeout(Duration::from_millis(time_all_ms as u64)),
            )?;
            let view = rx.view();
            let resp = parse_yilian_temp(
                &view,
                metadata.is_some(),
                metadata.unwrap_or(MetadataFlags::NONE),
                words as usize,
            )?;
            if let (Some(md), Some(flags)) = (&resp.metadata, resp.metadata_flags) {
                format::print_metadata(md, flags);
            }
            println!("读出数据：{}", format::hex(resp.read_data));
            if resp.read_data.len() >= 2 {
                let raw = [resp.read_data[0], resp.read_data[1]];
                let range = temp_range_from_pc(resp.pc);
                match convert_temperature(range, raw) {
                    Some(t) => println!(
                        "温度：{t:.2}°C（量程 {}）",
                        match range {
                            TempRange::Range30to97 => "-30~97°C",
                            TempRange::Range0to120 => "0~120°C",
                            TempRange::Range40to120 => "-40~120°C",
                            TempRange::Unknown => "未知",
                        }
                    ),
                    None => println!(
                        "温度量程未知（PC=0x{:04X}），原始值={}",
                        resp.pc,
                        format::hex(&raw)
                    ),
                }
            }
            println!(
                "标签：PC={:04X} EPC={} TagCRC=0x{:04X}",
                resp.pc,
                format::hex(resp.epc_id),
                resp.tag_crc
            );
        }
        YilianCmd::Led { timeout_ms, option } => {
            let req = LedLightRequest { timeout_ms, option };
            let mut frame = FrameBuf::new();
            req.encode(&mut frame)?;
            r.transact_frame(
                &frame,
                EXT_OPCODE,
                Some(0xAA51),
                ctx.timeout(Duration::ZERO),
            )?;
            println!("亮灯指令成功：Timeout=0x{timeout_ms:04X} Option=0x{option:02X}");
        }
    }
    Ok(())
}
