"""Writes the minimal NRO0 files that the captures in this directory were made from.

Run with `python3 -I make_nros.py` in an empty directory, then run ounce-core on each file as
NOTES.md describes. The NRO files are not committed: they are generated, not reviewed binaries.
Layout matches the synthetic NRO helper in tests/ounce_core_run.rs: an entry branch at 0x00,
the NRO0 magic at 0x10, sizes at 0x18 and 0x24, and code at 0x80.
"""

import struct


def nro(name, words, text_size_override=None):
    text = b"".join(struct.pack("<I", w) for w in words)
    total = 0x80 + len(text)
    b = bytearray(total)
    struct.pack_into("<I", b, 0x00, 0x14000020)  # b #0x80
    b[0x10:0x14] = b"NRO0"
    struct.pack_into("<I", b, 0x18, total)
    struct.pack_into("<I", b, 0x24, total)
    b[0x80:] = text
    if text_size_override is not None:
        struct.pack_into("<I", b, 0x24, text_size_override)
    with open(name, "wb") as f:
        f.write(bytes(b))


nro("clean.nro", [0x52800000, 0xD40000E1])  # mov w0,#0 ; svc #7 (svcExitProcess)
nro("unimpl.nro", [0x00000000])  # undefined word
nro("memfault.nro", [0xF9400020])  # ldr x0,[x1]  (x1 is not mapped)
nro("budget.nro", [0x14000000])  # b .  (captured with OUNCE_NRO_BUDGET=1000)
nro("svc_break.nro", [0x910003E1, 0xD2800082, 0xD40004C1])  # add x1,sp,#0 ; mov x2,#4 ; svc #0x26 (svcBreak)
nro("unhandled_svc.nro", [0xD4000FC1])  # svc #0x7E
nro("loader_error.nro", [0x52800000, 0xD40000E1], text_size_override=0x10000)  # text size past end of file
# "usage" has no NRO: it is ounce-core's output for `run-nro` with no path.
