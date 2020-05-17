extern crate cc;

use std::env::var;
use std::env::set_var;

fn main() {
    set_var("CC", "riscv64-unknown-elf-gcc");  // set the compiler to what's installed on the system

    let mut base_config = cc::Build::new();

    base_config.include("STMems_Standard_C_drivers/lsm6ds3_STdC/driver/");
    base_config.file("STMems_Standard_C_drivers/lsm6ds3_STdC/driver/lsm6ds3_reg.c");

	base_config.compile("libgyro.a");
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
