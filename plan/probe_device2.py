#!/usr/bin/env python3
"""真机双向验证: 用已确认算法发送指令, 校验响应帧 CRC 与 TagCRC/SubCRC 一致性。"""
import serial

PORT = "/dev/ttyUSB0"


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


def build(op: int, data: bytes = b"") -> bytes:
    body = bytes([0xFF, len(data), op]) + data
    c = crc_ex10(body[1:])
    return body + bytes([c >> 8, c & 0xFF])


def parse(resp: bytes):
    assert resp[0] == 0xFF, "帧头错误"
    ln, op = resp[1], resp[2]
    status = (resp[3] << 8) | resp[4]
    data = resp[5:5+ln]
    crc_got = (resp[5+ln] << 8) | resp[6+ln]
    crc_calc = crc_ex10(resp[1:5+ln])
    return op, status, data, crc_got, crc_calc


def main():
    ser = serial.Serial(PORT, 115200, timeout=1.0)

    # 1) 获取版本
    ser.reset_input_buffer()
    tx = build(0x03)
    ser.write(tx)
    resp = ser.read(64)
    op, st, data, cg, cc = parse(resp)
    print(f"0x03 版本   TX={tx.hex(' ').upper()}")
    print(f"          RX={resp.hex(' ').upper()}")
    print(f"          status={st:04X} CRC {'OK' if cg==cc else 'BAD'}({cg:04X}/{cc:04X}) data={data.hex(' ').upper()}")

    # 2) 获取序列号
    ser.reset_input_buffer()
    tx = build(0x10, bytes([0x00, 0x00]))
    ser.write(tx)
    resp = ser.read(64)
    if resp and resp[0] == 0xFF:
        op, st, data, cg, cc = parse(resp)
        print(f"0x10 序列号  status={st:04X} CRC {'OK' if cg==cc else 'BAD'} data={data.hex(' ').upper()}")

    # 3) 扩展指令: 0xAA40 KEY=0x04 GET (是否上电至APP)
    marker = bytes.fromhex("4D 6F 64 75 6C 65 74 65 63 68")
    sub = bytes([0xAA, 0x40, 0x04, 0x00])   # KEY=04, GETORSET=0(读)
    subcrc = sum(sub) & 0xFF
    payload = marker + bytes([0xAA,0x40]) + sub[2:] + bytes([subcrc, 0xBB])
    # 注意: subcode 在 marker 后, 重算范围 = AA40 04 00
    subcrc = sum(bytes([0xAA,0x40,0x04,0x00])) & 0xFF
    payload = marker + bytes([0xAA,0x40,0x04,0x00,subcrc,0xBB])
    tx = build(0xAA, payload)
    ser.reset_input_buffer()
    ser.write(tx)
    resp = ser.read(64)
    if resp and resp[0] == 0xFF:
        op, st, data, cg, cc = parse(resp)
        print(f"0xAA40-04读 TX={tx.hex(' ').upper()}")
        print(f"          RX={resp.hex(' ').upper()}")
        print(f"          status={st:04X} CRC {'OK' if cg==cc else 'BAD'} data={data.hex(' ').upper()}")

    ser.close()


if __name__ == "__main__":
    main()
