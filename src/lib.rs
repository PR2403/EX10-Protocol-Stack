//! EX10 系列 UHF RFID 读卡器协议栈（E310/E510/E710/E910）。
//!
//! 依据《EX10系列模块通信协议-230813.pdf》。分层：
//! - L1 公共类型（[`types`]）
//! - L2 帧层（[`frame`]）：CRC×3、帧构建器、流式解码器
//! - L3 指令层（[`command`]）：每条指令一个强类型 Request + Response 解析
//! - L4 传输层（[`transport`]，`std` feature）：串口读写、一发一收、通知分发
//!
//! L1–L3 为 `no_std` 且零堆分配，帧缓冲用 [`heapless::Vec`]（≤255 字节硬上限）。
//!
//! # 致命陷阱
//! 帧尾 CRC-16 是手册私有**非标准**算法（数据位插入寄存器 bit0，见 [`frame::crc::crc16_ex10`]），
//! 禁止引入任何标准 CRC 库计算帧 CRC（真机对标准 CRC 帧无响应，2026-10-29 实测）。

#![cfg_attr(not(feature = "std"), no_std)]

pub mod command;
pub mod error;
pub mod frame;
pub mod status;
#[cfg(feature = "std")]
pub mod transport;
pub mod types;
