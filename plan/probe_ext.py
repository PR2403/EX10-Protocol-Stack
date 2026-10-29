#!/usr/bin/env python3
"""验证修正后的完整扩展帧(含 SubCRC + 0xBB + CRC16)与 SubCRC 计算范围。"""

def crc_ex10(data: bytes) -> int:
    reg = 0xFFFF
    for b in data:
        for m in (0x80,0x40,0x20,0x10,0x08,0x04,0x02,0x01):
            xor_flag = reg & 0x8000
            reg = (reg << 1) & 0xFFFF
            if b & m:
                reg |= 1
            if xor_flag:
                reg ^= 0x1021
    return reg


def frame_crc(frame_wo_crc: bytes) -> int:
    return crc_ex10(frame_wo_crc[1:])  # 跳过 0xFF


def subcrc(data: bytes) -> int:
    return sum(data) & 0xFF


EXT_FRAMES = {
    # p.59 异步盘存例1: ...AA48 + SubData(00 BF 00 80 03) + SubCRC(29) + BB + CRC(0F?) ...
    # 从PDF: FF 13 AA 4D..63 68 AA 48 00 BF 00 80 03 34 BB 29 0F  ← 提取文本疑乱, 逐帧核对
    "p59-1": ("FF 13 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 48 00 BF 00 80 03 34 BB", "290F"),
    "p60-2": ("FF 1D AA 4D 6F 64 75 6C 65 74 65 63 68 AA 48 00 BF 04 80 03 00 00 00 00 00 00 00 20 08 E2 42 BB", "AB26"),
    "p60-3": ("FF 2A AA 4D 6F 64 75 6C 65 74 65 63 68 AA 48 00 BF 02 80 07 00 00 00 00 00 00 00 20 0C E2 00 01 09 28 00 00 00 02 00 00 00 00 02 7E BB", "D091"),
    "p65 停止异步": ("FF 0E AA 4D 6F 64 75 6C 65 74 65 63 68 AA 49 F3 BB", "0391"),
    "p68 EX异步": ("FF 13 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 58 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 16 00 80 03 9B BB", "0ABF"),
    "p69 停止EX": ("FF 0E AA 4D 6F 64 75 6C 65 74 65 63 68 AA 59 03 BB", "E1A0"),
    "p151 回波": ("FF 13 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 4A 0B B8 01 01 00 B9 BB", "BD67"),
}


if __name__ == "__main__":
    for key, (frame_hex, want_hex) in EXT_FRAMES.items():
        try:
            data = bytes.fromhex(frame_hex.replace(" ", ""))
        except ValueError as e:
            print(f"{key}: 帧十六进制录入错误 {e}")
            continue
        want = int(want_hex, 16)
        got = frame_crc(data)
        print(f"{'OK ' if got==want else 'MISS'} {key:12s} want={want:04X} got={got:04X}  {frame_hex[:36]}...")
