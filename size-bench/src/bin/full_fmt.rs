//! 全量 + core::fmt(Display)基准:额外引用全部 Display 实现。

#![no_std]
#![no_main]

use uhf_ex10::frame::RawFrame;
use uhf_ex10::types::bank::MemBank;
use uhf_ex10::types::metadata::MetadataFlags;
use uhf_ex10::types::region::RegionCode;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    let r = bench::full_bench::run_full();

    // Display 全家桶(编译期不可知 → core::fmt 机器码全部入镜)
    bench::fmt_sink::drain(&RegionCode::China1);
    bench::fmt_sink::drain(&MemBank::User);
    bench::fmt_sink::drain(&MetadataFlags::COMMON);
    bench::fmt_sink::drain(&"str");
    // Debug(派生实现)也测一下:RawFrame + StreamDecoder
    bench::fmt_sink::drain(&format_args!("{:?}", unsafe {
        &*core::ptr::addr_of!(DBG_FRAME)
    }));

    r + bench::fmt_sink::sink_len() as i32
}

static DBG_FRAME: RawFrame<'static> = RawFrame {
    length: 0,
    opcode: 0x10,
    status: 0,
    data: &[],
};
