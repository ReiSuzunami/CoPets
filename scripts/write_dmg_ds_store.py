#!/usr/bin/env python3
from __future__ import annotations

import os
import stat
import sys
from pathlib import Path

from ds_store import DSStore
from mac_alias import Alias


def main() -> int:
    if len(sys.argv) != 3:
        print(
            "usage: write_dmg_ds_store.py <mounted-volume> <installer-app-name>",
            file=sys.stderr,
        )
        return 2

    raw_volume = Path(sys.argv[1])
    installer_name = sys.argv[2]
    if (
        not installer_name
        or installer_name in {".", ".."}
        or Path(installer_name).name != installer_name
        or Path(installer_name).is_absolute()
    ):
        print(f"unsafe installer app name: {installer_name!r}", file=sys.stderr)
        return 1
    try:
        volume_stat = os.lstat(raw_volume)
    except OSError as error:
        print(f"could not inspect mounted volume {raw_volume}: {error}", file=sys.stderr)
        return 1
    if not stat.S_ISDIR(volume_stat.st_mode) or stat.S_ISLNK(volume_stat.st_mode):
        print(f"unsafe mounted volume: {raw_volume}", file=sys.stderr)
        return 1
    volume = raw_volume.resolve(strict=True)
    installer = volume / installer_name
    background_directory = volume / ".background"
    background = volume / ".background" / "background.png"
    try:
        installer_stat = os.lstat(installer)
        background_directory_stat = os.lstat(background_directory)
        background_stat = os.lstat(background)
    except OSError as error:
        print(f"could not inspect DMG layout inputs: {error}", file=sys.stderr)
        return 1
    if not stat.S_ISDIR(installer_stat.st_mode) or stat.S_ISLNK(installer_stat.st_mode):
        print(f"unsafe or missing installer app: {installer}", file=sys.stderr)
        return 1
    if not stat.S_ISDIR(background_directory_stat.st_mode) or stat.S_ISLNK(
        background_directory_stat.st_mode
    ):
        print(f"unsafe DMG background directory: {background_directory}", file=sys.stderr)
        return 1
    if not stat.S_ISREG(background_stat.st_mode) or stat.S_ISLNK(background_stat.st_mode):
        print(f"unsafe or missing DMG background: {background}", file=sys.stderr)
        return 1

    store_path = volume / ".DS_Store"
    background_alias = Alias.for_file(str(background)).to_bytes()
    with DSStore.open(str(store_path), "w+") as store:
        store["."]["bwsp"] = {
            "ContainerShowSidebar": False,
            "PreviewPaneVisibility": False,
            "ShowPathbar": False,
            "ShowSidebar": False,
            "ShowStatusBar": False,
            "ShowTabView": False,
            "ShowToolbar": False,
            "WindowBounds": "{{120, 120}, {720, 440}}",
        }
        store["."]["icvp"] = {
            "arrangeBy": "none",
            "backgroundColorBlue": 0.89,
            "backgroundColorGreen": 0.96,
            "backgroundColorRed": 0.98,
            "backgroundImageAlias": background_alias,
            "backgroundType": 2,
            "gridOffsetX": 0.0,
            "gridOffsetY": 0.0,
            "gridSpacing": 100.0,
            "iconSize": 112.0,
            "labelOnBottom": True,
            "showIconPreview": True,
            "showItemInfo": False,
            "textSize": 13.0,
            "viewOptionsVersion": 1,
        }
        store[installer_name]["Iloc"] = (360, 288)
        store["."]["vSrn"] = ("long", 1)

    if store_path.stat().st_size == 0:
        print(f"failed to create Finder metadata: {store_path}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
