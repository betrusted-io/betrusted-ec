#!/bin/bash

dd if=../build/software/bios/bios.bin of=/tmp/bios.bin bs=1 skip=106496
cat ../build/gateware/top_pad.bin /tmp/bios.bin > /tmp/bt-ec.bin
