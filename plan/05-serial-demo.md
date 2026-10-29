# 05 串口接入与 Demo（/dev/ttyUSB0）

## 1. 串口参数

- 默认 **115200 8N1**，无流控（手册：出厂默认波特率 115200，可被 0xAA40 KEY=0x06 修改）
- 可配置波特率集合（附录 2）：9600/19200/38400/57600/115200/230400/460800/921600
- 依赖：`serialport = "4"`（仅 `std` feature）

## 2. UhfReader 高层 API（std）

```rust
let reader = UhfReader::open("/dev/ttyUSB0", 115_200)?;
let ver = reader.get_version()?;            // 0x03
if reader.get_run_stage()? == Stage::Boot { reader.boot_firmware()?; } // 0x04
reader.set_antenna_ports(&[1])?;            // 0x91
let inv = reader.inventory(1000)?;          // 0x22
let tags = reader.get_tag_buffer(ReadOption::New, flags)?; // 0x29
// 异步：
reader.start_async_inventory(cfg, |n| match n { Notification::Tag(t) => .., Heartbeat(..) => .., .. })?;
```

## 3. 串口纪律（源自手册约束）

- **一发一收**：请求发出后未收到应答前不得发下一条（§3 ⑴）；
- 读串口采用小分片（如 64B）喂解码器，天然适配流式模型；
- 0x09 成功后额外 `sleep 250ms`（p.27）；
- 每条指令超时 = `5s + timeout_hint()`，回波检测 30s（§3 ⑷）。

## 4. Demo CLI（src/main.rs）

```
uhf-demo [--port /dev/ttyUSB0] [--baud 115200] <CMD>
CMD:
  info                # 0x0C + 0x03 + 0x10 + 0x67 + 0x72 汇总
  inventory <ms>      # 0x91 → 0x22 → 0x29
  async-inventory <s> # 0xAA48 → 收通知打印 → 0xAA49
  read-tid            # 0x28 TID 4 字
  write-user <hex>    # 0x24
  echo-detect         # 0xAA4A
```

输出示例（info）：
```
Stage:      AppFirmware
Boot:       2.2.18  Hw: E710 / 4ANT / CHINA / rev0
FwDate:     2022-07-08  Fw: 2.2.7.8
Serial:     2023 / 0000000000010245
Region:     China1(920-925)  Temp: 39°C
```

## 5. 权限说明

- Linux 下访问 `/dev/ttyUSB0` 需 `dialout` 组或 root；README 注明 `sudo usermod -aG dialout $USER`。
- 若模块上电停在 Bootloader 层且未配置自动跳 APP（0xAA40 KEY=0x04），demo 的 `info` 会自动执行 0x04。
