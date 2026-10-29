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

## 4. Demo CLI（src/main.rs + src/cli/）

基于 `clap` 的完整 CLI，命令分组对应协议手册章节：

```
uhf-demo [--port /dev/ttyUSB0] [--baud 115200] [--extra-timeout 0] <COMMAND>

  info                 # 0x0C + 0x03 + 0x10 + 0x67 + 0x72（Boot 层自动 0x04）
  system               # stage / version / serial / temperature / boot / reset
  config               # region / protocol / antenna / power / frequency / gpi / gpo /
                       # reader / protocol-param（0x61..0x9B）
  inventory            # run / sync / single / buffer / start / stop / ex-start / ex-stop
                       # （0x21/0x22/0x29/0xAA48/49/0xAA58/59）
  tag                  # read / write / write-epc / lock / kill / block-write
                       # （0x23/0x24/0x25/0x26/0x28/0x2D）
  multi-tag            # 0xAA4C 多标签匹配过滤
  echo                 # 0xAA4A 回波检测
  yilian               # 0xAA50 温度 / 0xAA51 亮灯
  persist              # 0xAA40：通用 get/set + 04/06/91/95/97/9A/9B/AA/AB
  firmware             # 0x01/0x02/0x08 + 离线 CHECKCRC（Bootloader 层）
  raw                  # cmd / ext 任意普通/扩展帧（调试）
```

常用示例：

```bash
uhf-demo info
uhf-demo inventory run --timeout-ms 500 --antenna 1 --buffer-flags 0x0015
uhf-demo tag read tid --words 6 --metadata read-count,rssi
uhf-demo tag read tid --epc E2806995... --access-password 11223344
uhf-demo inventory start --duration 10 --antenna 1 --heartbeat --polling
uhf-demo persist baud get
uhf-demo raw cmd 0C
```

全局约定：

- `--port`/`--baud` 支持环境变量 `UHF_PORT`/`UHF_BAUD`；
- 除固件/raw 外，Boot 层会自动发 0x04 进入 APP（`--no-auto-boot` 可关闭）；
- 超时 = 5s + `--extra-timeout` + 指令执行时间；
- 标签过滤：`--epc <EPC>`（EPC 值过滤）、`--select epc|tid|user:addr:bits:hex`、
  `--access-password <hex>`（无过滤时 Select-Option=0x05）、`--select multi`（0xAA4C 之后）。

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
