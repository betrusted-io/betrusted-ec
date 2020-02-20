#!/bin/sh

echo "This script generates the wfx Rust bindings from reference .h files provided by the vendor."
echo "Note that bindgen is invoked in the x86_64 environment, so technically the bindings are for a different target."
echo "However, bindgen does not seem to currently run with riscv as a target, so this is the best we can do for now."
echo "This command should be re-run anytime the imports/wfx submodules are updated, and all structures"
echo "should be reviewed with the caveat that the data types are sized for x86_64, and not riscv; however, in"
echo "theory, for the structures involved here these should be a match."

bindgen --ctypes-prefix c_types --use-core  imports/sl_status_bindgen.h -- -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/secure_links -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/bus -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware/3.3.1 > betrusted-hal/src/api_wf200/wfx_bindings.rs

bindgen --ctypes-prefix c_types --use-core  imports/wfx.h -- -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/secure_links -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/bus -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware -Iimports/wfx-fullMAC-tools/wfx-fullMAC-driver/wfx_fmac_driver/firmware/3.3.1 >> betrusted-hal/src/api_wf200/wfx_bindings.rs


