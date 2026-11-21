# SPDX-License-Identifier: MIT
DEVKITPRO ?= /opt/devkitpro
CC := $(DEVKITPRO)/devkitA64/bin/aarch64-none-elf-gcc
ELF2NSO := $(DEVKITPRO)/tools/bin/elf2nso
ARCH := -march=armv8-a+crc+crypto -mtune=cortex-a57 -mtp=soft -fPIE
.PHONY: all clean check-toolchain
all: libnx_nso_hello.nso
check-toolchain:
	DEVKITPRO=$(DEVKITPRO) sh ./check-toolchain.sh
libnx_nso_hello.elf: main.c Makefile | check-toolchain
	$(CC) $(ARCH) -O2 -Wall -Wextra -Werror -ffunction-sections -nostartfiles -D__SWITCH__ -I$(DEVKITPRO)/libnx/include -L$(DEVKITPRO)/libnx/lib -Wl,-T$(DEVKITPRO)/libnx/switch.ld -Wl,--build-id=none main.c -lnx -o $@
libnx_nso_hello.nso: libnx_nso_hello.elf
	$(ELF2NSO) $< $@
clean:
	rm -f libnx_nso_hello.elf libnx_nso_hello.nso
