# the constants used here need to be manually synced with xtask and betrusted_hal::mem_locs.rs
    # populate stack canary
	# form 0xACE0BACE
    #li          a0, 0xACE0BACE # for whatever reason, the assembler is not expanding this correctly
	# so we do it explicitly below
	lui         a0, 0xACE0C
    addi        a0, a0, -1330 # 0xACE

    lui         a1, 0x10013 # 0x1001_3000 code/stack boundary:
    lui         a2, 0x10018 # 0x1001_8000 start of stack area (above this are malloc buffers)
fillstack:
    sw          a0, 0(a1)
    addi        a1, a1, 4
    bltu        a1, a2, fillstack

    # copy ROM code into RAM for XIP
	lui  a1, 0x2001B   		# form address 0x2001_B000: a1 is source
	lui  a2, 0x10000		# form address 0x1000_0000: a2 is dest
	lui  a3, 0x10013	    # end copy,0x1001_3000 (76kiB): a3
loop:
	lw   a4, 0(a1)			# copy source to temp
	sw   a4, 0(a2)          # store in dest
	addi a1, a1, 0x4        # increment source
	addi a2, a2, 0x4        # increment dest
	bne  a2, a3, loop       # repeat if end address not hit

	lui  a2, 0x10000	    # re-form address 0x1000_0000
	jalr x0, a2, 0			# jump to head of SRAM

