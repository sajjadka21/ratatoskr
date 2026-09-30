import io
import struct
import unittest
import zipfile
from verify_android_native import elf_errors, scan_archive


def elf(alignment=16384, offset=0, address=0, machine=183):
    data = bytearray(120)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<H", data, 18, machine)
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HH", data, 54, 56, 1)
    struct.pack_into("<IIQQQQQQ", data, 64, 1, 5, offset, address, 0, 0, 0, alignment)
    return bytes(data)


def zipped(name, content):
    out = io.BytesIO()
    with zipfile.ZipFile(out, "w") as archive:
        archive.writestr(name, content)
    return out.getvalue()


class NativeCompatibilityTests(unittest.TestCase):
    def test_arm_and_x86_aligned(self):
        for machine in (183, 62):
            self.assertEqual(elf_errors(elf(machine=machine), "native"), [])

    def test_reject_four_k_and_fake_alignment(self):
        self.assertTrue(elf_errors(elf(4096), "native"))
        self.assertTrue(elf_errors(elf(16384, 4096, 0), "native"))

    def test_malformed_program_headers(self):
        self.assertTrue(elf_errors(elf()[:100], "native"))

    def test_scan_nested_zip_so(self):
        data = zipped("lib/arm64-v8a/libffmpeg.zip.so", zipped("usr/lib/libwebp.so", elf(4096)))
        errors, count = scan_archive(data)
        self.assertEqual(count, 1)
        self.assertEqual(len(errors), 1)
        self.assertIn("libffmpeg.zip.so!usr/lib/libwebp.so", errors[0])

    def test_regular_data_is_not_native(self):
        self.assertEqual(scan_archive(zipped("assets/info.txt", b"hello")), ([], 0))


if __name__ == "__main__":
    unittest.main()
