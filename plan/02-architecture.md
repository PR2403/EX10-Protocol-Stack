# 02 架构与模块设计

## 1. 线格式（L2 帧层）

### 1.1 通用帧（手册 §3.1，p.14–15）

```
请求: [Header=0xFF][Len][Op][Data..N][CRC16-Hi][CRC16-Lo]      N=Len
响应: [Header=0xFF][Len][Op][StatusHi][StatusLo][Data..N][CRC16-Hi][CRC16-Lo]
总长 = 响应 6+N+2 字节；Len 是 Data 长度；全部大端。
```

### 1.2 扩展帧（手册 §3.2，p.16）

```
请求: FF Len AA | "Moduletech"(10B) | SubCmd(2B) | SubData(N) | SubCRC(1B) | 0xBB | CRC16
响应: FF Len AA | Status(2B) | "Moduletech"(10B) | SubCmd(2B) | SubData(N) | CRC16
```

- `Moduletech` = `4D 6F 64 75 6C 65 74 65 63 68`
- SubCRC = 从 SubCmd 开始到 SubData 末尾所有字节累加和的低 8 位（附录 7）
- 扩展请求 Data = `10 + 2 + N + 1 + 1` 字节

### 1.3 CRC-16（附录 1）—— ⚠️ 非标准算法，禁止使用标准 CRC 库

- **实测确认**（真机 + 手册向量）：数据位插入寄存器**第 0 位**而非第 8 位，不等价于任何
  标准 CRC-16 参数组合；`crc` crate / `crc16` 等库全部不可用，必须逐位自实现（见
  `01-protocol-summary.md` §4 伪代码与验证向量）。
- `crc.rs` 提供：`crc16_ex10()`（协议帧 CRC）、`tag_crc()`（ISO6C，标准算法）、
  `sub_crc()`（累加和）。三者均带 `const fn` 或单元测试锁定黄金向量。
- TagCRC（附录 5）：seed 0xFFFF、poly 0x1021、按位含输入异或、最后取反；输入为 PC+EPC。

### 1.4 流式解码器（本 crate 的核心）

```rust
enum State { SeekHeader, Len, Opcode, StatusHi, StatusLo, Data, CrcHi, CrcLo }
pub struct StreamDecoder { state: State, buf: heapless::Vec<u8,256>, need: u8, op: u8, status: u16, crc: u16 }
pub enum Outcome<'a> { NeedMore, Frame(RawFrame<'a>), Resynced, }
```

规则：
1. `SeekHeader` 状态丢弃一切非 0xFF 字节（含上一帧残留），**天然免疫线路噪声与粘包**；
2. 进入 `Data/Crc` 后按 `need` 计数；帧完整即校验 CRC：
   - 通过 → 产出 `RawFrame`（零拷贝借用 `buf`）并复位到 `SeekHeader`；
   - 失败 → 把 `buf` 从第 1 字节起重扫 0xFF（可能恰好是下一帧头），否则整体丢弃，回 `SeekHeader`（计 `Resynced`）；
3. `Len` 合法性：`6 + Len ≤ 255`，否则视该 0xFF 为噪声继续扫；
4. 扩展帧由上层（`command/` 解析函数）在 Data 内二次解包；解码器只保证外层帧正确，做到**层间解耦**；
5. `feed_slice` 返回产出的帧数，便于 `log::trace!` 环形日志。

> 手册附录 6 的"先收 5 字节再收 Len+2"流程与本状态机等价，但本设计额外覆盖了粘包/坏帧重同步场景。

## 2. 传输层（L4，std feature）

```rust
pub trait Link { fn write(&mut self, buf:&[u8]) -> Result; fn read(&mut self, out:&mut [u8]) -> Result<usize>; /* 非阻塞或带超时 */ }
pub struct UhfReader<L: Link> { link: L, dec: StreamDecoder, txbuf: [u8;256], pub on_notify: Option<fn(Notification)> }
```

`transact(cmd)` 职责：
1. `cmd.encode(&mut txbuf)` → `link.write`；
2. 循环 `read → dec.feed_slice`：
   - 同 opcode 应答 → 校验 status → 解析 Data → 返回；
   - opcode=0xAA 通知（标签/心跳/轮询）→ `on_notify` 回调后继续等；
   - 超时（`cmd.timeout_hint()+5s`）→ `Error::Timeout`；
   - 收到 status=0xAA49/0xAA59 → 异步盘存已中断，原请求应答实为此错误，直接映射。

## 3. 指令层通用约定（L3）

- 每条指令一个模块：`pub struct XxxRequest{..}` + `impl XxxRequest { pub fn encode(&self, out:&mut Vec<u8,256>) }` + `pub fn parse_xxx(frame:&RawFrame) -> Result<XxxResponse>`；
- 参数结构体提供合理默认值（Timeout=1000ms、AccessPassword=0 等），字段名与手册英文域名一致，doc 注明页码；
- 复合域（Option / MetadataFlags / TagSingulation / SearchFlags）在 `types/` 中独立建模，多指令复用。

### 3.1 Option 位域（p.33–37）

| 位 | 名称 | 语义 |
|---|---|---|
| 0-2 | Select-Option Bits | 0=不过滤; 1=EPC值; 2=TID; 3=USER; 4=EPC区; 5=锁定区免过滤(带密码); 7=多标签匹配过滤(需先 0xAA4C) |
| 3 | Invert | 反选 |
| 4 | Non-Sel（0x21/0x28/0xAA51）| 携带 MetadataFlags |
| 5 | 长度扩展 | SelectDataLength 为 2 字节(>255bit) |
| 6/7 | Non-Sel | 因指令而异（0x2D BIT6 必须 1） |

`TagSingulation` 构建器按上表自动决定 `Select Address/Length/Data/AccessPassword` 的有无（p.37 表）。

### 3.2 MetadataFlags → Metadata（p.38–40）

| BIT | 域 | 长度 |
|---|---|---|
| 0 | ReadCount | 1 |
| 1 | RSSI(有符号 dBm 补码) | 1 |
| 2 | AntennaId | 1 |
| 3 | Frequency(kHz) | 3 |
| 4 | Timestamp(ms) | 4 |
| 5 | Phase（注意 0x21/0x28 为 2 字节起+止；0x22/异步为 2 字节止相位，低 12 位有效） | 2 |
| 6 | ProtocolId | 1 |
| 7 | TagDataLength(bit) | 2 (+TagData) |

`MetadataCursor` 流式读取，读越界返回 `Err(Truncated)`——响应解析永不 panic。

## 4. 关键指令数据模型速记（实现时的防错点）

- `0x03/0x04` 响应: BootVer(4)/HwVer(4)/FwDate(4)/FwVer(4)/Proto(4)；HwVer 各字节含义（芯片/天线数/认证区/序号）建模为结构体（p.23）。
- `0x22` 响应 TagsFound: SearchFlags.BIT4=0 → 1 字节，=1 → 4 字节（p.48）；嵌入读结果段按 SearchFlags.BIT2 有无。
- `0x29` 响应每标签: Metadata..EPCLength(2,单位bit)..PC(2)..EPC..TagCRC/Phase2；FASTID 判定算法（p.53-54）单独封装 `try_extract_fastid()`。
- 异步标签包（p.61-62）: Data = MetadataFlags(2) + TagEpcAndMeta；EPCLength 单位是**字节**且含 PC+TagCRC（与 0x29 的 bit 不同！）；心跳: `"XTSJ"`+SearchFlags；轮询包 EPCLength 固定 5、PC=0、EPCCRC=0。
- `0x23/0x24` 写后读: WriteReadFlag（0x8C / 0x84）存在时响应带 ReadData（p.71-77）。
- `0x25` 锁: Mask/Action 各 2 字节低 10 位有效，Perm/W/R 语义建模（p.83-84）。
- `0xAA40` 持久配置: KEY+GETORSET+KEYDATA；KEY=0x91 设置时 KEYDATA=整个 0x91 帧（含 Len 与 Op，不含头与 CRC）（p.133）。
- `0xAA4A` 回波: 响应 data=5+4M，每 4 字节 = 频点(3B)+回波损耗(0.1dB)；提供 VSWR 换算（p.150-151）。
- `0xAA50` 温度: 响应尾随 TagLenth/PC/EPC/TagCRC；三种 PC 后缀对应三种量程换算（p.158-159）。

## 5. 实现顺序（M2 内部排期）

1. bootloader.rs（简单，验证帧层）
2. get.rs + set.rs（结构规整，覆盖 Option/键值）
3. inventory.rs 通用部分（0x21/0x22/0x29，验证 Metadata/Select 模型）
4. access.rs（0x23/0x24/0x25/0x26/0x28/0x2D）
5. persist.rs（扩展指令全流程）
6. inventory.rs 异步部分（0xAA48/49/58/59 + 三种通知包）
7. misc.rs（0xAA4A/4C/50/51）
