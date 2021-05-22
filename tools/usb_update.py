#! /usr/bin/env python3

import argparse

import usb.core
import usb.util
import array

def peek(dev, addr):
    _dummy_s = '\x00'.encode('utf-8')
    data = array.array('B', _dummy_s * 4)

    numread = dev.ctrl_transfer(bmRequestType=(0x80 | 0x43), bRequest=0,
    wValue=(addr & 0xffff), wIndex=((addr >> 16) & 0xffff),
    data_or_wLength=data, timeout=500)

    read_data = int.from_bytes(data.tobytes(), byteorder='little', signed=False)
    print("0x{:08x}".format(read_data))
    #numread = dev._ctx.backend.ctrl_transfer(dev._ctx.handle,
    #    (0x80 | 0x43),
    #    0,
    #    addr & 0xffff,
    #    ((addr >> 16) & 0xffff),
    #    data,
    #    500
    #)

def poke(dev, addr, wdata, check=False):
    if check == True:
        _dummy_s = '\x00'.encode('utf-8')
        data = array.array('B', _dummy_s * 4)

        numread = dev.ctrl_transfer(bmRequestType=(0x80 | 0x43), bRequest=0,
        wValue=(addr & 0xffff), wIndex=((addr >> 16) & 0xffff),
        data_or_wLength=data, timeout=500)

        read_data = int.from_bytes(data.tobytes(), byteorder='little', signed=False)
        print("before poke: 0x{:08x}".format(read_data))

    data = array.array('B', wdata.to_bytes(4, 'little'))
    numwritten = dev.ctrl_transfer(bmRequestType=(0x00 | 0x43), bRequest=0,
        wValue=(addr & 0xffff), wIndex=((addr >> 16) & 0xffff),
        data_or_wLength=data, timeout=500)

    if check == True:
        _dummy_s = '\x00'.encode('utf-8')
        data = array.array('B', _dummy_s * 4)

        numread = dev.ctrl_transfer(bmRequestType=(0x80 | 0x43), bRequest=0,
        wValue=(addr & 0xffff), wIndex=((addr >> 16) & 0xffff),
        data_or_wLength=data, timeout=500)

        read_data = int.from_bytes(data.tobytes(), byteorder='little', signed=False)
        print("after poke: 0x{:08x}".format(read_data))
    else:
        print("wrote 0x{:08x} to 0x{:08x}".format(wdata, addr))


def auto_int(x):
    return int(x, 0)

def main():
    parser = argparse.ArgumentParser(description="Upload data to a Precursor device")
    parser.add_argument(
        "-f", "--fpga", required=False, help="FPGA bitstream", type=str,
    )
    parser.add_argument(
        "-l", "--loader", required=False, help="Loader", type=str,
    )
    parser.add_argument(
        "-k", "--kernel", required=False, help="Kernel", type=str,
    )
    parser.add_argument(
        "--peek", required=False, help="Inspect an address", type=auto_int,
    )
    parser.add_argument(
        "--poke", required=False, help="Write to an address", type=auto_int, nargs=2, metavar=('addr', 'data')
    )
    parser.add_argument(
        "--check-poke", required=False, action='store_true', help="Read data before and after the poke"
    )
    parser.add_argument(
        "--config", required=False, help="Print the descriptor", action='store_true'
    )
    args = parser.parse_args()

    dev = usb.core.find(idProduct=0x5bf0, idVendor=0x1209)

    if dev is None:
        raise ValueError('Precursor device not found')

    dev.set_configuration()
    if args.config:
        cfg = dev.get_active_configuration()
        print(cfg)

    if args.peek:
        peek(dev, args.peek)

    if args.poke:
        addr, data = args.poke
        poke(dev, addr, data, check=args.check_poke)



if __name__ == "__main__":
    main()
