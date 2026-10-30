//! 尺寸基准公共库:no_std、无 alloc;主体逻辑在 `full_bench`,
//! 供 full / full_fmt 两个 bin 复用(fmt 变体额外叠加 Display 体积)。

#![no_std]

pub mod full_bench;

#[cfg(feature = "fmt")]
pub mod fmt_sink;
