//! `inventory` 命令组：0x21/0x22/0x29 与 0xAA48/49、0xAA58/59。

use std::collections::HashSet;
use std::time::{Duration, Instant};

use uhf_ex10::command::async_inventory::{
    AsyncControlFlags, StartAsyncRequest, StartExAsyncRequest, build_stop_async,
    build_stop_ex_async, parse_notification,
};
use uhf_ex10::command::inventory::{
    EmbeddedRead, GetTagBufferRequest, ReadOption, SearchFlags, SingleInventoryRequest,
    SyncInventoryRequest, TagBufferCursor, parse_single_inventory, parse_sync_inventory,
};
use uhf_ex10::frame::{EXT_OPCODE, FrameBuf};
use uhf_ex10::transport::TransportError;

use super::args;
use super::format;
use super::{
    AsyncStartArgs, Ctx, ExAsyncStartArgs, InventoryBufferArgs, InventoryCmd, InventoryRunArgs,
    InventorySingleArgs, InventorySyncArgs, Reader, Res,
};

pub fn run(r: &mut Reader, ctx: &Ctx, cmd: InventoryCmd) -> Res {
    match cmd {
        InventoryCmd::Run(args) => run_and_buffer(r, ctx, args),
        InventoryCmd::Sync(args) => {
            let _ = sync(r, ctx, &args)?;
            Ok(())
        }
        InventoryCmd::Single(args) => {
            single(r, ctx, args)?;
            Ok(())
        }
        InventoryCmd::Buffer(args) => {
            let _ = buffer(r, ctx, &args)?;
            Ok(())
        }
        InventoryCmd::Start(args) => {
            start_async(r, ctx, args)?;
            Ok(())
        }
        InventoryCmd::Stop => {
            stop_async(r, ctx)?;
            Ok(())
        }
        InventoryCmd::ExStart(args) => {
            start_ex_async(r, ctx, args)?;
            Ok(())
        }
        InventoryCmd::ExStop => {
            stop_ex_async(r, ctx)?;
            Ok(())
        }
    }
}

fn set_antenna_if(r: &mut Reader, antenna: Option<u8>) -> Res {
    if let Some(ant) = antenna {
        r.set_access_antenna(ant, ant)?;
        println!("已设置标签访问天线：TX={ant} RX={ant}");
    }
    Ok(())
}

fn parse_embedded(spec: &str) -> Res<EmbeddedRead> {
    let (bank, address, word_count) = args::parse_embed_read(spec)?;
    Ok(EmbeddedRead {
        mem_bank: bank,
        address,
        word_count,
    })
}

struct SyncOutcome {
    tags_found: u32,
}

fn sync(r: &mut Reader, ctx: &Ctx, args: &InventorySyncArgs) -> Res<SyncOutcome> {
    set_antenna_if(r, args.antenna)?;

    let embedded = match &args.embed_read {
        Some(spec) => Some(parse_embedded(spec)?),
        None => None,
    };
    let search_flags = SearchFlags {
        embedded: embedded.is_some(),
        tagfocus: args.tagfocus,
        yilian_group_temp: false,
        mode_bits: args.mode_bits & 0x03,
    };
    let singulation = args.select.build()?;
    let req = SyncInventoryRequest {
        fastid: args.fastid,
        search_flags,
        timeout_ms: args.timeout_ms,
        singulation,
        embedded,
    };
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;

    let rx = r.transact(
        0x22,
        &d,
        ctx.timeout(Duration::from_millis(args.timeout_ms as u64)),
    )?;
    let view = rx.view();
    let resp = parse_sync_inventory(&view)?;
    println!(
        "盘存 {}ms：发现 {} 个标签（SearchFlags={:#06X}）",
        args.timeout_ms, resp.tags_found, resp.search_flags
    );
    if let Some(e) = &resp.embedded_result {
        println!(
            "嵌入指令 0x{:02X}：成功 {} / 失败 {}，数据={}",
            e.opcode,
            e.succeeded,
            e.failed,
            format::hex(e.data)
        );
    }
    Ok(SyncOutcome {
        tags_found: resp.tags_found,
    })
}

fn buffer(r: &mut Reader, ctx: &Ctx, args: &InventoryBufferArgs) -> Res<usize> {
    let req = GetTagBufferRequest {
        metadata_flags: args.flags,
        read_option: if args.previous {
            ReadOption::Previous
        } else {
            ReadOption::New
        },
    };
    let mut d = heapless::Vec::<u8, 8>::new();
    req.encode(&mut d)?;
    let rx = r.transact(0x29, &d, ctx.timeout(Duration::ZERO))?;
    let view = rx.view();
    let cursor = TagBufferCursor::parse(&view)?;
    println!(
        "标签缓冲：{} 个（Flags={} {}，ReadOption={}）",
        cursor.tag_count(),
        args.flags,
        format::describe_metadata_flags(args.flags),
        if args.previous {
            "上次重复读"
        } else {
            "新标签"
        }
    );
    let total = cursor.tag_count() as usize;
    let mut printed = 0usize;
    for (idx, item) in cursor.enumerate() {
        if let Some(limit) = args.limit
            && printed >= limit
        {
            break;
        }
        let tag = item?;
        format::print_tag(Some(idx), &tag, Some(args.flags));
        printed += 1;
    }
    if printed < total {
        println!("… 其余 {} 条未打印（--limit）", total - printed);
    }
    Ok(total)
}

fn run_and_buffer(r: &mut Reader, ctx: &Ctx, args: InventoryRunArgs) -> Res {
    let outcome = sync(r, ctx, &args.sync)?;
    if outcome.tags_found == 0 {
        return Ok(());
    }
    let buffer_args = InventoryBufferArgs {
        flags: args.buffer_flags,
        previous: false,
        limit: None,
    };
    let _ = buffer(r, ctx, &buffer_args)?;
    Ok(())
}

fn single(r: &mut Reader, ctx: &Ctx, args: InventorySingleArgs) -> Res {
    set_antenna_if(r, args.antenna)?;
    let singulation = args.select.build_filter_only()?;
    let req = SingleInventoryRequest {
        timeout_ms: args.timeout_ms,
        metadata_flags: args.metadata,
        singulation,
    };
    let mut d = heapless::Vec::<u8, 248>::new();
    req.encode(&mut d)?;
    let rx = r.transact(
        0x21,
        &d,
        ctx.timeout(Duration::from_millis(args.timeout_ms as u64)),
    )?;
    let view = rx.view();
    let resp = parse_single_inventory(&view)?;
    if let (Some(md), Some(flags)) = (&resp.metadata, resp.metadata_flags) {
        format::print_metadata(md, flags);
    }
    if resp.epc.len() < 2 {
        return Err("0x21 响应 EPC 域过短".into());
    }
    let pc = u16::from_be_bytes([resp.epc[0], resp.epc[1]]);
    println!(
        "单标签：PC={pc:04X} EPC={} TagCRC=0x{:04X}",
        format::hex(&resp.epc[2..]),
        resp.tag_crc
    );
    Ok(())
}

fn start_async(r: &mut Reader, ctx: &Ctx, args: AsyncStartArgs) -> Res {
    set_antenna_if(r, args.antenna)?;
    let singulation = args.select.build()?;
    let embedded = match &args.embed_read {
        Some(spec) => Some(parse_embedded(spec)?),
        None => None,
    };
    let req = StartAsyncRequest {
        metadata_flags: args.metadata,
        control: AsyncControlFlags {
            heartbeat: args.heartbeat,
            single_ant_auto_stop: args.auto_stop,
            polling_notify: args.polling,
        },
        search_flags: SearchFlags {
            embedded: embedded.is_some(),
            tagfocus: args.tagfocus,
            yilian_group_temp: false,
            mode_bits: args.mode_bits & 0x03,
        },
        singulation,
        embedded,
    };
    let mut frame = FrameBuf::new();
    req.encode(&mut frame)?;
    r.transact_frame(
        &frame,
        EXT_OPCODE,
        Some(0xAA48),
        ctx.timeout(Duration::ZERO),
    )?;
    println!(
        "异步盘存已启动（{}s，Flags={}，心跳={}，轮询通知={}）…",
        args.duration, args.metadata, args.heartbeat, args.polling
    );

    let stats = receive_notifications(r, Duration::from_secs(args.duration))?;
    if stats.interrupted {
        println!("设备已停止盘存，未发送 0xAA49。");
    } else {
        match r.stop_async_inventory() {
            Ok(()) => println!("已发送 0xAA49 停止异步盘存。"),
            Err(e) => eprintln!("停止异步盘存失败：{e}"),
        }
    }
    print_stats(&stats);
    Ok(())
}

fn stop_async(r: &mut Reader, ctx: &Ctx) -> Res {
    let mut frame = FrameBuf::new();
    build_stop_async(&mut frame)?;
    r.transact_frame(
        &frame,
        EXT_OPCODE,
        Some(0xAA49),
        ctx.timeout(Duration::ZERO),
    )?;
    println!("已停止异步盘存（0xAA49）");
    Ok(())
}

fn start_ex_async(r: &mut Reader, ctx: &Ctx, args: ExAsyncStartArgs) -> Res {
    let req = StartExAsyncRequest {
        sparse_mode: args.sparse,
        metadata_flags: args.metadata,
    };
    let mut frame = FrameBuf::new();
    req.encode(&mut frame)?;
    let started = r.transact_frame(
        &frame,
        EXT_OPCODE,
        Some(0xAA58),
        ctx.timeout(Duration::ZERO),
    );
    match started {
        Ok(_) => {}
        Err(TransportError::Timeout) => {
            return Err("EX 异步盘存无应答：该指令仅中国版/CE/INDIA/RUSSIA/PHILIPPINES/JAPAN 等区域版本支持，当前区域可能不支持（手册 p.67）".into());
        }
        Err(e) => return Err(e.into()),
    }
    println!(
        "EX 异步盘存已启动（{}s，{}模式，Flags={}）…",
        args.duration,
        if args.sparse { "稀疏" } else { "密集" },
        args.metadata
    );

    let stats = receive_notifications(r, Duration::from_secs(args.duration))?;
    if stats.interrupted {
        println!("设备已停止盘存，未发送 0xAA59。");
    } else {
        let mut stop = FrameBuf::new();
        build_stop_ex_async(&mut stop)?;
        match r.transact_frame(&stop, EXT_OPCODE, Some(0xAA59), ctx.timeout(Duration::ZERO)) {
            Ok(_) => println!("已发送 0xAA59 停止 EX 异步盘存。"),
            Err(e) => eprintln!("停止 EX 异步盘存失败：{e}"),
        }
    }
    print_stats(&stats);
    Ok(())
}

fn stop_ex_async(r: &mut Reader, ctx: &Ctx) -> Res {
    let mut frame = FrameBuf::new();
    build_stop_ex_async(&mut frame)?;
    r.transact_frame(
        &frame,
        EXT_OPCODE,
        Some(0xAA59),
        ctx.timeout(Duration::ZERO),
    )?;
    println!("已停止 EX 异步盘存（0xAA59）");
    Ok(())
}

struct NotifyStats {
    tag_notifications: u32,
    unique_tags: usize,
    heartbeats: u32,
    rounds: u32,
    interrupted: bool,
}

fn print_stats(stats: &NotifyStats) {
    println!(
        "统计：标签通知 {} 条 / 唯一 EPC {} 个 / 心跳 {} / 轮询完成 {}",
        stats.tag_notifications, stats.unique_tags, stats.heartbeats, stats.rounds
    );
}

fn receive_notifications(r: &mut Reader, duration: Duration) -> Res<NotifyStats> {
    let deadline = Instant::now() + duration;
    let mut stats = NotifyStats {
        tag_notifications: 0,
        unique_tags: 0,
        heartbeats: 0,
        rounds: 0,
        interrupted: false,
    };
    let mut seen: HashSet<Vec<u8>> = HashSet::new();

    while Instant::now() < deadline {
        let remain = deadline.saturating_duration_since(Instant::now());
        let wait = remain.min(Duration::from_millis(500));
        match r.next_notification(wait) {
            Ok(rx) => {
                let view = rx.view();
                match parse_notification(&view) {
                    Ok(notification) => {
                        match &notification {
                            uhf_ex10::command::async_inventory::Notification::Tag(t) => {
                                stats.tag_notifications += 1;
                                seen.insert(t.epc_id.to_vec());
                            }
                            uhf_ex10::command::async_inventory::Notification::Heartbeat {
                                ..
                            } => {
                                stats.heartbeats += 1;
                            }
                            uhf_ex10::command::async_inventory::Notification::PollingRound {
                                ..
                            } => {
                                stats.rounds += 1;
                            }
                        }
                        format::print_notification(
                            &notification,
                            stats.tag_notifications + stats.heartbeats + stats.rounds,
                        );
                    }
                    Err(e) => eprintln!("通知解析失败: {e}"),
                }
            }
            Err(TransportError::Timeout) => {}
            Err(TransportError::Status(s)) if s.value() == 0xAA49 || s.value() == 0xAA59 => {
                println!("盘存被设备打断（状态 {s}）");
                stats.interrupted = true;
                break;
            }
            Err(e) => return Err(e.into()),
        }
    }
    stats.unique_tags = seen.len();
    Ok(stats)
}
