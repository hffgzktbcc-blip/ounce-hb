# SPDX-License-Identifier: MIT
DEVKITPRO ?= /opt/devkitpro
CC := $(DEVKITPRO)/devkitA64/bin/aarch64-none-elf-gcc
ELF2NRO := $(DEVKITPRO)/tools/bin/elf2nro
ARCH := -march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIE
.PHONY: all clean check-toolchain
all: libnx_hello.nro
check-toolchain:
	DEVKITPRO=$(DEVKITPRO) sh ./check-toolchain.sh
libnx_hello.elf: main.c Makefile | check-toolchain
	$(CC) $(ARCH) -O2 -Wall -Wextra -Werror -ffunction-sections -D__SWITCH__ -I$(DEVKITPRO)/libnx/include -specs=$(DEVKITPRO)/libnx/switch.specs -L$(DEVKITPRO)/libnx/lib -Wl,--build-id=none main.c -lnx -o $@
libnx_hello.nro: libnx_hello.elf
	$(ELF2NRO) $< $@
clean:
	rm -f libnx_hello.elf libnx_hello.nro
