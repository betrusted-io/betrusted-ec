#!/bin/sh

python3 imports/wfx-fullMAC-tools/Tools/pds_compress bt-wf200-pds.h > wfx_rs/src/bt-wf200-pds.in
# add a trailing null to the file
truncate -s +1 wfx_rs/src/bt-wf200-pds.in 

