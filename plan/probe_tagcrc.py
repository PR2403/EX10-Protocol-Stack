#!/usr/bin/env python3
"""TagCRC(附录5)与 SubCRC(附录7) 全量核对。"""


def tagcrc(seed, data: bytes) -> int:
    crc = seed
    for b in data:
        for mask in (0x80,0x40,0x20,0x10,0x08,0x04,0x02,0x01):
            if crc & 0x8000:
                crc = ((crc << 1) ^ 0x1021) & 0xFFFF
            else:
                crc = (crc << 1) & 0xFFFF
            if b & mask:
                crc ^= 0x1021
    return (~crc) & 0xFFFF


def subcrc(data: bytes) -> int:
    return sum(data) & 0xFF


if __name__ == "__main__":
    print("== TagCRC (附录5) ==")
    # pc 3000 epc 050000000000000000002354 -> tagcrc 4AC8
    pcepc = bytes.fromhex("30 00 05 00 00 00 00 00 00 00 00 00 23 54")
    got = tagcrc(0xFFFF, pcepc)
    print(f"  {'OK' if got == 0x4AC8 else 'MISS'}  want=4AC8 got={got:04X}")

    print("== SubCRC (附录7) ==")
    cases = [
        (bytes([0xAA,0x40,0xAA,0x01]), 0x95),                       # 附录7原文
        (bytes([0xAA,0x49]), 0xF3),                                 # p.65
        (bytes([0xAA,0x59]), 0x03),                                 # p.69
        (bytes([0xAA,0x40,0x04,0x00]), None),                       # 真机实测 EE
        (bytes([0xAA,0x40,0x06,0x00]), 0xF0),                       # p.144
        (bytes([0xAA,0x40,0xAA,0x01]), 0x95),                       # p.146
        (bytes([0xAA,0x40,0xAB,0x01]), 0x96),                       # p.147
        (bytes([0xAA,0x4A,0x0B,0xB8,0x01,0x01,0x00]), 0xB9),       # p.152
        (bytes([0xAA,0x4C,0xFF,0xFF,0x03]), None),                  # 需含全部 SubData
    ]
    for data, want in cases:
        got = subcrc(data)
        if want is None:
            print(f"  info got={got:02X}  {data.hex()}")
        else:
            print(f"  {'OK' if got == want else 'MISS'}  want={want:02X} got={got:02X}  {data.hex()}")


