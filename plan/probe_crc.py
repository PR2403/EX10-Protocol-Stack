#!/usr/bin/env python3
"""探测 EX10 手册帧尾 CRC-16 的真实算法。

手册附录 1 给出 CCITT(poly=0x1021, init=0xFFFF) 的 C 代码，但手册未必准确。
策略：
1. 直接按手册算法计算所有黄金向量，统计命中率；
2. 若不完全命中，则暴力搜索参数空间：
   poly / init / refIn / refOut / xorOut / 字节序 / 计算起点(是否含帧头0xFF)。
"""
import itertools

# ---- 黄金向量: (不含CRC的帧字节, 手册CRC两字节) ----
# 全部取自 doc/EX10系列模块通信协议-230813.pdf 的指令实例
VECTORS = [
    # --- 请求帧 (手册请求帧: FF Len Op [Data] CRC) ---
    ([0xFF,0x00,0x03], 0x1D0C),                      # 获取版本 p.24
    ([0xFF,0x00,0x04], 0x1D0B),                      # 启动FW p.25
    ([0xFF,0x00,0x09], 0x1D06),                      # 启动Boot p.27
    ([0xFF,0x00,0x0C], 0x1D03),                      # 运行阶段 p.29
    ([0xFF,0x02,0x10,0x00,0x00], 0xF093),            # 序列号 p.31
    ([0xFF,0x00,0x65], 0x1D6A),                      # 获取跳频表 p.122
    ([0xFF,0x00,0x66], 0x1D69),                      # 获取GPI p.122
    ([0xFF,0x00,0x71], 0x1D7E),                      # 可用区域 p.129
    ([0xFF,0x00,0x72], 0x1D7D),                      # 温度 p.130
    ([0xFF,0x01,0x97,0x01], 0x4BBC),                 # 设区域NA p.106
    ([0xFF,0x01,0x97,0x06], 0x4BBB),                 # 设区域CN1 p.106
    ([0xFF,0x02,0x93,0x00,0x05], 0x517D),            # 设协议 p.102
    ([0xFF,0x00,0x23], 0x0000),                      # 占位无效,下面删
    # --- 响应帧 ---
    ([0xFF,0x00,0x01,0x00,0x00], 0x94E1),            # 写Flash响应 p.20
    ([0xFF,0x00,0x08,0x00,0x00], 0x05C8),            # 校验FW响应 p.27 (手册印 05C8)
    ([0xFF,0x00,0x09,0x00,0x00], 0x15E9),            # 启动Boot响应 p.28
    ([0xFF,0x01,0x0C,0x00,0x00,0x12], 0x6343),       # 运行阶段响应 p.29
    ([0xFF,0x14,0x03,0x00,0x00,0x00,0x22,0x02,0x18,0x00,
      0x31,0x00,0x00,0x00,0x20,0x22,0x07,0x08,0x22,0x07,0x08,
      0x00,0x00,0x00,0x00], 0x1054),                 # 版本响应 p.24 (手册印 ...10 FD 54)
    ([0xFF,0x00,0x22], 0x0000),                      # 占位无效,下面删
    ([0xFF,0x00,0x24,0x00,0x00], 0xE026),            # 写标签成功 p.79
    ([0xFF,0x00,0x24,0x04,0x24], 0xE402),            # 写标签失败 p.78
    ([0xFF,0x00,0x2D,0x00,0x00], 0x710F),            # 块写响应 p.95
    ([0xFF,0x00,0x97,0x00,0x00], 0x779E),            # 设区域响应 p.107
    ([0xFF,0x00,0x9A,0x00,0x00], 0xA633),            # 设配置响应 p.109
    ([0xFF,0x00,0xAA,0xAA,0x49], 0x1EEA),            # 异步被打断 p.65
]
# 删占位
VECTORS = [(f, c) for f, c in VECTORS if c != 0x0000 or 0x00 in (f[-1],)]


def reflect(v: int, w: int) -> int:
    r = 0
    for i in range(w):
        if v & (1 << i):
            r |= 1 << (w - 1 - i)
    return r


def crc16(data: bytes, poly: int, init: int, refIn: bool, refOut: bool, xorOut: int) -> int:
    reg = init
    for b in data:
        if refIn:
            b = reflect(b, 8)
        reg ^= b << 8
        for _ in range(8):
            if reg & 0x8000:
                reg = ((reg << 1) ^ poly) & 0xFFFF
            else:
                reg = (reg << 1) & 0xFFFF
    if refOut:
        reg = reflect(reg, 16)
    return reg ^ xorOut


def manual_algo(data: bytes) -> int:
    """手册附录1: 从 msgbuf[1] 开始(跳过帧头0xFF), poly 0x1021, init 0xFFFF, 不反转。"""
    return crc16(data[1:], 0x1021, 0xFFFF, False, False, 0x0000)


def try_all():
    polys = {0x1021, 0x8005, 0x8408, 0xA001, 0x3D65, 0xC867, 0x0589, 0x8BB7}
    inits = {0x0000, 0xFFFF, 0x1D0F, 0xFFFF & 0, 0xB2AA, 0x89EC}
    results = []
    for poly, init, ri, ro, xo, skip in itertools.product(
            polys, inits, (False, True), (False, True), (0x0000, 0xFFFF), (0, 1)):
        hit = 0
        for frame, crc in VECTORS:
            d = frame[skip:]
            # 大端比较(手册高字节在前)
            got = crc16(bytes(d), poly, init, ri, ro, xo)
            if got == crc:
                hit += 1
        if hit:
            results.append((hit, poly, init, ri, ro, xo, skip))
    results.sort(reverse=True)
    return results


def main():
    print(f"向量数: {len(VECTORS)}")
    print("\n== 手册算法(附录1)命中率 ==")
    hit = 0
    for frame, crc in VECTORS:
        got = manual_algo(bytes(frame))
        ok = "OK " if got == crc else "MISS"
        if got == crc:
            hit += 1
        print(f"{ok} want={crc:04X} got={got:04X}  {' '.join(f'{b:02X}' for b in frame)}")
    print(f"命中 {hit}/{len(VECTORS)}")

    print("\n== 参数空间搜索 TOP10 ==")
    for hit, poly, init, ri, ro, xo, skip in try_all()[:10]:
        print(f"hit={hit:2d}/{len(VECTORS)} poly={poly:04X} init={init:04X} "
              f"refIn={int(ri)} refOut={int(ro)} xorOut={xo:04X} skip_header={skip}")


if __name__ == "__main__":
    main()
