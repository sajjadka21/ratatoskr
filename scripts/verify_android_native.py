# SPDX-License-Identifier: GPL-3.0-only
"""Check every ELF, including executables hidden in nested .zip.so payloads."""
import argparse
import io
import struct
import zipfile
from pathlib import Path

PAGE_SIZE = 16384


def elf_errors(data, name):
    if not data.startswith(b"\x7fELF"):
        return []
    if len(data) < 64 or data[4] not in (1, 2) or data[5] not in (1, 2):
        return [f"{name}: malformed ELF header"]
    endian = "<" if data[5] == 1 else ">"
    wide = data[4] == 2
    machine = struct.unpack_from(endian + "H", data, 18)[0]
    # Android 16 KB pages apply to its 64-bit ARM and x86 architectures.
    if not wide:
        return []
    if machine not in (62, 183):
        return [f"{name}: unexpected 64-bit ELF machine {machine}"]
    phoff = struct.unpack_from(endian + "Q", data, 32)[0]
    size, count = struct.unpack_from(endian + "HH", data, 54)
    if size < 56 or count == 0 or phoff + size * count > len(data):
        return [f"{name}: malformed program header table"]
    errors = []
    loads = 0
    for index in range(count):
        pos = phoff + size * index
        if struct.unpack_from(endian + "I", data, pos)[0] != 1:
            continue
        loads += 1
        offset, address = struct.unpack_from(endian + "QQ", data, pos + 8)
        alignment = struct.unpack_from(endian + "Q", data, pos + 48)[0]
        if alignment < PAGE_SIZE or alignment & (alignment - 1):
            errors.append(f"{name}: PT_LOAD[{index}] alignment {alignment} < 16 KB or invalid")
        elif (address - offset) % alignment:
            errors.append(f"{name}: PT_LOAD[{index}] offset/address are not congruent")
    if not loads:
        errors.append(f"{name}: no PT_LOAD segments")
    return errors


def scan_archive(data, label="archive", depth=0):
    if depth > 5:
        raise ValueError("Native archive nesting exceeds safety limit")
    errors, count = [], 0
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        for item in archive.infolist():
            if item.is_dir():
                continue
            if item.file_size > 512 * 1024 * 1024:
                raise ValueError(f"Oversized archive entry: {item.filename}")
            content = archive.read(item)
            name = f"{label}!{item.filename}"
            if content.startswith(b"\x7fELF"):
                count += 1
                errors.extend(elf_errors(content, name))
            elif content.startswith(b"PK\x03\x04"):
                child_errors, child_count = scan_archive(content, name, depth + 1)
                errors.extend(child_errors)
                count += child_count
    return errors, count


def verify(path):
    errors, count = scan_archive(Path(path).read_bytes(), str(path))
    if not count:
        raise ValueError("No native ELF binaries found")
    if errors:
        raise ValueError("\n".join(errors))
    print(f"Verified {count} native ELF binaries, including nested payloads: 64-bit PT_LOAD segments support 16 KB pages")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    verify(parser.parse_args().archive)
