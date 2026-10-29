# EX10 系列 UHF RFID 读卡器 Rust 协议栈 — 实施计划

> 依据: `doc/EX10系列模块通信协议-230813.pdf`（179 页，Ver23-8-13）
> 目标硬件: EX10 系列（E310/E510/E710/E910）模块，经 `/dev/ttyUSB0` 串口连接
> 本目录只做规划，不含实施代码。

## 1. 目标与约束

| 项 | 内容 |
|---|---|
| 语言/版本 | Rust 2024 edition（与现有 `Cargo.toml` 一致） |
| 核心形态 | `no_std` 兼容库 crate（协议核心零堆分配，基于 `heapless` 定长缓冲） |
| std 层 | 通过 `std` feature 提供串口传输层与上位机 demo（`main.rs`） |
| 流式输入 | 协议核心提供**字节流增量解码器**：逐字节/逐块 feed，随时可能粘包、断包、噪声，解码器内部状态机完成同步、组帧、校验、重同步 |
| 外部依赖 | `heapless`（no_std 定长容器）；std 侧 `serialport`、`log`/`env_logger`；CRC 全部自实现（手册附录 1/5/7 直接翻译，无需 `crc` crate） |
| 帧上限 | 单帧 ≤ 255 字节（协议 §3 规定），缓冲区取 256 |
| 测试基准 | 手册全部十六进制指令实例作为"黄金向量"（见 `03-testing.md`） |

## 2. 分层架构

```
┌─────────────────────────────────────────────────────┐
│ L4  std 传输层 (feature="std")                       │
│     UhfReader: 串口读写、一发一收、超时、通知分发      │
├─────────────────────────────────────────────────────┤
│ L3  指令层 (no_std)                                  │
│     每条指令: Request 构建器 + Response 解析器        │
│     bootloader / inventory / access / set / get /    │
│     persistent(0xAA40) / misc(0xAA4A..0xAA51)        │
├─────────────────────────────────────────────────────┤
│ L2  帧层 (no_std)                                    │
│     RawFrame、帧构建器(含扩展指令封装)、流式解码器、   │
│     CRC-16 / SubCRC / TagCRC、状态码枚举             │
├─────────────────────────────────────────────────────┤
│ L1  公共类型 (no_std)                                │
│     Bank、Region、MetadataFlags、Select(Tag          │
│     Singulation)、大端读写工具、错误类型              │
└─────────────────────────────────────────────────────┘
```

- L1–L3 **不依赖任何 OS/串口**，可移植到嵌入式（后续加 `defmt` feature 即可）。
- L4 只在 std 下编译，负责把"字节来源"抽象为 `Tx`/`Rx` trait，串口只是默认实现。

## 3. Crate 结构（规划）

```
UHF/
├── Cargo.toml            # [features] default=["std"]; std=["dep:serialport",...]
├── doc/                  # 手册（已有）
├── plan/                 # 本计划
├── src/
│   ├── main.rs           # std demo 上位机（版本查询→进 APP→设天线→盘存）
│   └── lib.rs            # crate 根，#![cfg_attr(not(feature="std"), no_std)]
│       ├── frame/
│   │   ├── mod.rs        # RawFrame、帧常量（HEADER=0xFF、扩展指令 0xAA）
│   │   ├── builder.rs    # 帧构建器：通用帧 + 扩展帧(Moduletech/SubCRC/0xBB)
│   │   ├── decoder.rs    # ★ 流式状态机解码器（核心交付物）
│   │   └── crc.rs        # crc16_ccitt_false / tag_crc(iso6c) / sub_crc
│   ├── types/
│   │   ├── mod.rs
│   │   ├── bank.rs       # MemBank(Reserved/Epc/Tid/User)
│   │   ├── region.rs     # RegionCode 枚举（附录 3）+ 各区域默认跳频表
│   │   ├── select.rs     # Option 位域 + TagSingulation 构建器
│   │   ├── metadata.rs   # MetadataFlags 位标志 + Metadata 流式读取游标
│   │   └── baud.rs       # 附录 2 波特率表
│   ├── status.rs         # 状态码枚举 + Display（中文含义，手册"状态码"章）
│   ├── error.rs          # Error / DecodeError 统一错误
│   ├── command/
│   │   ├── mod.rs        # Command trait 约定
│   │   ├── bootloader.rs # 0x01/0x02/0x03/0x04/0x08/0x09/0x0C/0x10
│   │   ├── inventory.rs  # 0x21/0x22/0x29 + 异步 0xAA48/49/58/59 + 通知包
│   │   ├── access.rs     # 0x23/0x24/0x25/0x26/0x28/0x2D
│   │   ├── set.rs        # 0x91/0x93/0x95/0x96/0x97/0x9A/0x9B
│   │   ├── get.rs        # 0x61/0x62/0x63/0x65/0x66/0x67/0x6A/0x6B/0x70/0x71/0x72
│   │   ├── persist.rs    # 0xAA40 KEY/GETORSET 全套（含 0xAB、恢复出厂）
│   │   └── misc.rs       # 0xAA4A 回波 / 0xAA4C 多标签过滤 / 0xAA50 温度 / 0xAA51 亮灯
│   └── transport/
│       ├── mod.rs        # Io trait（send/recv，无阻塞读字节）
│       └── serial.rs     # std: serialport 实现 + UhfReader 高层 API（std feature）
└── tests/
    ├── golden_frames.rs  # 手册黄金向量回归
    └── stream_decode.rs  # 粘包/断包/噪声/CRC 错重同步测试
```

## 4. 关键设计决策

1. **流式解码器是第一公民**（详见 `02-architecture.md` §2）
   - `StreamDecoder::feed(byte) -> DecodeOutcome`，`feed_slice(&[u8])` 循环内驱动画出多帧；
   - 状态机: `SeekHeader → Len → Opcode → StatusHi → StatusLo → Data(n) → Crc(2)`；
   - 帧长非法（`6+len > 255`）与 CRC 失败：丢弃当前帧并在**已收缓冲内重扫 0xFF**，保证丢包后能重新同步；
   - 解码器不分配、不拷贝多余数据；一帧最大 256 字节驻留 `heapless::Vec<u8, 256>`。
2. **构建与解析对称**：每条指令一个 `request.rs` 内的强类型参数结构体 + `Response` 结构体，避免手工拼字节出错；全部按手册"发送指令 Data 字段格式"逐域实现，注释标明手册页码。
3. **扩展指令自动封装**：`builder.extended(subcmd: u16, subdata)` 自动加 `Moduletech` 标记、SubCRC（子码起累加和低 8 位）、`0xBB` 终止符；响应侧自动剥壳校验。
4. **一发一收纪律**（协议 §3 ⑴）：`UhfReader` 串行化所有请求；等待应答期间收到的 0xAA 帧按"通知"分发（标签/心跳 XTSJ/轮询周期），应答匹配规则 = opcode 相等（扩展指令再比子码）。
5. **超时模型**（协议 §3 ⑷）：默认 `5s + 指令执行时间`；`0x22` 用 Timeout 域、`0xAA4A` 固定 30s、`0xAA51` 加点亮时长——由每条指令的 `Command::timeout_hint()` 提供。
6. **错误即状态码**：非 0 状态码映射为 `Error::Status(Status)`，附中文描述；特殊值 `0xAA49/0xAA59` 表示"异步盘存被打断"，传输层据此刷新盘存状态。

## 5. 里程碑

| 阶段 | 内容 | 验收 |
|---|---|---|
| M1 | 帧层: CRC×3、帧构建器（通用+扩展）、流式解码器、状态码 | 黄金向量全过；模糊/粘包测试过 |
| M2 | 指令层全部指令（先常用后冷门，顺序见 02 §5） | 每条指令至少 1 组手册向量 |
| M3 | std 传输层: serialport + UhfReader + 通知分发 | 真机 `/dev/ttyUSB0` 冒烟 |
| M4 | `main.rs` demo CLI: 查版本→0x04 进 APP→0x91 设天线→0x22/0x29 盘存→0x28 读 TID | 真机完整流程 |

## 6. 明确的非目标（本期不做）

- 固件升级 Bootloader 流程编排（仅提供 0x01/0x02/0x08 单帧能力，不做分块状态机）；
- 宜链温度标签的温度换算策略封装之外的业务逻辑；
- async/await 传输层（预留 trait 边界，本期阻塞式）。
