extern crate cc;

use std::env::var;
use std::env::set_var;

fn main() {
    set_var("CC", "riscv-none-elf-gcc");  // set the compiler to what's installed on the system

	let mut base_config = cc::Build::new();

    base_config.include("wfx-fullMAC-driver/wfx_fmac_driver/");
    base_config.include("wfx-fullMAC-driver/wfx_fmac_driver/firmware");
    base_config.include("wfx-fullMAC-driver/wfx_fmac_driver/firmware/3.3.1");
	base_config.file("wfx-fullMAC-driver/wfx_fmac_driver/sl_wfx.c");
	base_config.file("wfx-fullMAC-driver/wfx_fmac_driver/bus/sl_wfx_bus.c");
    base_config.file("wfx-fullMAC-driver/wfx_fmac_driver/bus/sl_wfx_bus_spi.c");

	base_config.compile("libwfx.a");
}

pub fn link(name: &str, bundled: bool) {
    let target = var("TARGET").unwrap();
    let target: Vec<_> = target.split('-').collect();
    if target.get(2) == Some(&"windows") {
        println!("cargo:rustc-link-lib=dylib={}", name);
        if bundled && target.get(3) == Some(&"gnu") {
            let dir = var("CARGO_MANIFEST_DIR").unwrap();
            println!("cargo:rustc-link-search=native={}/{}", dir, target[0]);
        }
    } else {
		println!("cargo:rustc-link-lib=dylib={}", name);
	}
}

pub fn link_framework(name: &str) {
	println!("cargo:rustc-link-lib=framework={}", name);
}
