#!/usr/bin/env python3
"""从PDF原文重新核对 0xAA 扩展帧与 SubCRC 的字节序列。"""
from pypdf import PdfReader

r = PdfReader("doc/EX10系列模块通信协议-230813.pdf")
# 目标页(0-based): p59(异步例1), p60(例2,3), p65(停止), p143(上电APP), p144(波特率), p151(回波)
pages = {
    "p59 异步例1": 58,
    "p60 异步例2": 59,
    "p65 停止异步": 64,
    "p143 APP": 142,
    "p144 波特率": 143,
    "p151 回波": 150,
}
for name, idx in pages.items():
    t = r.pages[idx].extract_text()
    # 找 FF 开头的 hex 行
    print(f"===== {name} =====")
    for line in t.splitlines():
        if "FF" in line and any(c in line for c in "0123456789ABCDEF"):
            print("  ", line.strip())
    print()
