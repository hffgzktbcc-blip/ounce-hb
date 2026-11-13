#!/bin/sh
# SPDX-License-Identifier: MIT
set -eu
: "${DEVKITPRO:=/opt/devkitpro}"
for package in devkitA64-r30-1 devkita64-gcc-16.1.0-1 devkita64-binutils-2.46.0-1 devkita64-newlib-4.6.0.20260123-4 libnx-4.12.0-1 switch-tools-1.13.1-1; do
    test -f "$DEVKITPRO/pacman/var/lib/pacman/local/$package/desc" || {
        echo "Missing pinned devkitPro package: $package" >&2
        exit 1
    }
done
test "$("$DEVKITPRO/devkitA64/bin/aarch64-none-elf-gcc" -dumpfullversion)" = 16.1.0
