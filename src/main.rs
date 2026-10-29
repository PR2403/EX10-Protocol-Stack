//! EX10 UHF 读卡器完整 CLI（clap）。
//!
//! 运行 `uhf-demo --help` 查看全部命令；命令实现见 `src/cli/`。

mod cli;

fn main() -> std::process::ExitCode {
    cli::run()
}
