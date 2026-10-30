//! 全量基准(纯协议逻辑,无 fmt)。

#![no_std]
#![no_main]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> i32 {
    bench::full_bench::run_full()
}
