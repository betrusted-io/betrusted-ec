use std::io::Write;
use std::path::PathBuf;
use std::{env, fs};

fn main() {
    // Put the linker script somewhere the linker can find it
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    println!("cargo:rustc-link-search={}", out_dir.display());

    fs::File::create(out_dir.join("memory.x"))
        .unwrap()
        .write_all(include_bytes!("memory.x"))
        .unwrap();
    println!("cargo:rerun-if-changed=memory.x");

    // build FFI bindings for the wfx driver
    // this doesn't work -- bindgen can't run cross-environment. I can run bindgen for x86,
    // but attempting to run this for riscv causes an error. This note is left for historical
    // documentation of why we use a command-line monkey patch to generate the bindings.
/*
    // TODO - check all dependent files in wfx.h
    println!("cargo:rerun-if-changed=imports/wfx.h");

    let bindings = bindgen::Builder::default()
        .header("imports/wfx.h")
        .clang_arg("-Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver")
        .clang_arg("-Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/secure_links")
        .clang_arg("-Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/bus")
        .clang_arg("-Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware")
        .clang_arg("-Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware/3.3.1")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindout
        .write_to_file(out_path.join("wfx_bindings.rs"))
        .expect("Couldn't write wfx_bindings.rs!");
    */
    // this works: bindgen imports/wfx.h -- -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/secure_links -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/bus -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware/3.3.1
}

