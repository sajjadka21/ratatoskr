"""Rebuild the five WebP libraries in the official FFmpeg AAR for 16 KB Android.

Inputs are pinned by SHA-256. No third-party binary fork or ELF header patching.
The original FFmpeg/Python binaries remain supplied by youtubedl-android.
"""
import argparse
import hashlib
import io
import os
import re
import shutil
import subprocess
import tarfile
import urllib.request
import zipfile
from pathlib import Path
from verify_android_native import verify, elf_errors

NDK = "27.2.12479018"
CMAKE = "3.22.1"
VERSION = "0.18.1"
SOURCE_URL = "https://storage.googleapis.com/downloads.webmproject.org/releases/webp/libwebp-1.6.0.tar.gz"
SOURCE_SHA = "e4ab7009bf0629fd11982d4c2aa83964cf244cffba7347ecd39019a9e38c4564"
AAR_URL = f"https://repo.maven.apache.org/maven2/io/github/junkfood02/youtubedl-android/ffmpeg/{VERSION}/ffmpeg-{VERSION}.aar"
AAR_SHA = "0a87ffa6cf912b0fe76c1a99b9107f543ee2f247935fae2c71f0822eb7bc5f49"
ABIS = ("arm64-v8a", "armeabi-v7a", "x86_64")
LIBS = ("libsharpyuv.so", "libwebp.so", "libwebpdecoder.so", "libwebpdemux.so", "libwebpmux.so")


def download(url, path, expected):
    if not path.exists() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        temporary = path.with_suffix(path.suffix + ".download")
        with urllib.request.urlopen(url, timeout=120) as response, temporary.open("wb") as output:
            shutil.copyfileobj(response, output)
        if hashlib.sha256(temporary.read_bytes()).hexdigest() != expected:
            raise ValueError(f"Source checksum mismatch: {url}")
        temporary.replace(path)


def run(*arguments):
    subprocess.run([str(arg) for arg in arguments], check=True)


def dynamic_info(readelf, file):
    output = subprocess.check_output([str(readelf), "-d", str(file)], text=True)
    soname = re.search(r"\(SONAME\).*?\[(.*?)\]", output)
    if not soname:
        raise ValueError(f"Missing SONAME: {file}")
    symbols = subprocess.check_output([str(readelf), "--dyn-syms", "--wide", str(file)], text=True)
    exports = set()
    for line in symbols.splitlines():
        fields = line.split()
        if len(fields) >= 8 and fields[4] in ("GLOBAL", "WEAK") and fields[6] != "UND":
            name = fields[7].split("@")[0]
            if name.startswith(("WebP", "SharpYuv")):
                exports.add(name)
    return soname.group(1), exports


def rebuild(sdk, output):
    output.parent.mkdir(parents=True, exist_ok=True)
    cache = output.parent / "inputs"
    cache.mkdir(exist_ok=True)
    source_tar, original_aar = cache / "libwebp-1.6.0.tar.gz", cache / "ffmpeg-0.18.1.aar"
    download(SOURCE_URL, source_tar, SOURCE_SHA)
    download(AAR_URL, original_aar, AAR_SHA)
    source = cache / "libwebp-1.6.0"
    if not source.exists():
        with tarfile.open(source_tar) as archive:
            archive.extractall(cache, filter="data")
    ndk = sdk / "ndk" / NDK
    suffix = ".exe" if os.name == "nt" else ""
    cmake = sdk / "cmake" / CMAKE / "bin" / f"cmake{suffix}"
    host = "windows-x86_64" if os.name == "nt" else "linux-x86_64"
    bin_dir = ndk / "toolchains" / "llvm" / "prebuilt" / host / "bin"
    readelf, strip = bin_dir / f"llvm-readelf{suffix}", bin_dir / f"llvm-strip{suffix}"
    for tool in (cmake, readelf, strip):
        if not tool.is_file():
            raise ValueError(f"Missing {tool}. Install SDK packages ndk;{NDK} and cmake;{CMAKE}")
    replacements = {}
    with zipfile.ZipFile(original_aar) as aar:
        for abi in ABIS:
            build = output.parent / f"webp-{abi}"
            flags = [f"-DWEBP_BUILD_{name}=OFF" for name in (
                "CWEBP", "DWEBP", "GIF2WEBP", "IMG2WEBP", "VWEBP", "WEBPINFO",
                "ANIM_UTILS", "EXTRAS", "WEBPMUX", "FUZZTEST")]
            run(cmake, "-S", source, "-B", build, "-G", "Ninja",
                f"-DCMAKE_MAKE_PROGRAM={sdk / 'cmake' / CMAKE / 'bin' / ('ninja' + suffix)}",
                f"-DCMAKE_TOOLCHAIN_FILE={ndk / 'build' / 'cmake' / 'android.toolchain.cmake'}",
                f"-DANDROID_ABI={abi}", "-DANDROID_PLATFORM=android-29",
                "-DCMAKE_BUILD_TYPE=Release", "-DBUILD_SHARED_LIBS=ON",
                "-DCMAKE_SHARED_LINKER_FLAGS=-Wl,-z,max-page-size=16384",
                "-DWEBP_BUILD_LIBWEBPMUX=ON", *flags)
            run(cmake, "--build", build, "--parallel", str(min(os.cpu_count() or 2, 4)))
            payload_name = f"jni/{abi}/libffmpeg.zip.so"
            payload = io.BytesIO()
            with zipfile.ZipFile(io.BytesIO(aar.read(payload_name))) as original, zipfile.ZipFile(payload, "w") as patched:
                changed = set()
                for item in original.infolist():
                    data = original.read(item)
                    name = Path(item.filename).name
                    if item.filename == f"usr/lib/{name}" and name in LIBS:
                        candidates = list(build.rglob(name))
                        if len(candidates) != 1:
                            raise ValueError(f"Expected one rebuilt {abi}/{name}, got {candidates}")
                        new_file = candidates[0]
                        run(strip, "--strip-unneeded", new_file)
                        old_file = build / f"original-{name}"
                        old_file.write_bytes(data)
                        old_soname, old_exports = dynamic_info(readelf, old_file)
                        new_soname, new_exports = dynamic_info(readelf, new_file)
                        if old_soname != name or new_soname != name or not old_exports or old_exports - new_exports:
                            raise ValueError(f"WebP ABI mismatch {abi}/{name}: {old_exports - new_exports}")
                        data = new_file.read_bytes()
                        if elf_errors(data, f"{abi}/{name}"):
                            raise ValueError(f"Rebuilt library is not aligned: {abi}/{name}")
                        changed.add(name)
                    patched.writestr(item, data)
                if changed != set(LIBS):
                    raise ValueError(f"Incomplete WebP replacement for {abi}: {changed}")
            replacements[payload_name] = payload.getvalue()
        temporary = output.with_suffix(".temporary.aar")
        with zipfile.ZipFile(temporary, "w") as patched:
            for item in aar.infolist():
                if item.filename.startswith("jni/") and item.filename.split("/")[1] not in ABIS:
                    continue
                patched.writestr(item, replacements.get(item.filename, aar.read(item)))
            for name in ("COPYING", "PATENTS", "AUTHORS"):
                patched.writestr(f"assets/third-party/libwebp-1.6.0/{name}", (source / name).read_bytes())
            patched.writestr("assets/third-party/libwebp-1.6.0/SOURCE.txt",
                f"{SOURCE_URL}\nSHA256 {SOURCE_SHA}\nRebuilt with Android NDK {NDK}, 16 KB linker alignment.\n")
    verify(temporary)
    temporary.replace(output)
    print(f"Prepared official FFmpeg {VERSION} with rebuilt WebP 1.6.0 for {', '.join(ABIS)}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sdk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    rebuild(args.sdk.resolve(), args.output.resolve())
