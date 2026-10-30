# UHF-EX10 — EX10 系列 UHF RFID 读卡器 Rust 协议栈

**English version below.**

---

## 简介

EX10 系列 UHF RFID 读卡器模块(E310/E510/E710/E910)的 Rust 协议栈。上位机经串口(默认 `/dev/ttyUSB0`,115200 8N1)与模块通信,实现手册(《EX10系列模块通信协议-230813.pdf》,179 页)定义的全部指令。

核心特性:

- **协议核心 `no_std`、零堆分配** — L1–L3 层可直接跑在 MCU 上,帧缓冲用 `heapless::Vec<u8, 256>`
- **流式解码器** — 逐字节状态机,免疫粘包/断包/线路噪声,CRC 失败自动重同步
- **全指令覆盖** — 盘存、标签访问、配置读写、异步盘存、Bootloader 固件升级、扩展指令(0xAA40 持久化/0xAA4A 回波/0xAA4C 多标签/0xAA50 宜链温度/0xAA51 亮灯)
- **黄金向量回归** — 108 项测试全绿(105 协议 + 3 CLI),全部手册十六进制实例双向断言
- **真机验证** — 2026-10-29 于 EX10 模块(Fw 2.0.7.3)实测通过

## 体积与内存占用(`-Os` + LTO 实测)

| 档位 | ARMv7-M | RV32IMAC |
|---|---|---|
| 仅帧层(CRC×3 + builder + 解码器) | ~1.98 KB | ~2.29 KB |
| L1–L3 全量指令 | ~11.4 KB | ~14.3 KB |
| 全量 + `core::fmt` | ~16.1 KB | ~21.4 KB |

RAM:静态占用(`.data`/`.bss`)**0 字节**;驻留仅 `StreamDecoder` 272 B;最深调用链(重同步)544 B 栈。**总预算 < 1 KB RAM**。详见 `size-bench/`。

## 分层架构

```text
┌─────────────────────────────────────────────────────┐
│ L4  std 传输层 (feature="std")                       │
│     UhfReader: 串口读写、一发一收、超时、通知分发      │
├─────────────────────────────────────────────────────┤
│ L3  指令层 (no_std)                                  │
│     每条指令: Request 构建器 + Response 解析器        │
├─────────────────────────────────────────────────────┤
│ L2  帧层 (no_std)                                    │
│     RawFrame、帧构建器、流式解码器、CRC-16/SubCRC/     │
│     TagCRC、状态码                                   │
├─────────────────────────────────────────────────────┤
│ L1  公共类型 (no_std)                                │
│     Bank、Region(含跳频表)、MetadataFlags、Select、  │
│     Baud、错误类型                                   │
└─────────────────────────────────────────────────────┘
```

## 快速开始

```bash
# 构建 CLI(需 std feature,默认开启)
cargo build --release
# ./target/release/uhf-demo info -p /dev/ttyUSB0

# 跑测试(手册黄金向量回归)
cargo test

# no_std 门槛检查
cargo build --no-default-features

cargo clippy --all-features -- -D warnings
```

`uhf-demo` 子命令(与手册章节对应):`info` / `system` / `config` / `inventory` / `tag` / `multi-tag` / `echo` / `yilian` / `persist` / `firmware` / `raw`,详见 `uhf-demo --help`。

在 `no_std` 项目中使用:

```toml
[dependencies]
uhf-ex10 = { path = "...", default-features = false }
```

```rust
use uhf_ex10::frame::decoder::StreamDecoder;

let mut dec = StreamDecoder::new();          // 272 B 驻留
for byte in uart_bytes {                     // 逐字节喂入
    if let uhf_ex10::frame::decoder::Outcome::Frame(f) = dec.feed(byte) {
        // f: RawFrame — CRC 已校验,含 opcode/status/data
    }
}
```

## ⚠️ 致命陷阱:帧 CRC 是私有非标准算法

帧尾 CRC-16 **不是**任何标准 CRC(CCITT-FALSE/MODBUS/XMODEM 均不匹配,poly 65536 全搜索无解)。手册附录 1 的 C 代码将数据位插入寄存器 **bit0**(标准 CRC 经 bit8 进入):

```text
reg = 0xFFFF
for b in data[1:]:            # 帧头 0xFF 不参与
    for bit in b (MSB first):
        xorFlag = reg & 0x8000
        reg = (reg << 1) & 0xFFFF
        if bit: reg |= 1      # ★ 数据位进 bit0,非标准
        if xorFlag: reg ^= 0x1021
# 结果大端附加帧尾
```

**禁止引入 `crc` 等库计算帧 CRC** — 真机对标准 CRC 帧无响应(实测)。TagCRC(ISO6C,取反输出)与 SubCRC(简单累加和)是标准的,三者均已用真机+手册向量锁定。

## 手册勘误摘要

- p.68 EX 异步盘存示例帧印刷错误:DataLength 应为 `0x27`(原文 `0x13`)、MetadataFlags 应为 `0x0016`
- 0x06 设置波特率指令已失效,只能用扩展指令 0xAA40 KEY=0x06;0x2A 清缓存指令已无效
- p.41 TagEpcAndMeta 尾域:真机 Fw 2.0.7.3 的 0x29(flags=0x003F)尾值与 TagCRC 一致,而非手册所称 PHASE2

完整勘误与设计决策见 `plan/`。

## 目录结构

```text
src/
├── types/      # L1: bank/region(跳频表)/metadata/select/baud
├── frame/      # L2: crc(×3)/builder/decoder(流式状态机)
├── command/    # L3: bootloader/get/set/inventory/access/persist/async_inventory/misc
├── transport/  # L4(std): Link trait + SerialLink + UhfReader
├── cli/        # uhf-demo 子命令实现
└── main.rs     # clap CLI 入口
plan/           # 设计文档(01-protocol-summary 为协议权威参考)与 probe_*.py 验证脚本
doc/            # 协议手册 PDF
size-bench/     # 体积/内存基准(-Os+LTO,armv7m 与 rv32imac)
```

## 硬件实测注意

- 一发一收纪律:未收到应答前不得发下一条;应答超时 = 5s + 指令执行时间(回波检测 0xAA4A 为 30s)
- 模块可能处于 Bootloader 层(0xAA40 返回 0x0101):需先发 0x04 进入 APP 层才能盘存
- EX 异步盘存在 FCC 区域固件不支持

---

# UHF-EX10 — Rust Protocol Stack for EX10-series UHF RFID Readers

## Overview

A Rust protocol stack for EX10-series UHF RFID reader modules (E310/E510/E710/E910). The host talks to the module over serial (default `/dev/ttyUSB0`, 115200 8N1), implementing every command defined in the protocol manual ("EX10系列模块通信协议-230813.pdf", 179 pages).

Highlights:

- **`no_std`, zero-heap protocol core** — L1–L3 run directly on MCUs; frame buffers use `heapless::Vec<u8, 256>`
- **Streaming decoder** — byte-by-byte state machine, immune to concatenation / fragmentation / line noise, auto-resynchronization after CRC failure
- **Full command coverage** — inventory, tag access, configuration, async inventory, bootloader firmware upgrade, extended commands (0xAA40 persist / 0xAA4A echo / 0xAA4C multi-tag / 0xAA50 Yilian temp / 0xAA51 LED)
- **Golden-vector regression** — 108 tests green (105 protocol + 3 CLI), every manual hex example asserted in both encode & decode directions
- **Verified on hardware** — smoke-tested on a real EX10 module (Fw 2.0.7.3) on 2026-10-29

## Size & Memory Footprint (measured, `-Os` + LTO)

| Configuration | ARMv7-M | RV32IMAC |
|---|---|---|
| Frame layer only (CRC×3 + builder + decoder) | ~1.98 KB | ~2.29 KB |
| Full L1–L3 command set | ~11.4 KB | ~14.3 KB |
| Full set + `core::fmt` | ~16.1 KB | ~21.4 KB |

RAM: static usage (`.data`/`.bss`) is **zero bytes**; the only resident object is `StreamDecoder` at 272 B; deepest call chain (resync) uses 544 B of stack. **A total budget of < 1 KB RAM suffices.** See `size-bench/`.

## Layered Architecture

```text
┌─────────────────────────────────────────────────────┐
│ L4  std transport (feature="std")                   │
│     UhfReader: serial I/O, request/response,        │
│     timeouts, notification dispatch                 │
├─────────────────────────────────────────────────────┤
│ L3  Command layer (no_std)                          │
│     One typed Request builder + Response parser     │
│     per command                                     │
├─────────────────────────────────────────────────────┤
│ L2  Frame layer (no_std)                            │
│     RawFrame, frame builder, streaming decoder,     │
│     CRC-16 / SubCRC / TagCRC, status codes          │
├─────────────────────────────────────────────────────┤
│ L1  Common types (no_std)                           │
│     Bank, Region (with hopping tables),             │
│     MetadataFlags, Select, Baud, error types        │
└─────────────────────────────────────────────────────┘
```

## Quick Start

```bash
# Build the CLI (requires the std feature, on by default)
cargo build --release
# ./target/release/uhf-demo info -p /dev/ttyUSB0

# Run tests (manual golden-vector regression)
cargo test

# no_std gate check
cargo build --no-default-features

cargo clippy --all-features -- -D warnings
```

`uhf-demo` subcommands (mapped to manual chapters): `info` / `system` / `config` / `inventory` / `tag` / `multi-tag` / `echo` / `yilian` / `persist` / `firmware` / `raw` — see `uhf-demo --help`.

Using in a `no_std` project:

```toml
[dependencies]
uhf-ex10 = { path = "...", default-features = false }
```

```rust
use uhf_ex10::frame::decoder::StreamDecoder;

let mut dec = StreamDecoder::new();          // 272 B resident
for byte in uart_bytes {                     // feed byte by byte
    if let uhf_ex10::frame::decoder::Outcome::Frame(f) = dec.feed(byte) {
        // f: RawFrame — CRC verified, carries opcode/status/data
    }
}
```

## ⚠️ Critical Pitfall: the Frame CRC Is a Proprietary, Non-standard Algorithm

The frame-tail CRC-16 is **not** any standard CRC (CCITT-FALSE / MODBUS / XMODEM all fail; exhaustive search over all 65536 polys has no solution). The C code in manual Appendix 1 inserts data bits into register **bit0** (standard CRCs enter via bit8):

```text
reg = 0xFFFF
for b in data[1:]:            # frame header 0xFF excluded
    for bit in b (MSB first):
        xorFlag = reg & 0x8000
        reg = (reg << 1) & 0xFFFF
        if bit: reg |= 1      # ★ data bit enters bit0 — non-standard
        if xorFlag: reg ^= 0x1021
# append result big-endian as frame tail
```

**Do NOT use `crc` or similar crates for the frame CRC** — the real device does not respond to standard-CRC frames (verified on hardware). TagCRC (ISO6C, inverted output) and SubCRC (simple checksum sum) are standard; all three are locked by on-device + manual vectors.

## Manual Errata (Highlights)

- p.68 EX async inventory example frame has print errors: DataLength should be `0x27` (printed `0x13`), MetadataFlags should be `0x0016`
- The 0x06 set-baud-rate command is obsolete — use extended command 0xAA40 KEY=0x06; the 0x2A buffer-clear command is dead
- p.41 TagEpcAndMeta tail field: on real hardware Fw 2.0.7.3, the 0x29 (flags=0x003F) tail matches TagCRC, not PHASE2 as the manual claims

Full errata and design decisions live in `plan/`.

## Repository Layout

```text
src/
├── types/      # L1: bank/region (hopping tables)/metadata/select/baud
├── frame/      # L2: crc (×3)/builder/decoder (streaming state machine)
├── command/    # L3: bootloader/get/set/inventory/access/persist/async_inventory/misc
├── transport/  # L4 (std): Link trait + SerialLink + UhfReader
├── cli/        # uhf-demo subcommand implementations
└── main.rs     # clap CLI entry
plan/           # design docs (01-protocol-summary is the authoritative protocol reference) & probe_*.py scripts
doc/            # protocol manual PDF
size-bench/     # size/memory benchmarks (-Os+LTO, armv7m & rv32imac)
```

## Hardware Notes

- Strict one-request-one-response discipline: never send the next command before a reply arrives; response timeout = 5 s + command execution time (30 s for echo detection 0xAA4A)
- The module may sit in the Bootloader layer (0xAA40 returns 0x0101): send 0x04 to enter the APP layer before inventory commands
- EX async inventory is unsupported by firmware in the FCC region
