#!/bin/bash
if [ -z "$1" ]
then
    OUTPUT=/tmp/bt-ec.bin
else
    OUTPUT=$1
fi

riscv64-unknown-elf-as -fpic loader.S -o loader.elf
riscv64-unknown-elf-objcopy -O binary loader.elf /tmp/loader.bin
dd if=/dev/null of=/tmp/loader.bin bs=1 count=1 seek=4096

riscv64-unknown-elf-objcopy -O binary ../sw/target/riscv32i-unknown-none-elf/release/betrusted-ec /tmp/betrusted-ec.bin
BINLEN=`stat -c %s /tmp/betrusted-ec.bin`
if [ $BINLEN -ge 49152 ]
then
   echo "Warning: executable is longer than 48k, loader will fail to copy full binary!!!"
fi
cat ../build/gateware/top_pad.bin /tmp/loader.bin /tmp/betrusted-ec.bin > $OUTPUT
