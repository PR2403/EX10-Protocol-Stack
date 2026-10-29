# 03 指令清单与实现映射

> 覆盖手册第 4–10 章全部指令。✅=必实现，⭕=结构预留（帧能力具备，暂不封装高级流程）

## 4 Bootloader 指令

| Op | 名称 | 请求 Data | 响应 Data | 实现 |
|---|---|---|---|---|
| 0x01 | 写 Flash | FINFLAG(1)+WRITEADDR(4)+WRITELEN(1)+WRITEDATA(N=WRITELEN*4) | 无 | ⭕ 单帧编码器 |
| 0x02 | 读 Flash | READADDR(4)+READLEN(1) | READDATA(N) | ⭕ |
| 0x03 | 获取版本 | 无 | BootVer/HwVer/FwDate/FwVer/Proto 各 4B | ✅ HwVer 拆解 |
| 0x04 | 启动 Firmware | 无 | 同 0x03 | ✅ |
| 0x06 | 设置波特率 | — 手册注明无效 | — | ❌ 不实现（用 0xAA40 KEY=0x06） |
| 0x08 | 校验 Firmware | CHECKADDR(4)+CHECKDATALEN(4)+CHECKCRC(4) | 无 | ⭕ 含 CHECKCRC 组装算法 |
| 0x09 | 启动 Bootloader | 无 | 无 | ✅（注意收成功后等 250ms；先发 0xAB 的规则写在 doc） |
| 0x0C | 运行阶段 | 无 | Program(1): 0x11=Boot/0x12=APP | ✅ |
| 0x10 | 序列号 | Option(1)+DataFlags(1) | Year(4)+Serial(8) | ✅ |

## 5 标签盘存

| Op | 名称 | 实现 |
|---|---|---|
| 0x21 | 单标签盘存 | ✅ 两种响应（含/无 MetadataFlags） |
| 0x22 | 同步盘存 | ✅ SearchFlags(嵌入读/TAGFOCUS/温度群读 0xA5)/AccessPwd/Select/Embedded；响应 TagsFound 1/4 字节两态 + 嵌入结果 |
| 0x29 | 取标签缓冲 | ✅ ReadOption 0/1；TagEpcAndMeta 批量解析 + FASTID 判定 |
| 0x2A | 清缓存 | ❌ 手册注明已无效 |
| 0xAA48 | 开始异步盘存 | ✅ 心跳/单天线自停/轮询通知三个高位开关 |
| 0xAA49 | 停止异步盘存 | ✅ |
| 0xAA58 | EX 异步盘存 | ✅ ExConfigData(20B) 密集/稀疏模式 |
| 0xAA59 | 停止 EX 异步 | ✅ |
| — | 标签通知包 | ✅ EPCLength 字节语义 |
| — | 心跳包 "XTSJ" | ✅ |
| — | 轮询周期包 | ✅ ANTWORKCNT 特殊域 |

## 6 标签访问

| Op | 名称 | 实现 |
|---|---|---|
| 0x23 | 写 EPC | ✅ WriteReadFlag=0x8C 写后读 |
| 0x24 | 写标签 | ✅ WriteReadFlag=0x84；一次最多 32 字 |
| 0x25 | 锁标签 | ✅ Mask/Action 低 10 位 + Perm/W/R 语义 |
| 0x26 | 销毁 | ✅ KillPwd+RFU |
| 0x28 | 读存储区 | ✅ 一次最多 96 字；两种响应 |
| 0x2D | 块写 | ✅ ChipType=0, Option.BIT6=1, SubCmd=0x00C7, 最多 48 字 |

## 7 设置指令

| Op | 名称 | 实现 |
|---|---|---|
| 0x91 | 天线端口 | ✅ 三形态：无 Option 单口 / Option=2 多口 / Option=3,4 功率(+SettingTime) |
| 0x93 | 标签协议 | ✅ 固定 0x0005 |
| 0x95 | 跳频/驻留 | ✅ 频表(≤50) 与 Option=2 驻留时间 20-60000ms |
| 0x96 | GPO | ✅ 设 N 对 + 读 N 引脚（2 路） |
| 0x97 | 工作区域 | ✅ RegionCode 枚举（附录 3 两张表） |
| 0x9A | 读写器配置 | ✅ Key 0x00/0x06/0x08 |
| 0x9B | 协议配置 | ✅ Session/Target(静,动)/RFMode(+100 偏移)/Q(动,静 0-15) |

## 8 获取指令

| Op | 名称 | 实现 |
|---|---|---|
| 0x61 | 天线配置 | ✅ Option 0/2/3/4/5 五种响应形态 |
| 0x62 | 功率信息 | ✅ Option 0/1 |
| 0x63 | 当前协议 | ✅ 恒 0x0005 |
| 0x65 | 跳频设置 | ✅ 频表 / Option=2 驻留时间 |
| 0x66 | GPI | ✅ |
| 0x67 | 当前区域 | ✅ |
| 0x6A | 读写器配置 | ✅ |
| 0x6B | 协议配置 | ✅ 与 0x9B 参数表对应 |
| 0x70 | 可用协议 | ✅ |
| 0x71 | 可用区域 | ✅ |
| 0x72 | 温度 | ✅ 有符号 1 字节 |

## 9 永久保存配置（0xAA40）

| KEY | 内容 | 实现 |
|---|---|---|
| 0x91 | 默认天线+功率 | ✅ get: 0x5A 或 4+7N；set: KEYDATA=完整 0x91 帧(Option=2/4) |
| 0x95 | 默认频率/跳频时间 | ✅ 仅中国版；get 变长 |
| 0x97 | 默认区域 | ✅ |
| 0x9A | 默认读写器配置 | ✅ |
| 0x9B | 默认协议配置 | ✅ 3/4 字节变长 |
| 0x04 | 上电直达 APP | ✅ 0xA5A55A5A 魔数 |
| 0x06 | 默认波特率 | ✅ 附录 2 枚举 |
| 0xAA | 恢复出厂 | ✅ |
| 0xAB | 准备升级 | ✅ |

## 10 其它指令

| SubCode | 名称 | 实现 |
|---|---|---|
| 0xAA4A | 回波检测 | ✅ 变长 5+4M；VSWR=(1+RL)/(RL-1) 换算 |
| 0xAA4C | 多标签过滤 | ✅ SELFLAG 分片规则 + ≤16 标签×≤255bit |
| 0xAA50 | 宜链温度 | ✅ 三个等待时间 + 0x28 形态复用 + 温度换算 |
| 0xAA51 | 宜链亮灯 | ✅ Option.BIT7 重定义 Timeout；0x07 多标签齐亮 |

## 状态码（响应通用）

- 全量枚举（手册 p.164-167，约 40 项），`Error::Status(Status)` 携带；
- 特殊：0xAA49/0xAA59 = "异步盘存被其它指令打断"，传输层语义化处理。
