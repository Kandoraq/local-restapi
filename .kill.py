#!/usr/bin/env python3
"""Helper lokal: hentikan proses server proyek ini berdasarkan nama binary.

Tanpa argumen: tampilkan daftar proses yang berjalan (dry-run).
Pakai:
  python3 .kill.py              # lihat target
  python3 .kill.py youtubeapi   # matikan youtubeapi saja
  python3 .kill.py all          # matikan semua binary proyek
"""

import os
import sys
import signal

BINARY_NAMES = (
    "dashboard",
    "removewmapi",
    "tiktokapi",
    "removebgapi",
    "youtubeapi",
)


def targets():
    found = []
    for pid in os.listdir("/proc"):
        if not pid.isdigit():
            continue
        try:
            exe = os.readlink(f"/proc/{pid}/exe")
        except (OSError, PermissionError):
            continue
        base = os.path.basename(exe)
        if base in BINARY_NAMES:
            found.append((int(pid), base, exe))
    return sorted(found)


def main():
    args = sys.argv[1:]
    procs = targets()

    if not procs:
        print("tidak ada proses server yang berjalan")
        return

    if not args or args[0] in ("-h", "--help", "list"):
        for pid, name, exe in procs:
            print(f"pid={pid} name={name} exe={exe}")
        print(f"total {len(procs)} proses. Gunakan 'all' atau nama binary untuk menghentikan.")
        return

    requested = args[0]
    if requested == "all":
        selected = procs
    elif requested in BINARY_NAMES:
        selected = [p for p in procs if p[1] == requested]
    else:
        print(f"nama tidak dikenal: {requested}")
        print(f"pilihan: all, {', '.join(BINARY_NAMES)}")
        sys.exit(1)

    if not selected:
        print(f"tidak ada proses untuk '{requested}'")
        return

    for pid, name, _ in selected:
        try:
            os.kill(pid, signal.SIGTERM)
            print(f"SIGTERM pid={pid} name={name}")
        except ProcessLookupError:
            print(f"pid={pid} sudah tidak ada")


if __name__ == "__main__":
    main()
