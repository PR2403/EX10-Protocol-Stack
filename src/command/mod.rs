//! L3 指令层：每条指令一个强类型 Request + Response 解析函数。
//!
//! 约定（plan/02-architecture.md §3）：
//! - 每条指令一个模块：`XxxRequest::encode()` 组 Data 域 + `parse_*()` 解析响应；
//! - 字段名与手册英文域一致，doc 注明手册页码；
//! - 复合域（Option / MetadataFlags / TagSingulation）复用 [`crate::types`]。

pub mod access;
pub mod async_inventory;
pub mod bootloader;
pub mod get;
pub mod persist;
pub mod inventory;
pub mod misc;
pub mod set;
