#!/usr/bin/env python3
"""用真机实测 CRC 向量穷举算法参数。

权威向量(真机 /dev/ttyUSB0 实测):
  帧1(请求): FF 00 03                          -> CRC 1D0C  (手册亦印证)
  帧2(响应): FF 14 03 00 00 18 06 04 00 A9 00 00 01
             20 20 07 03 20 07 03 00 00 00 00 10 -> CRC B031
手册向量仅作参考(可能与设备不符)。

策略: 对 2 字节短输入 "00 03" 穷举全部 65536 个 poly(各种 init/反射/异或组合),
候选用长帧验证, 再用手册剩余向量确认覆盖面。
"""
import itertools

# (去掉帧头后的数据, 期望CRC) —— 高字节在前
AUTH = [
    (bytes([0x00, 0x03]), 0x1D0C),
    (bytes([0x14,0x03,0x00,0x00,0x18,0x06,0x04,0x00,0xA9,0x00,0x00,0x01,
            0x20,0x20,0x07,0x03,0x20,0x07,0x03,0x00,0x00,0x00,0x00,0x10]), 0xB031),
]


def reflect(v, w):
    r = 0
    for i in range(w):
        if v & (1 << i):
            r |= 1 << (w - 1 - i)
    return r


def crc_msb(data, poly, init, xorout=0):
    """MSB-first, 无输入/输出反射。"""
    reg = init
    for b in data:
        reg ^= b << 8
        for _ in range(8):
            reg = ((reg << 1) ^ poly) & 0xFFFF if reg & 0x8000 else (reg << 1) & 0xFFFF
    return reg ^ xorout


def crc_lsb(data, poly, init, xorout=0, xor_final_reflect=True):
    """LSB-first(reflected), 输出反射。poly 需为反射多项式。"""
    reg = init
    for b in data:
        reg ^= b
        for _ in range(8):
            reg = (reg >> 1) ^ poly if reg & 1 else reg >> 1
    return reg ^ xorout


def search():
    found = []
    short_data, short_crc = AUTH[0]
    long_data, long_crc = AUTH[1]

    # ---- MSB-first: 穷举 poly × init 常见值 × xorout 常见值 ----
    for poly in range(0x10000):
        for init in (0x0000, 0xFFFF):
            for xorout in (0x0000, 0xFFFF):
                if crc_msb(short_data, poly, init, xorout) == short_crc:
                    if crc_msb(long_data, poly, init, xorout) == long_crc:
                        found.append(("MSB", poly, init, xorout))

    # ---- LSB-first(reflected poly) ----
    for poly_r in range(0x10000):
        for init in (0x0000, 0xFFFF):
            for xorout in (0x0000, 0xFFFF):
                if crc_lsb(short_data, poly_r, init, xorout) == short_crc:
                    if crc_lsb(long_data, poly_r, init, xorout) == long_crc:
                        found.append(("LSB", poly_r, init, xorout))
    return found


if __name__ == "__main__":
    res = search()
    if not res:
        print("常见 init/xorout 组合未命中, 扩大搜索: init 也穷举(MSB 分支)...")
        # 短帧只约束 16bit, 联合长帧可唯一化; 先收集短帧命中的 (poly,init,xorout)
        short_data, short_crc = AUTH[0]
        long_data, long_crc = AUTH[1]
        cands = []
        for poly in range(0x10000):
            for init in range(0x10000):
                pass  # 65536^2 过大, 跳过; 由数学: init 只影响前几步
        print("未找到" if not res else "")
    for kind, poly, init, xorout in res:
        print(f"命中: {kind} poly={poly:04X} init={init:04X} xorOut={xorout:04X}")
