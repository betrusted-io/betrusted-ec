# Rust Support in Betrusted-EC

This document describes the steps required to get started developing in Rust on the Betrusted-EC project.  This involves basic compiler support, but does not yet include support for the Rust-based operating system.  This is bare-metal development.

## Background

Rust assigns specific conventions to various package names.  You will encounter the following:

* **-rt**: Rust Runtime.  This is the equivalent of `crt0.S`.  It is responsible for setting up things like the main irq handler and the various data sections.
* **-pac**: Peripheral Access Crate that is directly derived from the *svd* file.
* **-hal**: System implementation to work with the generic *embedded-hal* crate.  This usually uses the *-pac* crate.
* **-sys**: Low-level one-to-one bindings to a system library, such as *libjpeg*.  Usually contains lots of **unsafe** blocks.
* **-rs**: Rust-level crate to bind to a system library.  Uses the corresponding *-sys* crate, but has no **unsafe** methods.

## Getting Started

To set up your system for Rust development, perform the following steps.  Note that you need at least Rust 1.38+, which is current as of this writing:

1. Install Rust by visiting https://rustup.rs/
2. Install riscv support by running `rustup target add riscv32i-unknown-none-elf`
3. Install `svd2rust`, so we can convert lxsocdoc svd output to rust: `cargo install svd2rust`
4. Install `form`, which will expand the resulting file into its component parts: `cargo install form`
5. Install `rustfmt` which we'll use to beautify the automatically-generated files: `rustup component add rustfmt`
6. Install `cargo-generate` which will let you use project templates: `cargo install cargo-generate`. This will require libssl-dev as a system dependency.
7. (Optional) install `cargo-binutils`, which will let you do things like `cargo size` and `cargo objdump`: `cargo install cargo-binutils`

## Creating a Cargo crate

Create a Cargo crate by starting from the build template:

```sh
$ cargo generate --git https://github.com/betrusted-io/betrusted-rs-quickstart.git
```

At this point, you're ready to generate the Peripheral Access Crate.

## Generating the Peripheral Access Crate

1. Enter the empty `betrusted-pac` directory: `cd betrusted-pac`
2. Generate `lib.rs`, which contains the entirety of the PAC in one file: `svd2rust --target riscv -i ../../build/software/soc.svd`
3. Expand the resulting `lib.rs` into component files: `rm -rf src; form -i lib.rs -o src/; rm lib.rs`
Finally, (optional) you can reformat the crate: `cargo fmt`.  Afterwards, go back to the root directory: `cd ..`

You are now ready to use the crate.

## Creating a "Hello, World!"

```rust
    let mut peripherals = betrusted_pac::Peripherals::take().unwrap();
```

## Notes

Upon recompiling the gateware, the PAC needs to be regenreated by runnig the following in betrusted-pac:

1. Generate `lib.rs`, which contains the entirety of the PAC in one file: `svd2rust --target riscv -i ../../build/software/soc.svd`
2. Expand the resulting `lib.rs` into component files: `rm -rf src; form -i lib.rs -o src/; rm lib.rs`

To build the binary image, do `cargo build` anywhere in the `sw` Rust build subdirectory.

On an initial build from clean, you need to do a cargo build in the "main" directory before trying rebuild the PAC stuff with form.


The resulting ELF is located in sw/target/riscv32i-unknown-none-elf/debug/betrusted-ec

You can inspect this with riscv64-unknown-elf-gdb, and run commands like `disassemble main` and `x _start` to confirm things
like the boot address and the correct compilation of the code.

There is a script that can copy the ELF to a bin and merge it with the gateware located in bin/rust-rom.sh. This puts
a flashable binary file in /tmp/bt-ec.bin, which can be written to the ROM using fomu-flash on the host Raspberry Pi.

You can find docs on how to use the SVD API at https://docs.rs/svd2rust/0.16.1/svd2rust/#peripheral-api

You can build local docs on the gateware's API by going to betrusted-pac and running `cargo doc` and then `cargo doc --open`

`rustup doc` will pull up offline documentation about Rust.

To start visual studio code, just run `code .` in the sw subdirectory.


Using GDB:

there's a script in betrusted-scripts called "start-gdb.sh". It basically does this:
  wishbone-tool --uart /dev/ttyS0 -b 115200 -s gdb --bind-addr 0.0.0.0

This was tested with wishbone tool version 0.4.7. Earlier versions definitely do not work.

Run this on the raspberry pi that is connected to the serial port of betrusted. Be sure
the serial port mux is set to the CPU that you intend to debug.

This allows one to run gdb on a remote machine. Be sure to use the risc version of gdb, eg.:
  riscv64-unknown-elf-gdb

Once connected, do a
  target remote <ip address of pi>:1234
  set riscv use_compressed_breakpoints off
  file <ELF file to debug to pull in symbols>

breakpoints don't work unless you turn off use compressed breakpoints.

Local docs are available with `rustup doc --book` and `rustup doc`

Using vscode, you'll want the Rust(rls) extension, and Better TOML extensions. emacs user may enjoy the "emacs friendly keymap"