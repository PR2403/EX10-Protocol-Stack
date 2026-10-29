#!/usr/bin/env python3
"""p68 EX异步帧重建验证: DataLength=0x27(非提取文本的0x13), MetadataFlags=0x0016。"""

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

# 自洽帧: FF 27 AA Moduletech AA58 ExConfig(20B=19个00+首字节00) MetaFlags=0016 Opt=00 SF=8003 SubCRC=9B BB
frame = bytes.fromhex(
    "FF 27 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 58 "
    + "00 "*20
    + "00 16 00 80 03 9B BB"
)
crc = crc_ex10(frame[1:])
print(f"帧长={len(frame)} (期望44)  CRC={crc:04X} (手册期望 0ABF)")

# 也验证提取文本原样的 0x13 版本(供对照)
frame2 = bytes.fromhex(
    "FF 13 AA 4D 6F 64 75 6C 65 74 65 63 68 AA 58 "
    + "00 "*20
    + "00 16 00 80 03 9B BB"
)
crc2 = crc_ex10(frame2[1:])
print(f"对照(0x13版) CRC={crc2:04X}")
