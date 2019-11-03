#!/usr/bin/env python3

import os

seed = 10

for attempt in range(0, 20):
    print(" =============> Trying with seed " + str(seed))
    ret = os.system('./betrusted-ec.py --seed=' + str(seed))
    if ret == 0:
        break
    seed = seed + 1

