#!/usr/bin/env python3
"""Rewrites the SideStore source for a released iPhone build.

Called by the release-ios workflow once the IPA exists and has been
attached to the GitHub Release, the way write-cask.sh is for the Mac:
the source has to name the version, the download and its size, and none
of them is known until then.

A source is the JSON an AltStore-style store reads to learn which apps it
offers and where the newest build is. SideStore checks it on its own and
offers the update, so a release reaches the phone without a Mac.

The app's permissions are read out of the IPA rather than written down
here. The store compares the usage descriptions an app declares with the
ones the source announces, and a list kept by hand would drift the first
time someone adds a permission to Info.ios.plist.

Usage: scripts/write-sidestore-source.py <version> <path-to-ipa>
"""

from __future__ import annotations

import json
import plistlib
import sys
import zipfile
from datetime import datetime, timezone
from pathlib import Path

REPO = "huulanka/hippocampus"
RAW = f"https://raw.githubusercontent.com/{REPO}/main"
OUT = Path("sidestore/hippocampus.json")


def info_plist(ipa: Path) -> dict:
    with zipfile.ZipFile(ipa) as archive:
        name = next(
            n for n in archive.namelist() if n.startswith("Payload/") and n.endswith(".app/Info.plist")
        )
        return plistlib.loads(archive.read(name))


def main() -> None:
    if len(sys.argv) != 3:
        sys.exit("usage: write-sidestore-source.py <version> <path-to-ipa>")
    version, ipa = sys.argv[1], Path(sys.argv[2])

    info = info_plist(ipa)
    if info["CFBundleShortVersionString"] != version:
        sys.exit(f"the IPA says {info['CFBundleShortVersionString']}, the release says {version}")

    download = f"https://github.com/{REPO}/releases/download/v{version}/{ipa.name}"
    size = ipa.stat().st_size
    date = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    privacy = {k: v for k, v in sorted(info.items()) if k.endswith("UsageDescription")}

    source = {
        "name": "Hippocampus",
        "identifier": "com.andreasbauer.hippocampus.source",
        "sourceURL": f"{RAW}/{OUT}",
        "iconURL": f"{RAW}/client/src-tauri/icons/icon.png",
        "apps": [
            {
                "name": "Hippocampus",
                "bundleIdentifier": info["CFBundleIdentifier"],
                "developerName": "huulanka",
                "subtitle": "Speak a thought, find it again.",
                "localizedDescription": (
                    "Personal knowledge system fed by voice, structured without losing the original."
                ),
                "iconURL": f"{RAW}/client/src-tauri/icons/icon.png",
                "category": "productivity",
                "versions": [
                    {
                        "version": version,
                        "buildVersion": info["CFBundleVersion"],
                        "date": date,
                        "localizedDescription": f"https://github.com/{REPO}/releases/tag/v{version}",
                        "downloadURL": download,
                        "size": size,
                        "minOSVersion": info["MinimumOSVersion"],
                    }
                ],
                "appPermissions": {"entitlements": [], "privacy": privacy},
                # The fields before versions[] existed. Older SideStore
                # builds read only these.
                "version": version,
                "versionDate": date,
                "downloadURL": download,
                "size": size,
            }
        ],
        "news": [],
    }

    OUT.parent.mkdir(exist_ok=True)
    OUT.write_text(json.dumps(source, indent=2, ensure_ascii=False) + "\n")
    print(f"wrote {OUT} for {version} ({size} bytes)")


if __name__ == "__main__":
    main()
