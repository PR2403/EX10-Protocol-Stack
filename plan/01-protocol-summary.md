# 01 协议规范摘要（实现时随手查）

> 页码均指 `doc/EX10系列模块通信协议-230813.pdf`。

## 1. 通信模型

- 被动模式，请求/应答；**一发一收**，应答超时 = 5s + 指令执行时间（回波检测建议 30s）（§3 p.14）
- 单帧总字节数 ≤ 255；多字节整数**大端**（§3 p.14）
- 运行阶段：Boot(0x11) / APP(0x12)（0x0C p.28）；Boot 指令与 APP 指令不通用，错发返回 0x0101

## 2. 通用帧（§3.1 p.14-15）

```
请求  FF | Len(Data长度) | Op | Data[N] | CRC16
响应  FF | Len           | Op | Status[2] | Data[N] | CRC16
```

## 3. 扩展帧（§3.2 p.16）—— 已用实现+真机双重锁定

```
请求 Data = 0xAA + "Moduletech"(10) + SubCode(2) + SubData(N) + SubCRC(1) + 0xBB(1)
         Len = 14+N（**不含 Terminator 0xBB**），总帧长 = 19+N
         帧 CRC 覆盖 Len..0xBB（**含 Terminator**）
响应 Data = "Moduletech"(10) + SubCode(2) + SubData(N)   （Status 在通用头内，Len=12+N）
         响应侧**无 SubCRC/Terminator**
```
- SubCRC = SubCode..SubData 末尾字节累加和 & 0xFF（附录 7，含 SubCode 本身）
- 实例：`FF 0E AA MT AA 49 F3 BB 0391`（p.65 停止异步）、
  `FF 14 AA MT AA 40 04 01 A5 A5 5A 5A ED BB 1239`（p.143 上电直达 APP）
- ⚠️ `plan/probe_crc_full.py` 的 FRAMES 表把 SubCRC+BB 当成了 CRC（截尾错误），
  重跑有 17 个 MISS；扩展帧 golden 向量以 `src/frame/` 单元测试为准

## 4. CRC（附录 1）—— ⚠️ 已实测确认：**非标准 CRC，不能用任何标准库**

> **实测结论（2026-10-29，真机 /dev/ttyUSB0 + 手册 40+ 帧向量双重验证）**：
> 手册附录 1 的 C 代码是**逐位非标准实现**：数据位被插入到寄存器**第 0 位**（LSB），
> 而标准 CRC 的数据是经 `reg ^= byte<<8` 从第 8 位进入。这一差异导致该 CRC
> **不等价于任何标准参数组合**（CCITT-FALSE/MODBUS/XMODEM 全部不匹配，
> 参数暴力搜索 65536×poly 亦无解）。`crc` crate 等标准库**不可用**，必须自实现。

- 精确算法（逐位，手册附录 1 逐句翻译）：
  ```
  reg = 0xFFFF
  for each byte b (从 Len 开始，帧头 0xFF 不参与):
      for each bit of b (MSB first):
          xorFlag = reg & 0x8000
          reg = (reg << 1) & 0xFFFF
          if bit: reg |= 1            # ★ 关键：数据位进第 0 位
          if xorFlag: reg ^= 0x1021
  CRC 大端附加帧尾
  ```
- 验证向量：`FF 00 03` → `1D0C`；真机版本响应 26 字节帧 → `B031` ✓
- **手册 p68 勘误**：EX 异步盘存示例帧 `FF 13 AA ...` 的 DataLength 应为 `0x27`、
  MetadataFlags 应为 `0x0016`（原文印刷/提取有误，重建帧 CRC 恰好命中 `0ABF`）
- TagCRC（附录 5）：seed 0xFFFF、poly 0x1021、按位含输入异或、最后取反；输入 = PC+EPC。
  已验证：PC=3000+EPC=...2354 → `4AC8` ✓（此算法与 ISO6C 标准 CRC 一致）
- SubCRC（附录 7）：SubCode..SubData 累加和 & 0xFF，已用 6 组向量 + 真机验证 ✓
  - 注意 SubCRC 计算范围含 SubCode 本身，`0xAA40 KEY=04 GET` → `0xEE`（真机实测）

## 5. 核心公共域

- **Option**（§5.1.1 p.33-37）：bit0-2 Select（0无/1 EPC值/2 TID/3 USER/4 EPC区/5 锁区免过滤带密码/7 多标签匹配[需先0xAA4C]），bit3 反选，bit5 SelectLen 2字节，bit4/6/7 Non-Sel 因指令而异
- **Select-Option 与域存在性表**（p.37）：0x00→全无；0x01→Len+Data+Pwd；0x02/03/04→Addr+Len+Data+Pwd；0x05→仅Pwd；0x07→仅Pwd
- **MetadataFlags**（§5.1.3 p.38）：b0 ReadCount(1) b1 RSSI(1,补码dBm) b2 AntId(1) b3 Freq(3,kHz) b4 Timestamp(4,ms) b5 Phase(2；0x21/0x28 起止各1字节 vs 0x22/异步仅止相位低12位) b6 ProtoId(1) b7 TagDataLen(2,bit)+TagData
- **Timeout**：2 字节 ms，最大 65535（§5.1.6 p.41）

## 6. 指令速查（详见 03-commands.md）

| 类别 | Op |
|---|---|
| Bootloader | 01 写Flash 02 读Flash 03 版本 04 启动APP 08 校验 09 回Boot 0C 阶段 10 序列号 |
| 盘存 | 21 单标签 22 同步 29 缓冲区 AA48/49 异步 AA58/59 EX异步 |
| 访问 | 23 写EPC 24 写 25 锁 26 杀 28 读 2D 块写 |
| 设置 | 91 天线 93 协议 95 跳频/驻留 96 GPO 97 区域 9A 配置 9B 协议配置 |
| 获取 | 61 62 63 65 66 67 6A 6B 70 71 72 |
| 持久(0xAA40) | KEY: 91 95 97 9A 9B 04 06 AA恢复出厂 AB 准备升级 |
| 其它 | AA4A 回波 AA4C 多标签过滤 AA50 宜链温度 AA51 宜链亮灯 |

## 7. 易错点备忘

0. **帧 CRC 非标准**（实测确认）：数据位插入寄存器 bit0，任何标准 CRC 库不可用；
   手册 p68 EX 异步示例 DataLength/MetadataFlags 有印刷错误；
1. 0xFF 帧头**不参与** CRC；
2. 响应比请求多 2 字节 Status（在 Op 之后）；
3. 0x29 的 EPCLength 是 **2 字节域、单位 bit**（含 PC+EPCCRC）；**0xAA 通知包
   （标签/轮询）的 EPCLength 是 1 字节域、单位 byte**（p.61/64 实例，已真机验证）；
4. 0x22 TagsFound：SearchFlags.BIT4=1 时为 4 字节且表示 >255；
5. 嵌入读(0x22 Embedded) 的 Timeout/Option 域无效恒 0，总时长由外层 Timeout 决定；
   嵌入读上限手册自相矛盾（p.49 说 32 字节，p.51 例3 读 32 字），实现取 32 字；
6. 扩展帧响应没有 SubCRC/Terminator，但 Status 在 Marker 之前；请求 Len 不含 BB
   且帧 CRC **含** BB（p.59/65/143/144/163 全部实例 CRC 命中验证）；
7. 0x06(设波特率) 已失效 → 只能用 0xAA40 KEY=0x06；
8. 天线逻辑号 = 物理口号（收发一体），上电默认天线 1、最大功率；
9. 锁/杀操作前 AccessPassword 不得为 0；
10. 0x09 成功后需等 250ms；若配置了"上电直达 APP"须先发 0xAB 再发 0x09；
11. 多标签匹配过滤（Select-Option=0x07）前必须先 0xAA4C 成功设置；0x21 不支持 0x07；
12. 中国版模块独有：定频、0xAA58、宜链功能；国外版受限；
13. **TagEpcAndMeta 块尾 2 字节**：flags 未启用相位时=TagCRC；启用相位时（0x29/0xAA）
    该尾域承载**起始相位 PHASE2**（结束相位在 Metadata 内），无额外 PHASE2 域（p.40）；
14. 0xAA48 实例 SearchFlags=0x8003：高字节 BIT7=心跳使能；低字节 BIT0/1 手册未定义
    语义但实例使用 0x03，模型保留 `mode_bits` 原样透传；
15. 手册勘误：p.26 0x08 实例 CRC 印 1D0C（与 0x03 雷同，实算 2A05）；
    p.51 0x22 例3 Len=0x11 与 18 字节数据自相矛盾；p.163 0xAA51 文字要求
    BIT7 模式 Timeout 低字节 0xFF 但实例为 0x2020（实现不强制）。
