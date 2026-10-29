//! L4 传输层（`std` feature）：Link 抽象 + 串口 UhfReader。
//!
//! 串口纪律（协议 §3）：
//! - **一发一收**：请求发出后未收到应答前不得发下一条；
//! - 应答超时 = 5s + 指令执行时间（回波检测 30s）；
//! - 0x09 成功后额外等待 250ms；
//! - 等待应答期间收到的 0xAA 通知帧（标签/心跳/轮询）忽略（异步模式用
//!   [`UhfReader::next_notification`] 专门收）。

pub mod serial;

use crate::error::{EncodeError, Error};
use crate::frame::{
    builder::{build_extended, build_request},
    decoder::{Outcome, StreamDecoder},
    FrameBuf, RawFrame,
};
use core::time::Duration;
use std::time::Instant;

/// 应答超时基数（协议 §3 ⑷：5s + 指令执行时间）。
pub const RESPONSE_TIMEOUT_BASE: Duration = Duration::from_secs(5);
/// 回波检测建议超时（p.150：FCC 全频段约 25s）。
pub const ECHO_DETECT_TIMEOUT: Duration = Duration::from_secs(30);

/// 字节链路抽象（串口只是默认实现；后续可接 BLE/TCP/MCU UART）。
pub trait Link {
    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()>;
    /// 带超时读取；超时返回 `Ok(0)`。
    fn read(&mut self, out: &mut [u8], timeout: Duration) -> std::io::Result<usize>;
}

/// 传输层错误。
#[derive(Debug)]
pub enum TransportError {
    Io(std::io::Error),
    /// 请求编码失败。
    Encode(EncodeError),
    /// 设备返回非 0 状态码（0xAA49/0xAA59 表示异步盘存被打断）。
    Status(crate::status::Status),
    /// 应答超时。
    Timeout,
    /// 应答数据解析失败。
    Decode(crate::error::DecodeError),
    /// 协议层错误。
    Protocol(Error),
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TransportError::Io(e) => write!(f, "io error: {e}"),
            TransportError::Encode(e) => write!(f, "encode error: {e}"),
            TransportError::Status(s) => write!(f, "device status: {s}"),
            TransportError::Timeout => write!(f, "response timeout"),
            TransportError::Decode(e) => write!(f, "decode error: {e}"),
            TransportError::Protocol(e) => write!(f, "protocol error: {e}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransportError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        TransportError::Io(e)
    }
}

impl From<EncodeError> for TransportError {
    fn from(e: EncodeError) -> Self {
        TransportError::Encode(e)
    }
}

impl From<crate::status::Status> for TransportError {
    fn from(s: crate::status::Status) -> Self {
        TransportError::Status(s)
    }
}

impl From<Error> for TransportError {
    fn from(e: Error) -> Self {
        match e {
            Error::Status(s) => TransportError::Status(s),
            Error::Timeout => TransportError::Timeout,
            Error::Encode(e) => TransportError::Encode(e),
            Error::Decode(e) => TransportError::Decode(e),
        }
    }
}

/// 一帧已收响应（Data 域已拷贝，可自由解析/返回）。
#[derive(Debug, Clone)]
pub struct RxFrame {
    pub opcode: u8,
    pub status: u16,
    pub data: FrameBuf,
}

impl RxFrame {
    /// 以 [`RawFrame`] 视角解析（CRC 已在收帧时校验）。
    pub fn view(&self) -> RawFrame<'_> {
        RawFrame {
            length: self.data.len() as u8,
            opcode: self.opcode,
            status: self.status,
            data: &self.data,
        }
    }

    /// 扩展帧 SubCode。
    pub fn subcode(&self) -> Option<u16> {
        self.view().subcode()
    }

    /// 设备→主机 Data 域（扩展帧已剥壳）。
    pub fn payload(&self) -> Result<&[u8], crate::error::DecodeError> {
        self.view().ext_payload()
    }
}

/// EX10 读卡器高层驱动：串行化请求、等待应答、分发通知。
pub struct UhfReader<L: Link> {
    link: L,
    dec: StreamDecoder,
    /// 异步盘存是否处于激活状态（收到 0xAA49/0xAA59 打断码时清除）。
    async_active: bool,
}

impl<L: Link> UhfReader<L> {
    pub fn new(link: L) -> Self {
        UhfReader {
            link,
            dec: StreamDecoder::new(),
            async_active: false,
        }
    }

    pub fn link_mut(&mut self) -> &mut L {
        &mut self.link
    }

    /// 异步盘存是否激活。
    pub fn async_active(&self) -> bool {
        self.async_active
    }

    // ------------------------------------------------------------------
    // 一发一收核心
    // ------------------------------------------------------------------

    /// 发送通用指令并等待同 opcode 应答。
    pub fn transact(
        &mut self,
        opcode: u8,
        data: &[u8],
        timeout: Duration,
    ) -> Result<RxFrame, TransportError> {
        let mut tx = FrameBuf::new();
        build_request(opcode, data, &mut tx)?;
        self.tx_and_wait(&tx, opcode, None, timeout)
    }

    /// 发送扩展指令并等待同 SubCode 应答。
    pub fn transact_ext(
        &mut self,
        subcode: u16,
        subdata: &[u8],
        timeout: Duration,
    ) -> Result<RxFrame, TransportError> {
        let mut tx = FrameBuf::new();
        build_extended(subcode, subdata, &mut tx)?;
        self.tx_and_wait(&tx, crate::frame::EXT_OPCODE, Some(subcode), timeout)
    }

    fn tx_and_wait(
        &mut self,
        tx: &[u8],
        opcode: u8,
        subcode: Option<u16>,
        timeout: Duration,
    ) -> Result<RxFrame, TransportError> {
        self.link.write_all(tx)?;
        let deadline = Instant::now() + timeout;
        let mut chunk = [0u8; 64];
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(TransportError::Timeout);
            }
            let wait = (deadline - now).min(Duration::from_millis(100));
            let n = self.link.read(&mut chunk, wait)?;
            for &b in &chunk[..n] {
                if let Some(frame) = self.absorb(b, opcode, subcode)? {
                    return Ok(frame);
                }
            }
        }
    }

    /// 吸收一个字节；若产出匹配应答返回 Some，通知帧被忽略。
    fn absorb(
        &mut self,
        byte: u8,
        opcode: u8,
        subcode: Option<u16>,
    ) -> Result<Option<RxFrame>, TransportError> {
        let dec = &mut self.dec;
        match dec.feed(byte) {
            Outcome::NeedMore | Outcome::Resynced => Ok(None),
            Outcome::Frame(f) => {
                // 0xAA49/0xAA59 打断码：异步盘存被其它指令打断
                if f.status == 0xAA49 || f.status == 0xAA59 {
                    self.async_active = false;
                }
                let matched = if opcode == crate::frame::EXT_OPCODE {
                    match f.subcode() {
                        Some(sc) => Some(sc) == subcode,
                        // 非扩展壳的 0xAA 帧 = 通知包，忽略
                        None => false,
                    }
                } else {
                    f.opcode == opcode
                };
                if !matched {
                    return Ok(None);
                }
                if f.status != 0 {
                    return Err(TransportError::Status(crate::status::Status::from_u16(
                        f.status,
                    )));
                }
                Ok(Some(RxFrame {
                    opcode: f.opcode,
                    status: f.status,
                    data: f.data.iter().copied().collect(),
                }))
            }
        }
    }

    /// 等待下一帧通知包（异步盘存期间使用）。
    pub fn next_notification(&mut self, timeout: Duration) -> Result<RxFrame, TransportError> {
        let deadline = Instant::now() + timeout;
        let mut chunk = [0u8; 64];
        loop {
            let now = Instant::now();
            if now >= deadline {
                return Err(TransportError::Timeout);
            }
            let wait = (deadline - now).min(Duration::from_millis(100));
            let n = self.link.read(&mut chunk, wait)?;
            for &b in &chunk[..n] {
                match self.dec.feed(b) {
                    Outcome::Frame(f) => {
                        if f.status == 0xAA49 || f.status == 0xAA59 {
                            self.async_active = false;
                            return Err(TransportError::Status(crate::status::Status::from_u16(
                                f.status,
                            )));
                        }
                        if f.opcode == crate::frame::EXT_OPCODE && !f.is_extended() {
                            return Ok(RxFrame {
                                opcode: f.opcode,
                                status: f.status,
                                data: f.data.iter().copied().collect(),
                            });
                        }
                        // 其它响应帧（如停止指令的应答）跳过
                    }
                    Outcome::NeedMore | Outcome::Resynced => {}
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // 高层封装（常用指令；返回 owned 结果）
    // ------------------------------------------------------------------

    fn timeout_with_hint(hint: Duration) -> Duration {
        RESPONSE_TIMEOUT_BASE + hint
    }

    /// 0x03 获取版本。
    pub fn get_version(&mut self) -> Result<crate::command::bootloader::VersionInfo, TransportError> {
        let rx = self.transact(0x03, &[], Self::timeout_with_hint(Duration::ZERO))?;
        crate::command::bootloader::parse_version(&rx.view()).map_err(TransportError::Decode)
    }

    /// 0x0C 获取运行阶段。
    pub fn get_run_stage(&mut self) -> Result<crate::command::bootloader::RunStage, TransportError> {
        let rx = self.transact(0x0C, &[], Self::timeout_with_hint(Duration::ZERO))?;
        crate::command::bootloader::parse_run_stage(&rx.view()).map_err(TransportError::Decode)
    }

    /// 0x04 启动 Firmware（Boot → APP）。
    pub fn boot_firmware(
        &mut self,
    ) -> Result<crate::command::bootloader::VersionInfo, TransportError> {
        let rx = self.transact(0x04, &[], Self::timeout_with_hint(Duration::from_secs(2)))?;
        crate::command::bootloader::parse_version(&rx.view()).map_err(TransportError::Decode)
    }

    /// 0x09 回到 Bootloader 层（成功后本方法自动等待 250ms，p.27）。
    pub fn reset_to_bootloader(&mut self) -> Result<(), TransportError> {
        let _ = self.transact(0x09, &[], Self::timeout_with_hint(Duration::ZERO))?;
        std::thread::sleep(Duration::from_millis(250));
        Ok(())
    }

    /// 0x10 获取序列号。
    pub fn get_serial(&mut self) -> Result<crate::command::bootloader::SerialNumber, TransportError> {
        let rx = self.transact(0x10, &[0x00, 0x00], Self::timeout_with_hint(Duration::ZERO))?;
        crate::command::bootloader::parse_serial(&rx.view()).map_err(TransportError::Decode)
    }

    /// 0x72 获取温度（有符号 ℃）。
    pub fn get_temperature(&mut self) -> Result<i8, TransportError> {
        let rx = self.transact(0x72, &[], Self::timeout_with_hint(Duration::ZERO))?;
        crate::command::get::parse_temperature(&rx.view()).map_err(TransportError::Decode)
    }

    /// 0x67 获取当前区域。
    pub fn get_region(&mut self) -> Result<crate::types::region::RegionCode, TransportError> {
        let rx = self.transact(0x67, &[], Self::timeout_with_hint(Duration::ZERO))?;
        crate::command::get::parse_region(&rx.view()).map_err(TransportError::Decode)
    }

    /// 0x61(0) 获取标签访问天线。
    pub fn get_access_antenna(&mut self) -> Result<(u8, u8), TransportError> {
        let mut d = heapless::Vec::<u8, 8>::new();
        crate::command::get::encode_antenna_query(
            crate::command::get::AntennaQueryOption::Access,
            &mut d,
        );
        let rx = self.transact(0x61, &d, Self::timeout_with_hint(Duration::ZERO))?;
        match crate::command::get::parse_antenna_config(
            &rx.view(),
            crate::command::get::AntennaQueryOption::Access,
        )
        .map_err(TransportError::Decode)?
        {
            crate::command::get::AntennaConfigResponse::Access { tx, rx } => Ok((tx, rx)),
            _ => unreachable!("Access query always yields Access response"),
        }
    }

    /// 0x91 设置标签访问天线。
    pub fn set_access_antenna(&mut self, tx: u8, rx: u8) -> Result<(), TransportError> {
        let mut d = heapless::Vec::<u8, 8>::new();
        crate::command::set::SetAntennaRequest::AccessPort { tx, rx }.encode(&mut d)?;
        let _ = self.transact(0x91, &d, Self::timeout_with_hint(Duration::ZERO))?;
        Ok(())
    }

    /// 0x91 设置单天线功率（读/写 0.01dBm）。
    pub fn set_antenna_power(
        &mut self,
        antenna: u8,
        read_power: u16,
        write_power: u16,
    ) -> Result<(), TransportError> {
        let mut cfgs = heapless::Vec::new();
        let _ = cfgs.push(crate::command::set::AntennaPower {
            antenna,
            read_power,
            write_power,
            setting_time_us: 0,
        });
        let mut d = heapless::Vec::<u8, 32>::new();
        crate::command::set::SetAntennaRequest::Power { cfgs }.encode(&mut d)?;
        let _ = self.transact(0x91, &d, Self::timeout_with_hint(Duration::ZERO))?;
        Ok(())
    }

    /// 0x22 同步盘存，返回盘存到的标签数。
    pub fn inventory(&mut self, timeout_ms: u16) -> Result<u32, TransportError> {
        let req = crate::command::inventory::SyncInventoryRequest {
            fastid: false,
            search_flags: crate::command::inventory::SearchFlags::default(),
            timeout_ms,
            singulation: crate::types::select::TagSingulation::default(),
            embedded: None,
        };
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d)?;
        let rx = self.transact(
            0x22,
            &d,
            Self::timeout_with_hint(Duration::from_millis(timeout_ms as u64)),
        )?;
        let view = rx.view();
        let r = crate::command::inventory::parse_sync_inventory(&view)
            .map_err(TransportError::Decode)?;
        Ok(r.tags_found)
    }

    /// 0x29 读取标签缓冲区 → owned 标签列表。
    pub fn read_tag_buffer(
        &mut self,
        flags: crate::types::metadata::MetadataFlags,
    ) -> Result<heapless::Vec<OwnedTag, 64>, TransportError> {
        let req = crate::command::inventory::GetTagBufferRequest {
            metadata_flags: flags,
            read_option: crate::command::inventory::ReadOption::New,
        };
        let mut d = heapless::Vec::<u8, 8>::new();
        req.encode(&mut d)?;
        let rx = self.transact(0x29, &d, Self::timeout_with_hint(Duration::ZERO))?;
        let view = rx.view();
        let cursor = crate::command::inventory::TagBufferCursor::parse(&view)
            .map_err(TransportError::Decode)?;
        let mut out = heapless::Vec::new();
        for t in cursor {
            let t = t.map_err(TransportError::Decode)?;
            let _ = out.push(OwnedTag::from(&t));
        }
        Ok(out)
    }

    /// 0x28 读存储区（无过滤），返回 owned 数据。
    pub fn read_memory(
        &mut self,
        bank: crate::types::bank::MemBank,
        address: u32,
        words: u8,
    ) -> Result<heapless::Vec<u8, 192>, TransportError> {
        let req = crate::command::access::ReadTagRequest::new(
            1000,
            None,
            bank,
            address,
            words,
            crate::types::select::TagSingulation::default(),
        );
        let mut d = heapless::Vec::<u8, 64>::new();
        req.encode(&mut d)?;
        let rx = self.transact(0x28, &d, Self::timeout_with_hint(Duration::from_secs(1)))?;
        let view = rx.view();
        let r = crate::command::access::parse_read_tag(&view).map_err(TransportError::Decode)?;
        let mut data = heapless::Vec::new();
        data.extend_from_slice(r.read_data)
            .map_err(|_| TransportError::Decode(crate::error::DecodeError::BadFrame))?;
        Ok(data)
    }

    /// 0x24 写存储区（无过滤）。
    pub fn write_memory(
        &mut self,
        bank: crate::types::bank::MemBank,
        address: u32,
        data: &[u8],
    ) -> Result<(), TransportError> {
        let req = crate::command::access::WriteTagRequest {
            timeout_ms: 1000,
            read_after_write: None,
            singulation: crate::types::select::TagSingulation::default(),
            address,
            mem_bank: bank,
            data,
        };
        let mut d = heapless::Vec::<u8, 96>::new();
        req.encode(&mut d)?;
        let _ = self.transact(0x24, &d, Self::timeout_with_hint(Duration::from_secs(1)))?;
        Ok(())
    }

    /// 0xAA4A 回波检测（天线 + 频段 + 可选指定频点），建议 30s 超时。
    pub fn echo_detect(
        &mut self,
        antenna: u8,
        band: crate::types::region::RegionCode,
        frequencies: &[u32],
    ) -> Result<heapless::Vec<crate::command::misc::EchoResult, 64>, TransportError> {
        let req = crate::command::misc::EchoDetectRequest {
            power: 3000,
            antenna,
            band: band.value(),
            frequencies,
        };
        let mut tx = FrameBuf::new();
        req.encode(&mut tx)?;
        let rx = self.tx_and_wait(
            &tx,
            crate::frame::EXT_OPCODE,
            Some(0xAA4A),
            ECHO_DETECT_TIMEOUT,
        )?;
        let view = rx.view();
        let (_, _, _, results) =
            crate::command::misc::parse_echo_detect(&view).map_err(TransportError::Decode)?;
        Ok(results)
    }

    /// 0xAA48 开始异步盘存。
    pub fn start_async_inventory(
        &mut self,
        control: crate::command::async_inventory::AsyncControlFlags,
    ) -> Result<(), TransportError> {
        let req = crate::command::async_inventory::StartAsyncRequest {
            metadata_flags: crate::types::metadata::MetadataFlags::COMMON,
            control,
            search_flags: crate::command::inventory::SearchFlags {
                mode_bits: 0x03,
                ..Default::default()
            },
            singulation: crate::types::select::TagSingulation::default(),
            embedded: None,
        };
        let mut tx = FrameBuf::new();
        req.encode(&mut tx)?;
        let _ = self.tx_and_wait(
            &tx,
            crate::frame::EXT_OPCODE,
            Some(0xAA48),
            Self::timeout_with_hint(Duration::ZERO),
        )?;
        self.async_active = true;
        Ok(())
    }

    /// 0xAA49 停止异步盘存。
    pub fn stop_async_inventory(&mut self) -> Result<(), TransportError> {
        let mut tx = FrameBuf::new();
        crate::command::async_inventory::build_stop_async(&mut tx)?;
        let r = self.tx_and_wait(
            &tx,
            crate::frame::EXT_OPCODE,
            Some(0xAA49),
            Self::timeout_with_hint(Duration::ZERO),
        );
        self.async_active = false;
        r.map(|_| ())
    }
}

impl UhfReader<serial::SerialLink> {
    /// 打开串口（115200 8N1 出厂默认）。
    pub fn open(port: &str, baud: u32) -> Result<Self, TransportError> {
        Ok(UhfReader::new(serial::SerialLink::open(port, baud)?))
    }
}

/// owned 标签信息（从 [`crate::command::inventory::TagEpcAndMeta`] 拷贝）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OwnedTag {
    pub read_count: Option<u8>,
    pub rssi_dbm: Option<i8>,
    pub antenna_id: Option<u8>,
    pub frequency_khz: Option<u32>,
    pub timestamp_ms: Option<u32>,
    pub phase: Option<u16>,
    pub pc: u16,
    pub epc: heapless::Vec<u8, 64>,
    pub tid: Option<heapless::Vec<u8, 16>>,
    pub tag_crc: u16,
}

impl From<&crate::command::inventory::TagEpcAndMeta<'_>> for OwnedTag {
    fn from(t: &crate::command::inventory::TagEpcAndMeta<'_>) -> Self {
        OwnedTag {
            read_count: t.metadata.read_count,
            rssi_dbm: t.metadata.rssi_dbm,
            antenna_id: t.metadata.antenna_id,
            frequency_khz: t.metadata.frequency_khz,
            timestamp_ms: t.metadata.timestamp_ms,
            phase: t.metadata.phase,
            pc: t.pc,
            epc: t.epc_id.iter().copied().collect(),
            tid: t.tid.map(|tid| tid.iter().copied().collect()),
            tag_crc: t.tag_crc,
        }
    }
}
