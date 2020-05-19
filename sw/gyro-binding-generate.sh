#!/bin/sh

echo "This script generates the wfx Rust bindings from reference .h files provided by the vendor."
echo "Note that bindgen is invoked in the x86_64 environment, so technically the bindings are for a different target."
echo "However, bindgen does not seem to currently run with riscv as a target, so this is the best we can do for now."
echo "This command should be re-run anytime the imports/wfx submodules are updated, and all structures"
echo "should be reviewed with the caveat that the data types are sized for x86_64, and not riscv; however, in"
echo "theory, for the structures involved here these should be a match."

echo "\nAlso, note that bindgen can't be run in a build.rs because bindgen does not work in cross compilation."
echo "Instead we specify an i386-pc-linux-gnu target because this is the closest arch with headers installed we can target."

# --no-derive-debug is because #[derive]` can't be used on a `#[repr(packed)]` struct that does not derive Copy (error E0133)
# a future version of bindgen might be a little more targeted in the derive, but for now, don't derive any debug to avoid this error

BINDING_FILE=gyro_bindings/src/lib.rs

echo "#![no_std]" > $BINDING_FILE
echo "#![allow(nonstandard_style)]" >> $BINDING_FILE
echo "extern crate c_types;" >> $BINDING_FILE

bindgen --no-derive-debug --ctypes-prefix c_types --use-core --blacklist-type FP_NAN --blacklist-type FP_INFINITE --blacklist-type FP_ZERO --blacklist-type FP_SUBNORMAL --blacklist-type FP_NORMAL --blacklist-item FP_NAN --blacklist-item FP_INFINITE --blacklist-item FP_ZERO --blacklist-item FP_SUBNORMAL --blacklist-item FP_NORMAL gyro_sys/STMems_Standard_C_drivers/lsm6ds3_STdC/driver/lsm6ds3_reg.h -- --target=i386-pc-linux-gnu >> $BINDING_FILE

