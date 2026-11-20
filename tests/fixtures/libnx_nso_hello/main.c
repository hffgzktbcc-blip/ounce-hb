// SPDX-License-Identifier: MIT
// Original clean-room NSO fixture for ounce-hb using libnx SVCs.
#include <switch.h>

void _start(void) __attribute__((section(".text.jmp"), noreturn));

void _start(void) {
    static const char msg[] = "Hello NSO from ounce-hb!\n";
    svcOutputDebugString(msg, sizeof(msg) - 1);
    svcExitProcess();
    __builtin_unreachable();
}
