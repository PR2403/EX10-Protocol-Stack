# 06 依赖与 Cargo 配置

## Cargo.toml（规划）

```toml
[package]
name = "uhf-ex10"
version = "0.1.0"
edition = "2024"

[features]
default = ["std"]
std = ["dep:serialport", "dep:log", "dep:env_logger"]

[dependencies]
heapless = "0.8"          # no_std 定长 Vec/VecDeque/String

# 仅 std
serialport = { version = "4", optional = true }
log = { version = "0.4", optional = true }
env_logger = { version = "0.11", optional = true }

[dev-dependencies]       # 测试在 host 上跑，可用 std
```

> 现有包名 `UHF` 与二进制名冲突（需同时出 lib + bin），规划改为：
> - lib 名 `uhf_ex10`（`src/lib.rs`），`#![cfg_attr(not(feature = "std"), no_std)]`
> - bin `uhf-demo`（`src/main.rs`，`[[bin]] name="uhf-demo" path="src/main.rs"`，依赖 lib 的 std feature）

## 依赖选型理由

| 依赖 | 为什么 | 替代方案 |
|---|---|---|
| heapless 0.8 | no_std 下唯一需要的容器（帧缓冲 256B）；协议天然定长上限 | 自写 FixedBuf（不必要） |
| serialport 4 | Linux ttyUSB 事实标准，超时读支持完善 | tokio-serial（不引入 async） |
| CRC 三种 | **帧 CRC 为手册私有非标准算法（数据位进 bit0，实测确认），任何标准 CRC 库不可用**，必须自实现；TagCRC/SubCRC 同理由手册 C 代码直接翻译并以真机向量锁定 | ~~`crc` crate~~（参数不匹配，实测排除） |
| log/env_logger | demo 可观测性 | println（保留 trace 能力更好） |

不引入：`embedded-hal`（本期不做 MCU 移植，但 `Link` trait 保持兼容其 Read/Write 语义，后续加 feature 即可）、`nb`、`defmt`（同上，后续 feature）。

## 版本工具链

- rustc ≥ 1.85（edition 2024）
- 检查项：`cargo build --no-default-features`（no_std 门槛）、`cargo test`、`cargo clippy --all-features -- -D warnings`
