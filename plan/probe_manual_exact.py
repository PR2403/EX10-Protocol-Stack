#!/usr/bin/env python3
"""精确复现手册附录1的C代码语义(数据位从LSB插入), 并用真机向量验证。

手册 C 代码关键点(与标准CRC-CCITT不同!):
  for i in 0..8:
      xorFlag = reg & 0x8000      # 记录将移出的最高位
      reg <<= 1                   # 先左移
      reg |= bit                  # !!! 数据位插入到第0位(LSB), 标准CRC是插入第8位
      if xorFlag: reg ^= poly
其中 bit 按 MSB-first 从字节取出(dcdBitMask 从 0x80 右移)。
"""
AUTH = [
    (bytes([0x00, 0x03]), 0x1D0C),
    (bytes([0x14,0x03,0x00,0x00,0x18,0x06,0x04,0x00,0xA9,0x00,0x00,0x01,
            0x20,0x20,0x07,0x03,0x20,0x07,0x03,0x00,0x00,0x00,0x00,0x10]), 0xB031),
]


def manual_c(data: bytes, poly=0x1021, init=0xFFFF, msb_first=True) -> int:
    reg = init
    for b in data:
        mask = 0x80 if msb_first else 0x01
        for _ in range(8):
            xor_flag = reg & 0x8000
            reg = (reg << 1) & 0xFFFF
            bit = 1 if (b & mask) == mask else 0
            reg |= bit
            if xor_flag:
                reg ^= poly
            mask >>= 1 if msb_first else -1  # LSB-first 则左移
            if not msb_first:
                mask = (mask << 1) & 0xFF if _ >= 0 else mask
        # 修正: LSB-first 分支的 mask 处理有误, 重新实现
    return reg


def manual_c_v2(data: bytes, poly=0x1021, init=0xFFFF, msb_first=True) -> int:
    reg = init
    for b in data:
        masks = ([0x80,0x40,0x20,0x10,0x08,0x04,0x02,0x01] if msb_first
                 else [0x01,0x02,0x04,0x08,0x10,0x20,0x40,0x80])
        for m in masks:
            xor_flag = reg & 0x8000
            reg = (reg << 1) & 0xFFFF
            reg |= 1 if (b & m) == m else 0
            if xor_flag:
                reg ^= poly
    return reg


if __name__ == "__main__":
    for name, fn in [("MSB-first(手册原文)", lambda d: manual_c_v2(d, msb_first=True)),
                     ("LSB-first(变体)",    lambda d: manual_c_v2(d, msb_first=False))]:
        print(f"== {name} ==")
        for data, want in AUTH:
            got = fn(data)
            print(f"  {'OK ' if got == want else 'MISS'} want={want:04X} got={got:04X}  data={data[:6].hex()}...")
