//! 串口 Link 实现（`serialport` crate，Linux `/dev/ttyUSB0` 事实标准）。

use super::Link;
use core::time::Duration;
use std::io;

/// 串口链路：默认 115200 8N1 无流控（出厂默认，可被 0xAA40 KEY=0x06 修改）。
pub struct SerialLink {
    port: Box<dyn serialport::SerialPort>,
}

impl SerialLink {
    /// 打开串口。Linux 下需 root 或 dialout 组权限。
    pub fn open(port: &str, baud: u32) -> io::Result<Self> {
        let port = serialport::new(port, baud)
            .data_bits(serialport::DataBits::Eight)
            .parity(serialport::Parity::None)
            .stop_bits(serialport::StopBits::One)
            .flow_control(serialport::FlowControl::None)
            .timeout(Duration::from_millis(100))
            .open()?;
        Ok(SerialLink { port })
    }
}

impl Link for SerialLink {
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.port.write_all(buf)?;
        self.port.flush()
    }

    fn read(&mut self, out: &mut [u8], timeout: Duration) -> io::Result<usize> {
        // serialport 的 timeout 需在读取前设置；下限 1ms 防止忙轮询。
        let t = if timeout.is_zero() {
            Duration::from_millis(1)
        } else {
            timeout
        };
        self.port.set_timeout(t)?;
        match self.port.read(out) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == io::ErrorKind::TimedOut => Ok(0),
            Err(e) => Err(e),
        }
    }
}
