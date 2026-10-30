//! 全量基准主体:引用 L1-L3 全部指令(编码+解码+bootloader+扩展指令),
//! 供 full / full_fmt 两个 bin 调用;不含 Display/fmt(精确测纯协议逻辑)。

use uhf_ex10::command::access::{
    parse_read_tag, parse_write_read, BlockWriteRequest, KillTagRequest, LockTagRequest,
    ReadTagRequest, WriteEpcRequest, WriteTagRequest,
};
use uhf_ex10::command::async_inventory::{
    build_stop_async, build_stop_ex_async, parse_notification, AsyncControlFlags,
    StartAsyncRequest, StartExAsyncRequest,
};
use uhf_ex10::command::bootloader::{
    check_crc, parse_run_stage, parse_serial, parse_version, ReadFlashRequest,
    VerifyFirmwareRequest, WriteFlashRequest,
};
use uhf_ex10::command::get::{
    encode_antenna_query, encode_dwell_time_query, encode_power_query,
    encode_protocol_config_query, encode_reader_config_query, parse_antenna_config,
    parse_available_regions, parse_frequency_hop, parse_gpi, parse_power_info, parse_protocol,
    parse_protocol_config, parse_reader_config, parse_region, parse_temperature,
    AntennaQueryOption,
};
use uhf_ex10::command::inventory::{
    EpcLenUnit,
    parse_single_inventory, parse_sync_inventory, parse_tag_epc_and_meta, GetTagBufferRequest,
    ReadOption, SearchFlags, SingleInventoryRequest, SyncInventoryRequest,
};
use uhf_ex10::command::misc::{
    convert_temperature, parse_echo_detect, parse_yilian_temp, temp_range_from_pc,
    EchoDetectRequest, LedLightRequest, MultiTagFilterRequest, SelTagEntry, YilianTempRequest,
};
use uhf_ex10::command::persist::{
    build_baudrate_get, build_baudrate_set, build_power_on_to_app_enable,
    build_power_on_to_app_get, build_region_get, build_region_set, keydata_from_frame,
    parse_persist,
};
use uhf_ex10::command::set::{
    AntennaPower, ProtocolConfigParam, ReaderConfigKey, SetAntennaRequest,
    SetFrequencyHopRequest, SetGpoRequest, SetProtocolConfigRequest, SetProtocolRequest,
    SetReaderConfigRequest, SetRegionRequest,
};
use uhf_ex10::frame::builder::build_request;
use uhf_ex10::frame::decoder::StreamDecoder;
use uhf_ex10::frame::RawFrame;
use uhf_ex10::types::bank::MemBank;
use uhf_ex10::types::baud::BaudRate;
use uhf_ex10::types::metadata::{
    encode_metadata, parse_metadata, MetadataFlags, PhaseMode,
};
use uhf_ex10::types::region::{default_hopping_table, RegionCode};
use uhf_ex10::types::select::TagSingulation;

/// 把所有路径的结果折叠进返回值,防止被优化掉。
#[inline(never)]
fn black_box_sink(x: usize) -> usize {
    x
}

/// 一段含元数据的样本数据(供 parse_* 使用)。
const DATA: &[u8] = &[
    0x02, 0xC8, 0x01, 0x0E, 0x18, 0x0B, 0x00, 0x64, 0x1E, 0x00, 0x05, 0x00, 0x12, 0xE2, 0x00,
    0x34, 0x12, 0x00, 0x00, 0xAB, 0xCD,
];

/// 全量协议路径执行,返回折叠结果(体积测量用,不追求语义)。
pub fn run_full() -> i32 {
    let mut acc: usize = 0;
    let mut buf = uhf_ex10::frame::FrameBuf::new();
    let mut out = heapless::Vec::<u8, 256>::new();

    // ---- 帧/解码 ----
    build_request(0x10, &[0x00], &mut buf).ok();
    let mut dec = StreamDecoder::new();
    dec.feed_slice(&buf);
    dec.feed(0xFF);
    if let Ok(f) = RawFrame::parse(&buf) {
        acc ^= f.opcode as usize;
    }

    // ---- 盘存 0x21/0x22/0x29 ----
    SingleInventoryRequest {
        timeout_ms: 500,
        metadata_flags: Some(MetadataFlags::COMMON),
        singulation: TagSingulation::default(),
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    let _ = parse_single_inventory(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));
    SyncInventoryRequest {
        fastid: true,
        search_flags: SearchFlags::default(),
        timeout_ms: 1000,
        singulation: TagSingulation::epc_value(&[0xE2, 0x00]).unwrap_or_default(),
        embedded: None,
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    let _ = parse_sync_inventory(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));
    GetTagBufferRequest {
        metadata_flags: MetadataFlags::COMMON,
        read_option: ReadOption::New,
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    let _ = parse_tag_epc_and_meta(DATA, MetadataFlags::COMMON, EpcLenUnit::Bits2);

    // ---- 访问 0x23/0x24/0x25/0x26/0x28/0x2D ----
    WriteEpcRequest {
        timeout_ms: 500,
        read_after_write: None,
        singulation: TagSingulation::default(),
        epc: &[0xE2, 0x00, 0x34, 0x12],
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    WriteTagRequest {
        timeout_ms: 500,
        read_after_write: None,
        singulation: TagSingulation::default(),
        address: 2,
        mem_bank: MemBank::User,
        data: &[0x12, 0x34],
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    LockTagRequest {
        timeout_ms: 500,
        singulation: TagSingulation::default(),
        access_password: 0x1234_5678,
        mask_bits: 0x03FF,
        action_bits: 0x0155,
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    KillTagRequest {
        timeout_ms: 500,
        singulation: TagSingulation::default(),
        kill_password: 0x1122_3344,
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    ReadTagRequest::new(
        500,
        Some(MetadataFlags::COMMON),
        MemBank::Tid,
        0,
        4,
        TagSingulation::default(),
    )
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    BlockWriteRequest {
        timeout_ms: 500,
        singulation: TagSingulation::default(),
        mem_bank: MemBank::Epc,
        address: 2,
        word_count: 1,
        data: &[0xAB, 0xCD],
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    let _ = parse_write_read(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));
    let _ = parse_read_tag(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));

    // ---- 异步盘存 0xAA48/49/58/59 + 通知 ----
    StartAsyncRequest {
        metadata_flags: MetadataFlags::COMMON,
        control: AsyncControlFlags {
            heartbeat: true,
            single_ant_auto_stop: false,
            polling_notify: true,
        },
        search_flags: SearchFlags::default(),
        singulation: TagSingulation::default(),
        embedded: None,
    }
    .encode({ buf.clear(); &mut buf })
    .ok();
    acc ^= buf.len();
    StartExAsyncRequest {
        sparse_mode: false,
        metadata_flags: MetadataFlags::ALL,
    }
    .encode({ buf.clear(); &mut buf })
    .ok();
    acc ^= buf.len();
    build_stop_async({ buf.clear(); &mut buf }).ok();
    build_stop_ex_async({ buf.clear(); &mut buf }).ok();
    acc ^= buf.len();
    let _ = parse_notification(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));

    // ---- bootloader ----
    WriteFlashRequest {
        fin_flag: 0xFF,
        write_addr: 0x0800_8000,
        write_data: &[0u8; 16],
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    ReadFlashRequest {
        read_addr: 0x0800_8000,
        read_len_words: 4,
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    VerifyFirmwareRequest {
        check_addr: 0x0800_8000,
        check_data_len: 16,
        check_crc: check_crc(&[0u8; 16]),
    }
    .encode(&mut out)
    .ok();
    acc ^= out.len();
    let _ = parse_version(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));
    let _ = parse_run_stage(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));
    let _ = parse_serial(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));

    // ---- get ----
    encode_antenna_query(AntennaQueryOption::Power, &mut out);
    encode_power_query(true, &mut out);
    encode_dwell_time_query(&mut out);
    encode_reader_config_query(0x00, &mut out);
    encode_protocol_config_query(0x05, 0x00, &mut out);
    acc ^= out.len();
    let rf = RawFrame::parse(&buf).unwrap_or(ZERO_FRAME);
    let _ = parse_antenna_config(&rf, AntennaQueryOption::Power);
    let _ = parse_power_info(&rf);
    let _ = parse_protocol(&rf);
    let _ = parse_frequency_hop(&rf);
    let _ = parse_gpi(&rf);
    let _ = parse_region(&rf);
    let _ = parse_reader_config(&rf);
    let _ = parse_protocol_config(&rf);
    let _ = parse_available_regions(&rf);
    let _ = parse_temperature(&rf);

    // ---- set ----
    SetAntennaRequest::AccessPort { tx: 1, rx: 1 }.encode(&mut out).ok();
    let mut ports = heapless::Vec::<(u8, u8), 32>::new();
    let _ = ports.push((1, 1));
    SetAntennaRequest::InventoryPorts(ports).encode(&mut out).ok();
    let mut cfgs = heapless::Vec::<AntennaPower, 32>::new();
    let _ = cfgs.push(AntennaPower {
        antenna: 1,
        read_power: 3000,
        write_power: 3000,
        setting_time_us: 0,
    });
    SetAntennaRequest::Power { cfgs }.encode(&mut out).ok();
    SetProtocolRequest { protocol: 0x0005 }.encode(&mut out).ok();
    let mut table = heapless::Vec::<u32, 50>::new();
    for &f in default_hopping_table(RegionCode::China1) {
        let _ = table.push(f);
    }
    SetFrequencyHopRequest::FrequencyTable(table).encode(&mut out).ok();
    SetFrequencyHopRequest::DwellTime { timeout_ms: 100 }.encode(&mut out).ok();
    SetGpoRequest { pairs: [(1, 1); 2], count: 1 }.encode(&mut out).ok();
    SetRegionRequest { region: RegionCode::China1 }.encode(&mut out).ok();
    SetReaderConfigRequest { key: ReaderConfigKey::MaxRssi, value: 1 }.encode(&mut out).ok();
    SetProtocolConfigRequest { protocol: 0x05, param: ProtocolConfigParam::Session(2) }
        .encode(&mut out)
        .ok();
    acc ^= out.len();

    // ---- persist 0xAA40 ----
    build_power_on_to_app_get({ buf.clear(); &mut buf }).ok();
    build_power_on_to_app_enable({ buf.clear(); &mut buf }).ok();
    build_baudrate_get({ buf.clear(); &mut buf }).ok();
    build_baudrate_set(BaudRate::Baud115200, { buf.clear(); &mut buf }).ok();
    build_region_get({ buf.clear(); &mut buf }).ok();
    build_region_set(0x06, { buf.clear(); &mut buf }).ok();
    keydata_from_frame(&buf).ok();
    let _ = parse_persist(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));
    acc ^= buf.len();

    // ---- misc 0xAA4A/4C/50/51 ----
    EchoDetectRequest {
        power: 3000,
        antenna: 1,
        band: RegionCode::China1.value(),
        frequencies: &[920_625],
    }
    .encode({ buf.clear(); &mut buf })
    .ok();
    acc ^= buf.len();
    let _ = parse_echo_detect(&RawFrame::parse(&buf).unwrap_or(ZERO_FRAME));
    let entry = SelTagEntry {
        bank: 1,
        address: 0,
        bits_len: 16,
        data: {
            let mut d = heapless::Vec::<u8, 32>::new();
            let _ = d.extend_from_slice(&[0xE2, 0x00]);
            d
        },
    };
    MultiTagFilterRequest { entries: &[entry] }.encode({ buf.clear(); &mut buf }).ok();
    YilianTempRequest {
        time_all_ms: 3000,
        time_sel_wait_ms: 0,
        time_read_wait_ms: 0,
        metadata_flags: None,
        mem_bank: 3,
        address: 0,
        word_count: 2,
        singulation: TagSingulation::default(),
    }
    .encode({ buf.clear(); &mut buf })
    .ok();
    acc ^= buf.len();
    let _ = parse_yilian_temp(
        &RawFrame::parse(&buf).unwrap_or(ZERO_FRAME),
        false,
        MetadataFlags::NONE,
        2,
    );
    LedLightRequest { timeout_ms: 1000, option: 0x00 }.encode({ buf.clear(); &mut buf }).ok();
    if let Some(tv) = convert_temperature(temp_range_from_pc(0x0000), [20, 128]) {
        acc ^= tv.to_bits() as usize;
    }

    // ---- types 层直测 ----
    if let Ok((md, used)) = parse_metadata(DATA, MetadataFlags::COMMON, PhaseMode::StartStop) {
        acc ^= used;
        let mut meta_out = heapless::Vec::<u8, 256>::new();
        let _ = encode_metadata(&md, MetadataFlags::COMMON, &mut meta_out);
        acc ^= meta_out.len();
        let _ = md.phase_degrees(PhaseMode::End);
    }

    black_box_sink(acc) as i32
}

/// 解析失败时的兜底帧(长度 0 的空帧;仅在测量路径上使用)。
static ZERO_FRAME: RawFrame<'static> = RawFrame {
    length: 0,
    opcode: 0x10,
    status: 0,
    data: &[],
};
