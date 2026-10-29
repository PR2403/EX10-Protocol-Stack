#!/usr/bin/env python3
"""穷举 init: 利用'init 的作用等价于对首 2 字节的一次性扰动'数学性质。

标准 CRC 中, init=I 等价于先算 I 走 16 步空移位。因此对固定 data 与 poly:
  crc(data; init=I) = crc(data; init=0) XOR f(I)
其中 f(I) 只依赖 poly 与 len。=> 可先算出目标 delta, 再反解 I。

更简单直接的做法: 对每个 poly, init=0 时短帧与长帧各算一次, 得到
  s0 = crc(short;0), l0 = crc(long;0)
标准算法中 init 的贡献是'穿过多余字节的线性传播', 不易闭式, 所以这里
干脆对每个 poly 穷举 init (65536) —— 总量 2^32 太大。

换个思路: 不设 init, 而是把 init 视为前置虚拟字节。即算法可能是
'init=0 但把帧头 0xFF 当作首字节参与' 或 'data 前有隐藏字节'。
因此本脚本穷举: poly(65536) × {init=0, 前缀字节 0x00/0xFF} × xorout(推导)。
由于 xorout = want XOR crc计算值, 每个候选只需验证两帧的 xorout 一致即可。
"""
POLY_RANGE = range(0x10000)

AUTH = [
    (bytes([0x00, 0x03]), 0x1D0C),
    (bytes([0x14,0x03,0x00,0x00,0x18,0x06,0x04,0x00,0xA9,0x00,0x00,0x01,
            0x20,0x20,0x07,0x03,0x20,0x07,0x03,0x00,0x00,0x00,0x00,0x10]), 0xB031),
]


def crc_msb(data, poly, init=0):
    reg = init
    for b in data:
        reg ^= b << 8
        for _ in range(8):
            reg = ((reg << 1) ^ poly) & 0xFFFF if reg & 0x8000 else (reg << 1) & 0xFFFF
    return reg


def crc_lsb(data, poly_r, init=0):
    reg = init
    for b in data:
        reg ^= b
        for _ in range(8):
            reg = (reg >> 1) ^ poly_r if reg & 1 else reg >> 1
    return reg


def search(mode):
    sd, sc = AUTH[0]
    ld, lc = AUTH[1]
    hits = []
    for poly in POLY_RANGE:
        f = crc_msb if mode == "MSB" else crc_lsb
        s0 = f(sd, poly)
        l0 = f(ld, poly)
        # xorout 需同时满足: sc^s0 == lc^l0 == xorout
        if (sc ^ s0) == (lc ^ l0):
            hits.append((poly, sc ^ s0))
    return hits


if __name__ == "__main__":
    for mode in ("MSB", "LSB"):
        hits = search(mode)
        print(f"{mode}: {len(hits)} 个候选")
        for poly, xo in hits[:20]:
            print(f"  poly={poly:04X} xorOut={xo:04X}")
