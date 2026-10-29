#!/usr/bin/env python3
"""真机探测: 验证手册例帧 FF 00 03 1D 0C (获取版本) 是否被设备接受。"""
import serial, sys

PORT = "/dev/ttyUSB0"

def main():
    ser = serial.Serial(PORT, 115200, timeout=0.5)
    ser.reset_input_buffer()

    tests = [
        ("手册原样 FF 00 03 1D 0C", bytes([0xFF,0x00,0x03,0x1D,0x0C])),
        ("CCITT-FALSE(00 03)=3F5D", bytes([0xFF,0x00,0x03,0x3F,0x5D])),
        ("CCITT含头(FF 00 03)",     bytes([0xFF,0x00,0x03]) ),  # 占位, 下面填
    ]
    # 计算含头的 CCITT-FALSE
    def ccitt_false(data):
        reg = 0xFFFF
        for b in data:
            reg ^= b << 8
            for _ in range(8):
                reg = ((reg << 1) ^ 0x1021) & 0xFFFF if reg & 0x8000 else (reg << 1) & 0xFFFF
        return reg
    c = ccitt_false(bytes([0xFF,0x00,0x03]))
    tests[2] = (f"CCITT-FALSE 含帧头 = {c:04X}", bytes([0xFF,0x00,0x03,c>>8,c&0xFF]))
    c2 = ccitt_false(bytes([0x00,0x03,0x1D,0x0C]))
    tests.append((f"若CRC也参与下一帧校验({c2:04X}) 忽略", b""))

    for name, frame in tests:
        if not frame:
            continue
        ser.reset_input_buffer()
        ser.write(frame)
        resp = ser.read(64)
        print(f"{name:34s} -> {'无响应' if not resp else resp.hex(' ').upper()}")

    ser.close()

if __name__ == "__main__":
    main()
