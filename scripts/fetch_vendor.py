#!/usr/bin/env python3
"""
Fetch and verify pinned third-party runtime binaries (LGPL FFmpeg + PDFium)
with strict SHA-256 checksum verification and FFmpeg LGPL build-flag checks.
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import os
import platform
import shutil
import stat
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path

PDFIUM_TAG = "chromium/8179"
PDFIUM_ASSETS = {
    "linux-x64": (
        f"https://github.com/bblanchon/pdfium-binaries/releases/download/{PDFIUM_TAG}/pdfium-linux-x64.tgz",
        "b4766a6bf2856efbcc6c3cc7d6ae9022bdc6bb36ddc1acdd92d1f1873f773f4b",
    ),
    "linux-arm64": (
        f"https://github.com/bblanchon/pdfium-binaries/releases/download/{PDFIUM_TAG}/pdfium-linux-arm64.tgz",
        "18bb6d568257b28d14f6c3b8bfb48caf73d4615b319ca4d43b424c2a03431d9f",
    ),
    "macos-arm64": (
        f"https://github.com/bblanchon/pdfium-binaries/releases/download/{PDFIUM_TAG}/pdfium-mac-arm64.tgz",
        "44b7c6f55dd33454d0909728bd0adf6c86a8e141b36f07a6b6f50e228e1dfb46",
    ),
    "macos-x64": (
        f"https://github.com/bblanchon/pdfium-binaries/releases/download/{PDFIUM_TAG}/pdfium-mac-x64.tgz",
        "b29bbfe37efcad5b443fa3556c0485ec96fc1992d5b19d84df49d48fbed1d0d2",
    ),
    "windows-x64": (
        f"https://github.com/bblanchon/pdfium-binaries/releases/download/{PDFIUM_TAG}/pdfium-win-x64.tgz",
        "836a77c9053f624839c66010cddea7b675628a36b8831d2330178bb169d241b6",
    ),
    "windows-arm64": (
        f"https://github.com/bblanchon/pdfium-binaries/releases/download/{PDFIUM_TAG}/pdfium-win-arm64.tgz",
        "9c659bf0cd9bdefcbf7ec0e4b1675c2961e0fdfd4ce097b411efd9742832fe06",
    ),
}

FFMPEG_BUILDS_TAG = "autobuild-2026-11-26-12-56"
FFMPEG_ASSETS = {
    "linux-x64": (
        f"https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/{FFMPEG_BUILDS_TAG}/ffmpeg-n8.1.3-41-ga8029b6528-linux64-lgpl-shared-8.1.tar.xz",
        "a8b42082934142810b98d18b3d9c36b012fc55e75dfafff61f81a139af3a399d",
        "tar.xz",
    ),
    "linux-arm64": (
        f"https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/{FFMPEG_BUILDS_TAG}/ffmpeg-n8.1.3-41-ga8029b6528-linuxarm64-lgpl-shared-8.1.tar.xz",
        "1e6de523bc0be231284c1c5a88da1c941080a52c02960121f8ecc8025e42eb8d",
        "tar.xz",
    ),
    "windows-x64": (
        f"https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/{FFMPEG_BUILDS_TAG}/ffmpeg-n8.1.3-41-ga8029b6528-win64-lgpl-shared-8.1.zip",
        "372016b1248544b4ffc290ae41c9c57e6441bd0da32068ea18032a25b7b36e08",
        "zip",
    ),
    "windows-arm64": (
        f"https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/{FFMPEG_BUILDS_TAG}/ffmpeg-n8.1.3-41-ga8029b6528-winarm64-lgpl-shared-8.1.zip",
        "e89857ba29f13d2d422d814ddc2e46b2ab89c9e3a837530a9dd16b6c242b6b02",
        "zip",
    ),
    "macos-arm64": (
        "https://github.com/genesis-webagentur-org/ffmpeg-lgpl-macos/releases/download/9.0.2-1/ffmpeg-darwin-arm64.gz",
        "c9ff6614a678ef0540d63c9d0ff252c44736c90ce9473d45df232c88ebb3927d",
        "gz",
    ),
    "macos-x64": (
        "https://github.com/genesis-webagentur-org/ffmpeg-lgpl-macos/releases/download/9.0.2-1/ffmpeg-darwin-x86_64.gz",
        "f72ebc3b2488fcb8d77a9dad3f0eac824b80fb84b830b4e07f57c701ab7114dc",
        "gz",
    ),
}


def detect_host_target() -> str:
    sys_name = platform.system().lower()
    machine = platform.machine().lower()
    arch = "arm64" if machine in ("aarch64", "arm64") else "x64"
    if sys_name == "linux":
        return f"linux-{arch}"
    if sys_name == "darwin":
        return f"macos-{arch}"
    if sys_name == "windows":
        return f"windows-{arch}"
    raise RuntimeError(f"Unsupported host platform: {sys_name}/{machine}")


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def download_verified(url: str, expected_sha256: str, dest: Path) -> None:
    print(f"Downloading {url} ...")
    req = urllib.request.Request(url, headers={"User-Agent": "tool-center-fetch/0.1"})
    with urllib.request.urlopen(req, timeout=180) as resp, dest.open("wb") as out:
        shutil.copyfileobj(resp, out)
    actual = sha256_file(dest)
    if actual.lower() != expected_sha256.lower():
        dest.unlink(missing_ok=True)
        raise RuntimeError(
            f"SHA-256 mismatch for {url}!\n  expected: {expected_sha256}\n  actual:   {actual}"
        )
    print(f"Verified SHA-256: {actual}")


def make_executable(path: Path) -> None:
    if os.name != "nt" and path.exists():
        path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def fetch_pdfium(target: str, out_dir: Path) -> None:
    url, sha = PDFIUM_ASSETS[target]
    pdfium_dir = out_dir / "pdfium"
    pdfium_dir.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        archive = Path(tmp) / "pdfium.tgz"
        download_verified(url, sha, archive)
        with tarfile.open(archive, "r:gz") as tf:
            tf.extractall(pdfium_dir, filter="data")
    print(f"Extracted PDFium ({PDFIUM_TAG}) -> {pdfium_dir}")


def verify_ffmpeg_lgpl(ffmpeg_bin: Path, target: str) -> None:
    host = detect_host_target()
    # Only execute binary license verification when runnable on current host OS/arch
    can_run = (target == host) or (
        host.startswith("macos-") and target.startswith("macos-")
    )
    if not can_run:
        print(f"Skipping runtime license execution check for cross-target {target} on {host}")
        return

    env = os.environ.copy()
    lib_dir = ffmpeg_bin.parent.parent / "lib"
    if lib_dir.exists():
        env["LD_LIBRARY_PATH"] = f"{lib_dir}:{env.get('LD_LIBRARY_PATH', '')}"

    out = subprocess.check_output(
        [str(ffmpeg_bin), "-L"], stderr=subprocess.STDOUT, env=env, text=True
    )
    lower = out.lower()
    if "--enable-gpl" in lower or "--enable-nonfree" in lower:
        raise RuntimeError(f"FFmpeg binary failed LGPL verification:\n{out}")
    if "lesser general public license" not in lower and "lgpl" not in lower:
        raise RuntimeError(f"Unexpected FFmpeg license output:\n{out}")
    print("Verified FFmpeg binary is strictly LGPL (no --enable-gpl / --enable-nonfree).")


def fetch_ffmpeg(target: str, out_dir: Path) -> None:
    url, sha, kind = FFMPEG_ASSETS[target]
    ffmpeg_dir = out_dir / "ffmpeg"
    ffmpeg_dir.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        archive = tmp_path / f"ffmpeg.{kind}"
        download_verified(url, sha, archive)

        if kind == "tar.xz":
            extract_root = tmp_path / "extracted"
            with tarfile.open(archive, "r:xz") as tf:
                tf.extractall(extract_root, filter="data")
            subdirs = [p for p in extract_root.iterdir() if p.is_dir()]
            src_root = subdirs[0] if len(subdirs) == 1 else extract_root
            for item in src_root.iterdir():
                dest = ffmpeg_dir / item.name
                if dest.exists():
                    shutil.rmtree(dest) if dest.is_dir() else dest.unlink()
                shutil.move(str(item), str(dest))
            ffmpeg_bin = ffmpeg_dir / "bin" / "ffmpeg"
            make_executable(ffmpeg_bin)
            make_executable(ffmpeg_dir / "bin" / "ffprobe")
        elif kind == "zip":
            extract_root = tmp_path / "extracted"
            with zipfile.ZipFile(archive, "r") as zf:
                zf.extractall(extract_root)
            subdirs = [p for p in extract_root.iterdir() if p.is_dir()]
            src_root = subdirs[0] if len(subdirs) == 1 else extract_root
            for item in src_root.iterdir():
                dest = ffmpeg_dir / item.name
                if dest.exists():
                    shutil.rmtree(dest) if dest.is_dir() else dest.unlink()
                shutil.move(str(item), str(dest))
            ffmpeg_bin = ffmpeg_dir / "bin" / "ffmpeg.exe"
        elif kind == "gz":
            bin_dir = ffmpeg_dir / "bin"
            bin_dir.mkdir(parents=True, exist_ok=True)
            ffmpeg_bin = bin_dir / "ffmpeg"
            with gzip.open(archive, "rb") as gz_in, ffmpeg_bin.open("wb") as f_out:
                shutil.copyfileobj(gz_in, f_out)
            make_executable(ffmpeg_bin)
        else:
            raise ValueError(f"Unknown archive kind: {kind}")

    verify_ffmpeg_lgpl(ffmpeg_bin, target)
    print(f"Extracted LGPL FFmpeg -> {ffmpeg_dir}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Fetch pinned PDFium & LGPL FFmpeg binaries.")
    parser.add_argument(
        "--target",
        default=detect_host_target(),
        choices=[
            "linux-x64",
            "linux-arm64",
            "macos-arm64",
            "macos-x64",
            "macos-universal",
            "windows-x64",
            "windows-arm64",
        ],
        help="Target OS/arch to fetch (default: current host)",
    )
    parser.add_argument(
        "--out-dir",
        default=None,
        help="Destination directory (default: <repo>/vendor/<target>)",
    )
    parser.add_argument(
        "--component",
        default="all",
        choices=["all", "ffmpeg", "pdfium"],
        help="Which component(s) to fetch",
    )
    args = parser.parse_args()

    repo_root = Path(__file__).resolve().parent.parent
    if args.target == "macos-universal":
        arm_dir = repo_root / "vendor" / "macos-arm64"
        x64_dir = repo_root / "vendor" / "macos-x64"
        uni_dir = Path(args.out_dir) if args.out_dir else (repo_root / "vendor" / "macos-universal")
        for sub_target, sub_dir in [("macos-arm64", arm_dir), ("macos-x64", x64_dir)]:
            if args.component in ("all", "pdfium"):
                fetch_pdfium(sub_target, sub_dir)
            if args.component in ("all", "ffmpeg"):
                fetch_ffmpeg(sub_target, sub_dir)
        uni_dir.mkdir(parents=True, exist_ok=True)
        if shutil.which("lipo"):
            if args.component in ("all", "ffmpeg"):
                (uni_dir / "ffmpeg" / "bin").mkdir(parents=True, exist_ok=True)
                subprocess.check_call(
                    [
                        "lipo",
                        "-create",
                        str(arm_dir / "ffmpeg" / "bin" / "ffmpeg"),
                        str(x64_dir / "ffmpeg" / "bin" / "ffmpeg"),
                        "-output",
                        str(uni_dir / "ffmpeg" / "bin" / "ffmpeg"),
                    ]
                )
                make_executable(uni_dir / "ffmpeg" / "bin" / "ffmpeg")
            if args.component in ("all", "pdfium"):
                (uni_dir / "pdfium" / "lib").mkdir(parents=True, exist_ok=True)
                subprocess.check_call(
                    [
                        "lipo",
                        "-create",
                        str(arm_dir / "pdfium" / "lib" / "libpdfium.dylib"),
                        str(x64_dir / "pdfium" / "lib" / "libpdfium.dylib"),
                        "-output",
                        str(uni_dir / "pdfium" / "lib" / "libpdfium.dylib"),
                    ]
                )
        return 0

    out_dir = Path(args.out_dir) if args.out_dir else (repo_root / "vendor" / args.target)
    out_dir.mkdir(parents=True, exist_ok=True)
    if args.component in ("all", "pdfium"):
        fetch_pdfium(args.target, out_dir)
    if args.component in ("all", "ffmpeg"):
        fetch_ffmpeg(args.target, out_dir)
    return 0


if __name__ == "__main__":
    sys.exit(main())
