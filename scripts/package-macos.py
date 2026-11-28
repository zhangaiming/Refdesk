"""Package the built Apple Silicon release into an ad-hoc-signed app ZIP."""

import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import re
import shutil
import struct
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parent.parent
TARGET = "aarch64-apple-darwin"


def deployment_version(binary):
    arch = subprocess.check_output(["xcrun", "lipo", "-archs", binary], text=True)
    if arch.strip() != "arm64":
        raise ValueError(f"需要单架构 arm64 可执行文件，实际架构为 {arch.strip()!r}")
    build = subprocess.check_output(["xcrun", "vtool", "-show-build", binary], text=True)
    platform = re.search(r"(?m)^\s*platform\s+(\S+)", build)
    minimum = re.search(r"(?m)^\s*minos\s+(\d+(?:\.\d+){1,2})\s*$", build)
    if not platform or platform[1].upper() != "MACOS" or not minimum:
        raise ValueError("无法读取 macOS 可执行文件的最低系统版本")
    return minimum[1]


def make_app(app, binary, version, minimum):
    contents = app / "Contents"
    executable = contents / "MacOS/refdesk"
    executable.parent.mkdir(parents=True)
    shutil.copyfile(binary, executable)
    executable.chmod(0o755)

    resources = contents / "Resources"
    resources.mkdir()
    icon = (ROOT / "assets/refdesk.png").read_bytes()
    if icon[:8] != b"\x89PNG\r\n\x1a\n" or icon[16:24] != struct.pack(">II", 256, 256):
        raise ValueError("应用图标需要 256 × 256 的 PNG")
    header = struct.pack(">4sI4sI", b"icns", len(icon) + 16, b"ic08", len(icon) + 8)
    (resources / "Refdesk.icns").write_bytes(header + icon)

    bundle_version = re.split(r"[+-]", version, maxsplit=1)[0]
    info = {
        "CFBundleDevelopmentRegion": "zh_CN",
        "CFBundleDisplayName": "Refdesk",
        "CFBundleExecutable": executable.name,
        "CFBundleIconFile": "Refdesk.icns",
        "CFBundleIdentifier": "io.github.zhangaiming.refdesk",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleName": "Refdesk",
        "CFBundlePackageType": "APPL",
        "CFBundleShortVersionString": bundle_version,
        "CFBundleVersion": bundle_version,
        "LSMinimumSystemVersion": minimum,
    }
    with (contents / "Info.plist").open("wb") as plist:
        plistlib.dump(info, plist)


def package(output, tag):
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"],
        cwd=ROOT,
    ))
    version = next(item["version"] for item in metadata["packages"] if item["name"] == "refdesk")
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"Git 标签 {tag!r} 必须与 Cargo 版本标签 v{version} 一致")

    binary = Path(metadata["target_directory"]) / TARGET / "release/refdesk"
    if not binary.is_file():
        raise ValueError(f"未找到 {binary}，请先运行 cargo build --release --locked --target {TARGET}")
    minimum = deployment_version(binary)
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".refdesk-macos-", dir=output) as staging:
        app = Path(staging) / "Refdesk.app"
        make_app(app, binary, version, minimum)
        subprocess.run(["codesign", "--force", "--sign", "-", app], check=True)
        subprocess.run(["codesign", "--verify", "--strict", app], check=True)
        zip_file = Path(staging) / f"refdesk-{version}-macos-arm64.zip"
        subprocess.run(["ditto", "-c", "-k", "--sequesterRsrc", "--keepParent", app, zip_file], check=True)
        checksum = zip_file.with_suffix(".zip.sha256")
        digest = hashlib.sha256(zip_file.read_bytes()).hexdigest()
        checksum.write_text(f"{digest}  {zip_file.name}\n", encoding="ascii")
        for artifact in (zip_file, checksum):
            print(artifact.replace(output / artifact.name))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "artifacts")
    parser.add_argument("--tag", help="与 Cargo 版本一致的标签，例如 v0.2.0")
    args = parser.parse_args()
    try:
        package(args.output.resolve(), args.tag)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"{error}\n")
