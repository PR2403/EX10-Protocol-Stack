//! 统一错误类型。

use crate::status::Status;

/// 帧编码错误（构建请求帧时）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// 超出单帧 255 字节上限（协议 §3 p.14）。
    FrameTooLong,
    /// 参数非法（如频点数 > 50、Q > 15、字数超限等）。
    InvalidParameter(&'static str),
}

/// 帧解码 / 数据域解析错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// 剩余数据不足，域被截断。
    Truncated,
    /// CRC 校验失败。
    BadCrc,
    /// 帧结构错误（DataLength 与实际字节不符、DataLength 超上限等）。
    BadFrame,
    /// 扩展帧壳不合法（缺 "Moduletech" Marker / SubCode 不全）。
    BadExtFrame,
    /// 域值不在合法范围。
    BadValue(&'static str),
}

/// 协议栈统一错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// 设备返回非 0 状态码（手册"状态码"章 p.164-167）。
    Status(Status),
    /// 应答超时（默认 5s + 指令执行时间，协议 §3 ⑷ p.14）。
    Timeout,
    /// 编码失败。
    Encode(EncodeError),
    /// 解码失败。
    Decode(DecodeError),
}

impl From<EncodeError> for Error {
    fn from(e: EncodeError) -> Self {
        Error::Encode(e)
    }
}

impl From<DecodeError> for Error {
    fn from(e: DecodeError) -> Self {
        Error::Decode(e)
    }
}

impl From<Status> for Error {
    fn from(s: Status) -> Self {
        Error::Status(s)
    }
}

impl core::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EncodeError::FrameTooLong => write!(f, "frame exceeds 255-byte limit"),
            EncodeError::InvalidParameter(what) => write!(f, "invalid parameter: {what}"),
        }
    }
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::Truncated => write!(f, "data truncated"),
            DecodeError::BadCrc => write!(f, "frame CRC mismatch"),
            DecodeError::BadFrame => write!(f, "bad frame structure"),
            DecodeError::BadExtFrame => write!(f, "bad extended-frame shell"),
            DecodeError::BadValue(what) => write!(f, "bad field value: {what}"),
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Status(s) => write!(f, "device status {s}"),
            Error::Timeout => write!(f, "response timeout"),
            Error::Encode(e) => write!(f, "{e}"),
            Error::Decode(e) => write!(f, "{e}"),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

#[cfg(feature = "std")]
impl std::error::Error for EncodeError {}

#[cfg(feature = "std")]
impl std::error::Error for DecodeError {}
