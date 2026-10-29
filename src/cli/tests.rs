//! CLI 定义自检与参数解析回归测试。

#![cfg(test)]

use clap::CommandFactory;

use super::*;

#[test]
fn cli_definition_is_valid() {
    Cli::command().debug_assert();
}

#[test]
fn parses_core_commands() {
    let cli = Cli::try_parse_from(["uhf-demo", "info"]).unwrap();
    assert!(matches!(cli.command, Command::Info));

    let cli = Cli::try_parse_from([
        "uhf-demo",
        "inventory",
        "run",
        "--timeout-ms",
        "500",
        "--antenna",
        "1",
        "--buffer-flags",
        "0x0015",
    ])
    .unwrap();
    match cli.command {
        Command::Inventory(InventoryCmd::Run(args)) => {
            assert_eq!(args.sync.timeout_ms, 500);
            assert_eq!(args.sync.antenna, Some(1));
            assert_eq!(args.buffer_flags, MetadataFlags(0x0015));
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let cli = Cli::try_parse_from(["uhf-demo", "tag", "write", "user", "AABBCCDD"]).unwrap();
    match cli.command {
        Command::Tag(TagCmd::Write { bank, data, .. }) => {
            assert!(matches!(bank, BankArg::User));
            assert_eq!(data, "AABBCCDD");
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn parses_extended_commands() {
    let cli = Cli::try_parse_from([
        "uhf-demo",
        "inventory",
        "start",
        "--duration",
        "3",
        "--embed-read",
        "tid:0:6",
        "--tagfocus",
    ])
    .unwrap();
    match cli.command {
        Command::Inventory(InventoryCmd::Start(args)) => {
            assert_eq!(args.embed_read.as_deref(), Some("tid:0:6"));
            assert!(args.tagfocus);
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let cli = Cli::try_parse_from(["uhf-demo", "raw", "ext", "AA40", "--data", "06 00"]).unwrap();
    match cli.command {
        Command::Raw(RawCmd::Ext { subcode, data, .. }) => {
            assert_eq!(subcode, 0xAA40);
            assert_eq!(data, "06 00");
        }
        other => panic!("unexpected command: {other:?}"),
    }

    let cli = Cli::try_parse_from(["uhf-demo", "persist", "baud", "set", "921600"]).unwrap();
    match cli.command {
        Command::Persist(PersistCmd::Baud(BaudPersistCmd::Set { rate })) => {
            assert_eq!(rate, BaudRate::Baud921600);
        }
        other => panic!("unexpected command: {other:?}"),
    }
}
