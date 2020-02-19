#!/bin/bash

# argument 1 is the target for copy

if [ -z "$1" ]
then
    echo "Usage: $0 ssh-target [privatekey]"
    echo "ssh-target is missing."
    echo "Assumes betrusted-scripts repo is cloned on repository at ~/code/betrused-scripts/"
fi

# case of no private key specified
if [ -z "$2" ]
then
cargo build --release && ./rust-rom.sh && scp /tmp/bt-ec.bin $1:code/betrusted-scripts/
else
# there is a private key
cargo build --release && ./rust-rom.sh && scp -i $2 /tmp/bt-ec.bin $1:code/betrusted-scripts/
fi
