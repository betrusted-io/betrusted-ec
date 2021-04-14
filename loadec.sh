sudo wishbone-tool 0x40080000 --burst-source target/riscv32i-unknown-none-elf/release/bt-ec.bin
sudo wishbone-tool 0x40080000 --burst-length 167936 > /tmp/bt-ec.verify
diff -s /tmp/bt-ec.verify target/riscv32i-unknown-none-elf/release/bt-ec.bin
