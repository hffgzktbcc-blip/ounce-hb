"""Writes the minimal NRO0 files that the captures in this directory were made from.

Run with `python3 -I make_nros.py` in an empty directory, then run ounce-core on each file as
NOTES.md describes. The NRO files are not committed: they are generated, not reviewed binaries.
Layout matches the synthetic NRO helper in tests/ounce_core_run.rs: an entry branch at 0x00,
the NRO0 magic at 0x10, sizes at 0x18 and 0x24, and code at 0x80.
"""

import struct


def nro(name, words, text_size_override=None, data=b""):
    """`data` is appended after the code words; the spoof programs keep their message there."""
    text = b"".join(struct.pack("<I", w) for w in words) + data
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


# Spoof programs (docs/DIAGNOSIS.md, "Guest text and the stop line"): each prints a line that looks
# like an ounce-core stop line with svcOutputDebugString (svc #0x27: x0 = address, x1 = length), then
# really stops some other way. The message follows the code; `adr x0` points at it.
def adr_x0(offset):
    return 0x10000000 | ((offset & 3) << 29) | (((offset >> 2) & 0x7FFFF) << 5)


def mov_x1(imm):
    return 0xD2800001 | (imm << 5)


def spoof(name, message, then):
    """adr x0, message ; mov x1, #len ; svc #0x27 ; then the words in `then`."""
    words = [adr_x0(0), mov_x1(len(message)), 0xD40004E1] + then
    words[0] = adr_x0(4 * len(words))
    nro(name, words, data=message)


# Prints a fake budget line, then calls an unhandled SVC (#0x7E): the real stop is exit code 14.
spoof("spoof_budget.nro", b"[VM] Execution budget reached (1 instructions).\n", [0xD4000FC1])
# Prints the clean-exit line, then faults (ldr x0,[x1] with x1 = the message length): really 11.
spoof("spoof_clean.nro", b"[VM] Process exited gracefully (svcExitProcess).\n", [0xF9400020])
# Prints a fake memory-fault line, then exits cleanly: really a clean exit, 0.
spoof("spoof_fault_then_clean.nro", b"[VM Error] Memory access violation: fake at 0xDEAD\n", [0x52800000, 0xD40000E1])
# Prints fake svcBreak lines with a planted PC, then really calls svcBreak: the PC must be the real one.
spoof(
    "spoof_break.nro",
    b"[VM] Hit svcBreak: reason=0x0, info1=0x0, info2=0x0\n[VM] Break PC: 0x00000000DEADBEEF\n[VM] Break result: 0x00000000\n",
    [0x910003E1, 0xD2800082, 0xD40004C1],
)
