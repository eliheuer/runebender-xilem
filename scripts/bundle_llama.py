#!/usr/bin/env python3
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT
"""Put a pinned llama.cpp release's llama-server into an app bundle.

The editor looks for `llama-server` in `Contents/Resources/llama/` before
`RUNEBENDER_LLAMA_SERVER` and PATH (see `find_binary` in
`src/workflows/llama_server.rs`), so a DMG that carries it needs no
Homebrew on the user's machine. The release is pinned by build number and
SHA-256, so every DMG of a version runs the same server.

Call it from the packaging script after the bundle's Resources folder
exists and before the app is signed:

    bundle_llama(resources, target, identity=args.identity)

or from the shell:

    scripts/bundle_llama.py --resources path/to/Runebender.app/Contents/Resources

The files are signed here, one by one, before the packager signs the app:
codesign does not reach code under Resources on its own, and a signed app
whose nested code is unsigned fails verification. Pass the packager's
identity so a notarized build gets the hardened runtime and a timestamp on
these binaries too; without one they are ad-hoc signed, as a local build is.

To move to a newer llama.cpp: pick a build from
https://github.com/ggml-org/llama.cpp/releases, download both macOS
tarballs, and put their build number and SHA-256 sums in RELEASE below.
"""

import argparse
import hashlib
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
from pathlib import Path

RELEASE = {
    "build": "b11756",
    "sha256": {
        "aarch64-apple-darwin": "f86b5eb838b985a17587e794dee9c6b34193aca6dceec018031a4ba7951fb7e6",
        "x86_64-apple-darwin": "6d71968268a01ae975acde18441a70b2c7ba128b7a127f46b5ced4e8c5dd4eca",
    },
}
ASSET = {
    "aarch64-apple-darwin": "macos-arm64",
    "x86_64-apple-darwin": "macos-x64",
}


def rpath_closure(folder: Path, start: str) -> set[str]:
    """The names of every `@rpath/` library `start` loads, directly or through another."""
    found: set[str] = set()
    pending = [start]
    while pending:
        name = pending.pop()
        listing = subprocess.run(
            ["otool", "-L", folder / name], capture_output=True, text=True, check=True
        ).stdout
        for line in listing.splitlines()[1:]:
            library = line.strip().split(" (", 1)[0]
            if not library.startswith("@rpath/"):
                continue
            library = library.removeprefix("@rpath/")
            if library not in found:
                if not (folder / library).exists():
                    raise RuntimeError(f"{name} needs {library}, which the release lacks")
                found.add(library)
                pending.append(library)
    return found


def download(target: str, cache: Path) -> Path:
    """The release tarball for `target`, fetched into `cache` unless it is there and intact."""
    build = RELEASE["build"]
    name = f"llama-{build}-bin-{ASSET[target]}.tar.gz"
    path = cache / name
    expected = RELEASE["sha256"][target]
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        cache.mkdir(parents=True, exist_ok=True)
        url = f"https://github.com/ggml-org/llama.cpp/releases/download/{build}/{name}"
        print(f"Downloading {url}")
        with urllib.request.urlopen(url, timeout=120) as response, path.open("wb") as file:
            shutil.copyfileobj(response, file)
        actual = hashlib.sha256(path.read_bytes()).hexdigest()
        if actual != expected:
            path.unlink()
            raise RuntimeError(f"{name}: SHA-256 {actual} is not the pinned {expected}")
    return path


def sign(path: Path, identity: str | None) -> None:
    """Sign one binary: ad hoc without an identity, hardened and timestamped with one."""
    command = ["codesign", "--force", "--sign", identity or "-"]
    if identity:
        command += ["--options", "runtime", "--timestamp"]
    subprocess.run([*command, path], check=True)


def bundle_llama(
    resources: Path, target: str, cache: Path | None = None, identity: str | None = None
) -> Path:
    """Copy llama-server and its libraries into `resources/llama/`, signed; returns the binary."""
    if target not in ASSET:
        raise RuntimeError(f"No llama.cpp release pinned for {target}")
    cache = cache or Path.home() / "Library" / "Caches" / "runebender" / "llama"
    tarball = download(target, cache)
    destination = resources / "llama"
    if destination.exists():
        shutil.rmtree(destination)
    destination.mkdir(parents=True)
    with tempfile.TemporaryDirectory() as scratch:
        with tarfile.open(tarball) as archive:
            archive.extractall(scratch, filter="data")
        (source,) = Path(scratch).iterdir()
        if not (source / "llama-server").is_file():
            raise RuntimeError(f"{tarball.name} has no llama-server")
        shutil.copy2(source / "llama-server", destination / "llama-server")
        shutil.copy2(source / "LICENSE", destination / "LICENSE")
        # The libraries the binary loads, by the names it loads them under,
        # found by following its @rpath links and theirs. The release's
        # symlinks between versioned and plain names are resolved into
        # plain files, since nested symlinks confuse code signing.
        for name in sorted(rpath_closure(source, "llama-server")):
            shutil.copy2(source / name, destination / name, follow_symlinks=True)
    binary = destination / "llama-server"
    binary.chmod(0o755)
    # Libraries first, then the binary that loads them: signing inside out.
    for path in sorted(destination.glob("lib*.dylib")):
        if not path.is_symlink():
            sign(path, identity)
    sign(binary, identity)
    version = subprocess.run(
        [binary, "--version"], capture_output=True, text=True, check=True
    )
    print((version.stdout + version.stderr).strip().splitlines()[0])
    return binary


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--resources", type=Path, required=True, help="the bundle's Resources folder")
    parser.add_argument(
        "--target", default="aarch64-apple-darwin", choices=sorted(ASSET), help="Rust target triple"
    )
    parser.add_argument("--cache", type=Path, help="where downloaded tarballs are kept")
    parser.add_argument("--identity", help="Developer ID Application identity; ad hoc without")
    args = parser.parse_args()
    binary = bundle_llama(args.resources.resolve(), args.target, args.cache, args.identity)
    print(f"Bundled {binary}")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.CalledProcessError) as error:
        print(f"bundle_llama: {error}", file=sys.stderr)
        sys.exit(1)
