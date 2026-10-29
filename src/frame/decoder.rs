//! ★ 流式状态机解码器（本 crate 核心交付物，plan/02-architecture.md §1.4）。
//!
//! 逐字节 feed，免疫粘包 / 断包 / 线路噪声：
//! - `SeekHeader` 丢弃一切非 0xFF 字节；
//! - 帧长非法（`7 + Len > 255`）或 CRC 失败时，在已收缓冲内**从第 1 字节起重扫
//!   0xFF** 重同步（坏帧尾可能恰好含下一帧帧头），否则整体丢弃；
//! - 只解析设备→主机形态（Op 之后恒有 2 字节 Status，通知包亦然，见 p.61-63）；
//! - 零堆分配，一帧驻留 [`FrameBuf`]。
//!
//! 扩展帧的外层由本解码器保证，Marker/SubData 解包交由指令层（层间解耦）。

use super::{crc, FrameBuf, FRAME_HEADER, MAX_FRAME_LEN, RawFrame};
use core::mem;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// 丢弃噪声，等待 0xFF
    SeekHeader,
    /// 收 Len（收完即校验合法性）
    Len,
    /// 收 Opcode
    Opcode,
    /// 收 Status 高 / 低字节
    StatusHi,
    StatusLo,
    /// 收 Data（按 Len 计数）
    Data,
    /// 收 CRC 高 / 低字节
    CrcHi,
    CrcLo,
}

/// 单字节喂入的结果。
///
/// `Frame` 借用解码器内部缓冲：持有 `Outcome` 期间不得再调用 `feed`。
#[derive(Debug)]
pub enum Outcome<'a> {
    /// 字节已吸收，还凑不齐一帧。
    NeedMore,
    /// 产出一条 CRC 已校验通过的完整帧。
    Frame(RawFrame<'a>),
    /// 发生重同步（帧长非法 / CRC 失败后已在缓冲内重扫帧头）。
    Resynced,
}

/// 流式解码器。
#[derive(Debug)]
pub struct StreamDecoder {
    state: State,
    buf: FrameBuf,
    /// 当前状态还需收集的字节数（Data / CRC 阶段使用）。
    need: u8,
    /// 是否刚产出过帧（帧数据仍被借用占用，下次 feed 前需清缓冲）。
    frame_pending: bool,
}

impl Default for StreamDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl StreamDecoder {
    pub const fn new() -> Self {
        StreamDecoder {
            state: State::SeekHeader,
            buf: FrameBuf::new(),
            need: 0,
            frame_pending: false,
        }
    }

    /// 喂入单个字节。
    pub fn feed(&mut self, byte: u8) -> Outcome<'_> {
        if self.frame_pending {
            // 上一帧已被取走，缓冲可回收
            self.buf.clear();
            self.frame_pending = false;
        }
        match self.state {
            State::SeekHeader => {
                if byte == FRAME_HEADER {
                    let _ = self.buf.push(byte);
                    self.state = State::Len;
                }
                Outcome::NeedMore
            }
            State::Len => {
                let _ = self.buf.push(byte);
                // 响应帧总长 = 1+1+1+2+Len+2 = 7+Len ≤ 255
                if 7 + byte as usize > MAX_FRAME_LEN {
                    return self.resync();
                }
                self.state = State::Opcode;
                Outcome::NeedMore
            }
            State::Opcode => {
                let _ = self.buf.push(byte);
                self.state = State::StatusHi;
                Outcome::NeedMore
            }
            State::StatusHi => {
                let _ = self.buf.push(byte);
                self.state = State::StatusLo;
                Outcome::NeedMore
            }
            State::StatusLo => {
                let _ = self.buf.push(byte);
                self.need = self.buf[1]; // Data 字节数 = Len
                self.state = if self.need == 0 {
                    State::CrcHi
                } else {
                    State::Data
                };
                Outcome::NeedMore
            }
            State::Data => {
                let _ = self.buf.push(byte);
                self.need -= 1;
                if self.need == 0 {
                    self.state = State::CrcHi;
                }
                Outcome::NeedMore
            }
            State::CrcHi => {
                let _ = self.buf.push(byte);
                self.need = 2;
                self.state = State::CrcLo;
                Outcome::NeedMore
            }
            State::CrcLo => {
                let _ = self.buf.push(byte);
                let n = self.buf.len();
                let got = u16::from_be_bytes([self.buf[n - 2], self.buf[n - 1]]);
                let expect = crc::crc16_ex10(&self.buf[1..n - 2]);
                if got == expect {
                    let len = self.buf[1];
                    self.state = State::SeekHeader;
                    self.frame_pending = true; // 缓冲留给 Frame 借用
                    return Outcome::Frame(RawFrame {
                        length: len,
                        opcode: self.buf[2],
                        status: u16::from_be_bytes([self.buf[3], self.buf[4]]),
                        data: &self.buf[5..5 + len as usize],
                    });
                }
                self.resync()
            }
        }
    }

    /// 批量喂入，对产出的每条帧调用 `on_frame`；返回 `(帧数, 重同步次数)`。
    pub fn feed_slice_with<F>(&mut self, bytes: &[u8], mut on_frame: F) -> (usize, usize)
    where
        F: FnMut(RawFrame<'_>),
    {
        let mut frames = 0;
        let mut resyncs = 0;
        for &b in bytes {
            match self.feed(b) {
                Outcome::NeedMore => {}
                Outcome::Frame(f) => {
                    frames += 1;
                    on_frame(f);
                }
                Outcome::Resynced => resyncs += 1,
            }
        }
        (frames, resyncs)
    }

    /// 批量喂入，仅统计（便于日志 / 测试）。
    pub fn feed_slice(&mut self, bytes: &[u8]) -> (usize, usize) {
        self.feed_slice_with(bytes, |_| {})
    }

    /// 复位到初始状态（串口断开重连等场景）。
    pub fn reset(&mut self) {
        self.state = State::SeekHeader;
        self.buf.clear();
        self.need = 0;
        self.frame_pending = false;
    }

    /// 在已收缓冲内从第 1 字节起重扫 0xFF 重同步。
    ///
    /// - 找到 → 把该字节挪到缓冲头、回到 `Len` 状态等下一字节；
    /// - 没找到 → 整体丢弃回 `SeekHeader`。
    fn resync(&mut self) -> Outcome<'_> {
        let found = self.buf.iter().skip(1).position(|&b| b == FRAME_HEADER);
        match found {
            Some(off) => {
                let at = off + 1;
                // 移除帧头前的垃圾字节
                let keep: FrameBuf = self.buf[at..].iter().copied().collect();
                let _ = mem::replace(&mut self.buf, keep);
                self.state = State::Len;
            }
            None => {
                self.buf.clear();
                self.state = State::SeekHeader;
            }
        }
        self.need = 0;
        Outcome::Resynced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 手册 p.27 响应帧：FF 00 09 00 00 15 E9
    const BOOT_RESP: [u8; 7] = [0xFF, 0x00, 0x09, 0x00, 0x00, 0x15, 0xE9];

    fn feed_collect<'d>(
        dec: &'d mut StreamDecoder,
        bytes: &[u8],
    ) -> heapless::Vec<u16, 16> {
        let mut out: heapless::Vec<u16, 16> = heapless::Vec::new();
        for &b in bytes {
            if let Outcome::Frame(f) = dec.feed(b) {
                let _ = out.push(u16::from_be_bytes([f.opcode, (f.status >> 8) as u8]));
            }
        }
        out
    }

    #[test]
    fn single_frame() {
        let mut dec = StreamDecoder::new();
        assert_eq!(feed_collect(&mut dec, &BOOT_RESP), [0x0900]);
        assert_eq!(feed_collect(&mut dec, &BOOT_RESP), [0x0900]);
    }

    #[test]
    fn two_frames_glued() {
        let mut dec = StreamDecoder::new();
        let mut glued = heapless::Vec::<u8, 32>::new();
        let _ = glued.extend_from_slice(&BOOT_RESP);
        let _ = glued.extend_from_slice(&BOOT_RESP);
        assert_eq!(feed_collect(&mut dec, &glued), [0x0900, 0x0900]);
    }

    #[test]
    fn fragmented_feed() {
        // 半帧分 3 次喂入（模拟串口分片）
        let mut dec = StreamDecoder::new();
        assert!(feed_collect(&mut dec, &BOOT_RESP[..3]).is_empty());
        assert!(feed_collect(&mut dec, &BOOT_RESP[3..5]).is_empty());
        assert_eq!(feed_collect(&mut dec, &BOOT_RESP[5..]), [0x0900]);
    }

    #[test]
    fn skips_noise_before_frame() {
        let mut dec = StreamDecoder::new();
        let noise = [0x00, 0x12, 0xAB];
        assert!(feed_collect(&mut dec, &noise).is_empty());
        assert_eq!(feed_collect(&mut dec, &BOOT_RESP), [0x0900]);
    }

    #[test]
    fn bad_crc_resync_into_next_frame() {
        // 构造：坏帧(1字节CRC被改) 尾部紧跟好帧的 0xFF —— 重扫帧头后应产出好帧
        let mut broken = BOOT_RESP;
        broken[5] ^= 0x01;
        let mut dec = StreamDecoder::new();
        let mut input = heapless::Vec::<u8, 32>::new();
        let _ = input.extend_from_slice(&broken);
        let _ = input.extend_from_slice(&BOOT_RESP);
        let mut seen = 0;
        for &b in &input {
            match dec.feed(b) {
                Outcome::Resynced => seen |= 1,
                Outcome::Frame(f) => {
                    assert_eq!(f.opcode, 0x09);
                    seen |= 2;
                }
                Outcome::NeedMore => {}
            }
        }
        assert_eq!(seen, 3);
    }

    #[test]
    fn illegal_length_treated_as_noise() {
        // Len=250 使总长 >255：该 0xFF 应被丢弃并继续扫描，紧随的好帧可正常解出
        let mut dec = StreamDecoder::new();
        let mut input = heapless::Vec::<u8, 32>::new();
        let _ = input.push(0xFF);
        let _ = input.push(250);
        let _ = input.extend_from_slice(&BOOT_RESP);
        assert_eq!(feed_collect(&mut dec, &input), [0x0900]);
    }

    #[test]
    fn fake_len_noise_eventually_resyncs() {
        // 噪声伪造合法 Len（0xAB）：状态机被吃进 Data 阶段，收满 178 字节后
        // CRC 必失败 → 重同步 → 好帧正常产出（CRC 失败重扫是最终兑底）。
        let mut dec = StreamDecoder::new();
        let mut input = heapless::Vec::<u8, 256>::new();
        let _ = input.extend_from_slice(&[0xFF, 0xAB, 0xCD]);
        // 填充垃圾直到假帧收满（确保不含 0xFF）：(i*7+1) % 256 无 255 解
        let mut i = 0u32;
        while input.len() < 178 {
            let _ = input.push((i * 7 + 1) as u8);
            i += 1;
        }
        let _ = input.extend_from_slice(&BOOT_RESP);
        let mut frames = 0;
        let mut resyncs = 0;
        for &b in &input {
            match dec.feed(b) {
                Outcome::Frame(f) => {
                    assert_eq!(f.opcode, 0x09);
                    frames += 1;
                }
                Outcome::Resynced => resyncs += 1,
                Outcome::NeedMore => {}
            }
        }
        assert_eq!((frames, resyncs), (1, 2));
    }

    #[test]
    fn reset_clears_partial_state() {
        let mut dec = StreamDecoder::new();
        let _ = dec.feed(0xFF);
        let _ = dec.feed(0x05);
        dec.reset();
        assert_eq!(feed_collect(&mut dec, &BOOT_RESP), [0x0900]);
    }

    #[test]
    fn extended_response_decodes() {
        // p.65 接收帧（19 字节）
        let frame: [u8; 19] = [
            0xFF, 0x0C, 0xAA, 0x00, 0x00, 0x4D, 0x6F, 0x64, 0x75, 0x6C, 0x65, 0x74, 0x65, 0x63,
            0x68, 0xAA, 0x49, 0x0F, 0x22,
        ];
        let mut dec = StreamDecoder::new();
        let mut got_sub = 0;
        for &b in &frame {
            if let Outcome::Frame(f) = dec.feed(b) {
                assert_eq!(f.subcode(), Some(0xAA49));
                got_sub += 1;
            }
        }
        assert_eq!(got_sub, 1);
    }

    #[test]
    fn tag_notification_decodes() {
        // p.61 标签信息数据包实例1（真机同构）：FF 1B AA 00 00 ... 共 34 字节
        // 构造一条最小合法通知：Len=2, Op=AA, Status=0000, Data=003F
        let mut f = heapless::Vec::<u8, 32>::new();
        let _ = f.extend_from_slice(&[0xFF, 0x02, 0xAA, 0x00, 0x00, 0x00, 0x3F]);
        let c = crc::crc16_ex10(&f[1..]);
        let _ = f.push((c >> 8) as u8);
        let _ = f.push(c as u8);
        let mut dec = StreamDecoder::new();
        let mut frames = 0;
        for &b in &f {
            if let Outcome::Frame(frame) = dec.feed(b) {
                assert_eq!(frame.opcode, 0xAA);
                assert_eq!(frame.status, 0x0000);
                assert_eq!(frame.data, &[0x00, 0x3F]);
                frames += 1;
            }
        }
        assert_eq!(frames, 1);
    }

    #[test]
    fn feed_slice_counts() {
        let mut dec = StreamDecoder::new();
        let mut input = heapless::Vec::<u8, 32>::new();
        let _ = input.extend_from_slice(&BOOT_RESP);
        let _ = input.extend_from_slice(&BOOT_RESP);
        let (frames, resyncs) = dec.feed_slice(&input);
        assert_eq!((frames, resyncs), (2, 0));
    }
}
