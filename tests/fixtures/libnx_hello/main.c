// SPDX-License-Identifier: MIT
// Original clean-room fixture for ounce-hb.
#include <stdio.h>
#include <switch.h>

int main(int argc, char **argv) {
    consoleInit(NULL);
    consoleDebugInit(debugDevice_SVC);
    printf("Hello from ounce-hb!\n");
    fprintf(stderr, "Hello from ounce-hb!\n");
    for (int i = 1; i < argc; ++i) {
        printf("arg[%d]: %s\n", i, argv[i]);
        fprintf(stderr, "arg[%d]: %s\n", i, argv[i]);
    }
    consoleUpdate(NULL);
    consoleExit(NULL);
    return 0;
}
