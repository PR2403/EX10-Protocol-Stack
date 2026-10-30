//! core::fmt::Write 接收器(仅在 fmt 基准中引用,把 Display 实现计入体积)。

use core::fmt::Write;

/// 丢弃一切输出的 Writer(把字节写进全局哨兵,防止 fmt 代码被整体优化掉)。
pub struct Sink;

static mut SINK: [u8; 64] = [0; 64];
static mut SINK_POS: usize = 0;

impl Write for Sink {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        unsafe {
            let pos = SINK_POS;
            let n = s.len().min(64 - pos);
            SINK[pos..pos + n].copy_from_slice(&s.as_bytes()[..n]);
            SINK_POS = pos + n;
        }
        Ok(())
    }
}

/// 把一个可 Display 的值写入 Sink。
pub fn drain<T: core::fmt::Display + ?Sized>(v: &T) {
    let mut sink = Sink;
    let _ = write!(sink, "{v}");
}

/// 汇出累计字节数(基准 main 用它防止整个 fmt 路径被 DCE)。
pub fn sink_len() -> usize {
    unsafe { SINK_POS }
}
