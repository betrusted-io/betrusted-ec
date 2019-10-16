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
6. Install `cargo-generate` which will let you use project templates: `cargo install cargo-generate`
7. (Optional) install `cargo-binutils`, which will let you do things like `cargo size` and `cargo objdump`: `cargo install cargo-binutils`

## Creating a Cargo crate

Create a Cargo crate by starting from the build template:

cargo generate --git https://github.com/betrusted-io/betrusted-rs-quickstart.git

Chang

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