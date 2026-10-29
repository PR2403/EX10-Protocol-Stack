# AGENTS.md

EX10 系列 UHF RFID 读卡器（E310/E510/E710/E910）的 Rust 协议栈。
上位机经 `/dev/ttyUSB0` 串口与模块通信。

## 当前状态

**M1–M4 已实施完成（2026-10-29）**，`cargo test` 104 项全绿，真机冒烟通过：

- `src/frame/` — 三种 CRC、帧构建器、流式解码器（粘包/断包/噪声/重同步）
- `src/types/` — bank/region(含各区域默认跳频表)/select/metadata/baud
- `src/command/` — bootloader/get/set/inventory/access/persist/async_inventory/misc
- `src/transport/` — Link trait + SerialLink + UhfReader（一发一收、通知分发）
- `src/main.rs` — `uhf-demo` CLI（info/inventory/async-inventory/read-tid/write-user/echo-detect）
- 真机验证（2026-10-29）：info 全字段、盘存+缓冲元数据、读 TID、异步盘存 365 条通知均正常

设计决策见 `plan/`（注意 §3 扩展帧格式已在实施中修正，以 `plan/01-protocol-summary.md` 为准）。
`plan/probe_*.py` 是历史验证脚本（保留作复现依据，勿删）；其中
`probe_crc_full.py` 的 FRAMES 表对扩展帧有截尾错误（17 项 MISS），勿作向量来源。

## 构建与测试

```bash
cargo build                              # std 模式（默认）
cargo build --no-default-features        # no_std 门槛，必须通过
cargo test                               # 黄金向量回归
cargo clippy --all-features -- -D warnings
```

## ⚠️ 致命陷阱：帧 CRC 是私有非标准算法

**2026-10-29 真机实测确认，这是本项目最重要的结论。**

帧尾 CRC-16 **不是**任何标准 CRC（CCITT-FALSE/MODBUS/XMODEM 均不匹配，
65536 个 poly 暴力搜索无解）。手册附录 1 的 C 代码中，数据位插入寄存器
**bit0**（标准 CRC 是经 `reg ^= byte<<8` 从 bit8 进入）：

```
reg = 0xFFFF
for b in data[1:]:            # 帧头 0xFF 不参与计算
    for bit in b 的每一位 (MSB first):
        xorFlag = reg & 0x8000
        reg = (reg << 1) & 0xFFFF
        if bit: reg |= 1      # ★ 数据位进 bit0，非标准
        if xorFlag: reg ^= 0x1021
# 结果大端（高字节在前）附加帧尾
```

- **禁止引入 `crc` 等 CRC 库计算帧 CRC**——参数不存在，实测真机对标准
  CRC 帧无响应。
- Rust 实现约 15 行逐位循环即可，勿过度设计。
- TagCRC（附录 5，PC+EPC 校验）是**标准** ISO6C 算法（取反输出）；
  SubCRC（扩展指令）是**简单累加和** `sum & 0xFF`（范围从 SubCode 起）。
  三者均已用真机 + 手册向量锁定，见 `plan/04-testing.md` §1。

验证向量（实现时必须写进单元测试）：

| 数据 | CRC |
|---|---|
| `FF 00 03` | `1D 0C` |
| `FF 14 03 00 00 18 06 04 00 A9 00 00 01 20 20 07 03 20 07 03 00 00 00 00 10`（真机实测响应帧） | `B0 31` |
| `{AA 40 04 00}` 的 SubCRC | `EE` |

扩展帧完整向量（CRC 覆盖含 0xBB Terminator；Len=14+N 不含 BB）：

| 帧（不含 CRC） | CRC | 来源 |
|---|---|---|
| `FF 0E AA MT AA 49 F3 BB` | `0391` | p.65 |
| `FF 14 AA MT AA 40 04 01 A5 A5 5A 5A ED BB` | `1239` | p.143 |
| `FF 10 AA MT AA 40 06 00 F0 BB` | `A051` | p.144 |
| `FF 13 AA MT AA 48 00 BF 00 80 03 34 BB` | `290F` | p.59 |
| `FF 11 AA MT AA 51 20 20 87 C2 BB` | `DC50` | p.163 |

（MT = `4D 6F 64 75 6C 65 74 65 63 68` "Moduletech"；更多见 `src/frame/builder.rs` 单元测试）

## 手册勘误（doc/EX10系列模块通信协议-230813.pdf）

- **p.68 EX 异步盘存示例帧有印刷错误**：DataLength 应为 `0x27`（原文印
  `0x13`），MetadataFlags 应为 `0x0016`。勿直接抄手册该帧；已重建验证
  （`plan/probe_p68.py`），CRC 命中 `0ABF`。
- 0x06 设置波特率指令已失效，只能用扩展指令 0xAA40 KEY=0x06。
- 0x2A 清缓存指令已无效。
- 手册十六进制实例整体可信（40+ 帧验证通过），但抄录进测试时注意 PDF
  文本提取可能丢字/粘连，需与上下文数据长度字段自洽性交叉核对。

## 硬件实测

- 设备：`/dev/ttyUSB0`，**115200 8N1**，无流控（root/dialout 权限）
- 截至上次测试模块处于 **Bootloader 层**（0xAA40 返回 0x0101）；
  用 0x0C 查询阶段，Boot 层需先发 0x04 进入 APP 层才能执行盘存类指令
- Python 快速验证环境已就绪：`pyserial`、`pypdf`（pip --break-system-packages 安装）
- 一发一收纪律：未收到应答前不得发下一条指令；应答超时 = 5s + 指令执行
  时间（回波检测 0xAA4A 为 30s）；0x09 成功后额外等 250ms

## 架构约束（实现时不可违背）

1. **协议核心必须 no_std**：L1–L3（类型/帧/指令层）零堆分配，帧缓冲用
   `heapless::Vec<u8, 256>`；串口 (`serialport`) 等只在 `std` feature 下编译
2. **流式解码器是一等公民**：逐字节状态机
   `SeekHeader→Len→Op→Status→Data→Crc`，要求免疫粘包/断包/噪声，
   CRC 失败后需在已收缓冲内重扫 `0xFF` 重同步
3. 单帧 ≤ 255 字节（协议硬性上限）
4. 多字节整数大端；响应帧比请求多 2 字节 Status（在 Op 之后）
5. 扩展指令帧：`0xAA` + `"Moduletech"`(10B) + SubCode(2B) + SubData +
   SubCRC + `0xBB`；响应侧无 SubCRC/Terminator
6. 指令 API 风格：每条指令一个强类型 Request 结构体（`encode` 到定长缓冲）
   + Response 解析函数，域名与手册英文域一致，doc 注明页码

## 约定

- 计划/文档/注释用中文；代码标识符、commit 信息用英文
- 每条指令实现必须附手册对应十六进制实例作为测试向量（编码+解码双向断言）
- 修改协议层行为前先更新 `plan/01-protocol-summary.md` 对应条目
- 探测脚本放 `plan/`，命名 `probe_*.py`；真机测试脚本不得提交设备专属
  序列号等标识信息
