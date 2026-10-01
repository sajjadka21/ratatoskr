# SPDX-License-Identifier: GPL-3.0-only
"""Package same-version Android application/build sources and rebuilt WebP source."""
import argparse
import hashlib
import zipfile
from pathlib import Path
from build_android_native import SOURCE_SHA


def package(root, output):
    source = root / 'android/build/native/inputs/libwebp-1.6.0.tar.gz'
    if hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError('WebP source hash mismatch')
    paths = sorted(p for p in (root / 'android').rglob('*') if p.is_file()
                   and not set(p.relative_to(root / 'android').parts) & {'build', '.gradle', '.idea'}
                   and p.name != 'local.properties')
    paths += [root / 'scripts' / name for name in (
        'build_android_native.py', 'verify_android_native.py', 'test_android_native.py', 'package-android-source.py')]
    paths += [root / 'LICENSES.md']
    with zipfile.ZipFile(output, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for file in paths:
            archive.write(file, 'Ratatoskr-android-source/' + file.relative_to(root).as_posix())
        archive.write(source, 'Ratatoskr-android-source/third-party/libwebp-1.6.0.tar.gz')
    print(f'Packaged {len(paths)} Android application/build source files plus verified WebP source')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    package(Path(__file__).resolve().parents[1], args.output)
