"""Writes a DOL file, the GameCube's executable, from a big-endian 32-bit PowerPC ELF file.

Usage: python3 dol.py program.elf program.dol

A DOL is a header of 0x100 bytes and the sections' bytes after it: seven places for sections of
code and eleven for data, each an offset in the file, an address and a size (three tables of
eighteen words), then the address and the size of what the program clears, and its entry. Each
segment the ELF file loads becomes a section, code when it is executable, with what it has in
the file; the first segment's bytes that are the ELF file's own headers go with it, as the
linker laid them out. What a segment has in memory past its bytes is the cleared part.
"""

import struct
import sys


def main(source, target):
    data = open(source, "rb").read()
    assert data[:6] == b"\x7fELF\x01\x02", "a big-endian 32-bit ELF file"
    entry, table = struct.unpack_from(">II", data, 24)
    size, count = struct.unpack_from(">HH", data, 42)
    code, plain, cleared = [], [], (0, 0)
    for n in range(count):
        kind, offset, address, _, in_file, in_memory, flags, _ = struct.unpack_from(">8I", data, table + n * size)
        if kind != 1 or in_memory == 0:
            continue
        if in_file:
            (code if flags & 1 else plain).append((address, data[offset:offset + in_file]))
        if in_memory > in_file:
            cleared = (address + in_file, in_memory - in_file)
    assert len(code) <= 7 and len(plain) <= 11, "more sections than a DOL has places for"
    slots = code + [None] * (7 - len(code)) + plain + [None] * (11 - len(plain))
    offsets, addresses, sizes, body = [], [], [], b""
    for slot in slots:
        if slot is None:
            offsets.append(0), addresses.append(0), sizes.append(0)
            continue
        address, content = slot
        # a section's size is a multiple of four, as the console's loader copies words
        content += b"\0" * (-len(content) % 4)
        offsets.append(0x100 + len(body)), addresses.append(address), sizes.append(len(content))
        body += content
    header = struct.pack(">18I18I18I", *offsets, *addresses, *sizes) + struct.pack(">III", cleared[0], cleared[1], entry)
    open(target, "wb").write(header.ljust(0x100, b"\0") + body)
    print(f"{target}: {len(code)} sections of code, {len(plain)} of data, entry {entry:#x}")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
