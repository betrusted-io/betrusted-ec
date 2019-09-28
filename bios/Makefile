BETRUSTED_EC_DIR=../..

include $(BETRUSTED_EC_DIR)/software/include/generated/variables.mak
include $(SOC_DIRECTORY)/software/common.mak

OBJECTS=main.o isr.o boot-helper-$(CPU).o 

# prepend our local files to override system ones
CFLAGS := -I. $(CFLAGS) -fdata-sections -ffunction-sections

all: bios.bin

# pull in dependency info for *existing* .o files
-include $(OBJECTS:.o=.d)

%.bin: %.elf
	$(OBJCOPY) -O binary $< $@
ifneq ($(OS),Windows_NT)
	chmod -x $@
endif
ifeq ($(CPUENDIANNESS),little)
	$(PYTHON) -m litex.soc.software.mkmscimg $@ --little
else
	$(PYTHON) -m litex.soc.software.mkmscimg $@
endif

bios.elf: ../libbase/crt0-$(CPU)-ctr.o ../libbase/libbase-nofloat.a ../libcompiler_rt/libcompiler_rt.a $(BIOS_DIRECTORY)/linker.ld $(OBJECTS)
	$(LD) $(LDFLAGS) -T $(BIOS_DIRECTORY)/linker.ld -N -o $@ \
		../libbase/crt0-$(CPU)-ctr.o \
		$(OBJECTS) \
		-L../libbase \
		-L../libcompiler_rt \
		-lbase-nofloat -lcompiler_rt
ifneq ($(OS),Windows_NT)
	chmod -x $@
endif

# pull in dependency info for *existing* .o files
-include $(OBJECTS:.o=.d)

%.o: $(BIOS_DIRECTORY)/%.c
	$(compile)

%.o: $(BIOS_DIRECTORY)/%.S
	$(assemble)

clean:
	$(RM) $(OBJECTS) bios.elf bios.bin .*~ *~

.PHONY: all clean
