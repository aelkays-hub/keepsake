#!/usr/bin/env python3
"""Write the third-party notices shipped with the installer.

Lists every Rust crate built into the Keepsake app (normal and build
dependencies of `keepsake-app`, for Windows x64, Windows ARM64 and macOS
Apple Silicon) and every npm package in the UI's production bundle, with
the licence texts their packages carry. The programs shipped beside the
app (FFmpeg, ExifTool, LibRaw, ONNX Runtime, DirectML) have their own
licence files in the installer; this file points to them.

    python scripts/make-notices.py           # write the file
    python scripts/make-notices.py --check   # fail if it is out of date (CI)

Run it from the repository root after changing Cargo.lock or the UI's
dependencies. It needs cargo and pnpm (`pnpm install` done) and uses only
Python's standard library.
"""

import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "app", "ui", "src-tauri", "licenses", "THIRD-PARTY-NOTICES.txt")
APP_PACKAGE = "keepsake-app"
TARGETS = ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "aarch64-apple-darwin"]
LICENCE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")

# Shipped beside the app, each with its own licence files (bundle resources
# in tauri.windows.conf.json; scripts/setup.ps1 fetches them).
BESIDE = """\
FFmpeg and ffprobe (separate programs, not linked)
  Licence: GNU LGPL 3.0 (BtbN LGPL build; no GPL or non-free parts).
  Text: licenses/FFmpeg-LICENSE.txt. Source: https://ffmpeg.org and
  https://github.com/BtbN/FFmpeg-Builds

ExifTool 13.59 by Phil Harvey (separate program, not linked)
  Licence: free software under the same terms as Perl itself (the Artistic
  License or the GNU GPL). Its Windows build carries Strawberry Perl and
  the Windows launcher (CC0). Texts: exiftool/exiftool_files/LICENSE and
  exiftool/exiftool_files/Licenses_Strawberry_Perl.zip.
  Source: https://exiftool.org and https://strawberryperl.com

LibRaw 0.22.2 (dcraw_emu and libraw.dll; separate program, not linked)
  Licence: GNU LGPL 2.1 or CDDL 1.0. Texts: libraw/LICENSE.LGPL,
  libraw/LICENSE.CDDL and libraw/COPYRIGHT. Source: https://www.libraw.org

ONNX Runtime (loaded at run time, not linked)
  Licence: MIT. Texts: licenses/onnxruntime-LICENSE.txt and
  licenses/onnxruntime-ThirdPartyNotices.txt.
  Source: https://github.com/microsoft/onnxruntime

DirectML (Windows; loaded at run time, not linked)
  Licence: Microsoft DirectML licence (redistributable with an application).
  Text: licenses/DirectML-LICENSE.txt.

AI models are downloaded by the user, not shipped; each one's licence is
listed in Keepsake's Settings where it is downloaded (SigLIP 2, PP-OCRv5,
YOLOX-s and SFace: Apache-2.0; YuNet: MIT).
"""


def run(cmd, cwd):
    return subprocess.run(
        cmd, cwd=cwd, check=True, capture_output=True, shell=(os.name == "nt")
    ).stdout.decode("utf-8")


def read_text(path):
    with open(path, "rb") as f:
        t = f.read().decode("utf-8", errors="replace")
    t = t.replace("\r\n", "\n").replace("\r", "\n").lstrip("﻿")
    return "\n".join(line.rstrip() for line in t.split("\n")).strip("\n")


def licence_files(directory):
    names = []
    for n in os.listdir(directory):
        p = os.path.join(directory, n)
        if os.path.isfile(p) and n.upper().startswith(LICENCE_PREFIXES):
            names.append(n)
    return sorted(names, key=str.upper)


def rust_crates():
    """(name, version, licence, repository, dir) of every crate built into the app."""
    found = {}
    for target in TARGETS:
        m = json.loads(
            run(
                ["cargo", "metadata", "--format-version", "1", "--locked",
                 "--filter-platform", target],
                ROOT,
            )
        )
        pk = {p["id"]: p for p in m["packages"]}
        nodes = {n["id"]: n for n in m["resolve"]["nodes"]}
        members = set(m["workspace_members"])
        roots = [i for i in members if pk[i]["name"] == APP_PACKAGE]
        if not roots:
            sys.exit(f"{APP_PACKAGE} is not in the workspace")
        seen, stack = set(), list(roots)
        while stack:
            i = stack.pop()
            if i in seen:
                continue
            seen.add(i)
            for d in nodes[i]["deps"]:
                if any(k["kind"] in (None, "build") for k in d["dep_kinds"]):
                    stack.append(d["pkg"])
        for i in seen - members:
            p = pk[i]
            key = (p["name"], p["version"])
            found[key] = (
                p["name"],
                p["version"],
                p.get("license") or "see the licence file",
                p.get("repository") or "",
                os.path.dirname(p["manifest_path"]),
                p.get("license_file"),
            )
    return [found[k] for k in sorted(found)]


def npm_packages():
    ui = os.path.join(ROOT, "app", "ui")
    data = json.loads(run(["pnpm", "licenses", "list", "--prod", "--json"], ui))
    out = []
    for licence, pkgs in data.items():
        for p in pkgs:
            for version, path in zip(p["versions"], p["paths"]):
                out.append((p["name"], version, licence, p.get("homepage") or "", path, None))
    return sorted(out)


def texts_of(directory, licence_file):
    names = licence_files(directory) if os.path.isdir(directory) else []
    if licence_file:
        extra = os.path.normpath(os.path.join(directory, licence_file))
        if os.path.isfile(extra) and os.path.basename(extra) not in names:
            names.append(os.path.relpath(extra, directory))
    return [read_text(os.path.join(directory, n)) for n in names]


MIT_TEXT = """\
MIT License

Copyright (c) the authors of {name} (see its repository)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE."""

BSD3_TEXT = """\
BSD 3-Clause License

Copyright (c) the authors of {name} (see its repository)

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

3. Neither the name of the copyright holder nor the names of its
   contributors may be used to endorse or promote products derived from
   this software without specific prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE."""


def build():
    crates = rust_crates()
    npm = npm_packages()
    groups = {}  # text hash -> (text, [users])
    order = []

    def keep(t, label):
        h = hashlib.sha256(t.encode("utf-8")).hexdigest()[:12]
        if h not in groups:
            groups[h] = (t, [])
            order.append(h)
        groups[h][1].append(label)
        return f"[{h}]"

    # Pass 1: the texts the packages carry.
    found = {}
    for items in (crates, npm):
        for name, version, _licence, _repo, directory, licence_file in items:
            label = f"{name} {version}"
            found[label] = [keep(t, label) for t in texts_of(directory, licence_file) if t]

    def standard(start):
        """A full text another package carries, for one that carries none."""
        for h in order:
            if groups[h][0].lstrip().startswith(start):
                return h
        return None

    apache = standard("Apache License")
    mpl = standard("Mozilla Public License Version 2.0")

    # Pass 2: a package with no licence file gets the standard text of the
    # licence it declares (said so), never nothing.
    def fallback(name, label, licence):
        if "Apache-2.0" in licence and apache:
            groups[apache][1].append(label)
            return f"[{apache}]"
        if "MPL-2.0" in licence and mpl:
            groups[mpl][1].append(label)
            return f"[{mpl}]"
        if "MIT" in licence:
            return keep(MIT_TEXT.format(name=name), label)
        if "BSD-3-Clause" in licence:
            return keep(BSD3_TEXT.format(name=name), label)
        return None

    def add(kind, items):
        lines = []
        for name, version, licence, repo, _directory, _lf in items:
            label = f"{name} {version}"
            refs = found[label]
            if refs:
                note = " ".join(refs)
            else:
                ref = fallback(name, label, licence)
                note = (
                    f"{ref} (standard text of its licence; the package carries no licence file)"
                    if ref
                    else "(the package carries no licence file; see its repository)"
                )
            where = f"  {repo}" if repo else ""
            lines.append(f"{label}  —  {licence}{where}\n    Licence text: {note}")
        return f"{kind} ({len(items)})\n\n" + "\n".join(lines)

    parts = [
        "THIRD-PARTY NOTICES FOR KEEPSAKE\n"
        "\n"
        "Keepsake includes the software listed below, made by others, under the\n"
        "licences given. Generated by scripts/make-notices.py; do not edit by hand.",
        "1. PROGRAMS SHIPPED BESIDE KEEPSAKE\n\n" + BESIDE.rstrip("\n"),
        "2. " + add("RUST CRATES BUILT INTO KEEPSAKE", crates),
        "3. " + add("JAVASCRIPT PACKAGES IN KEEPSAKE'S WINDOW", npm),
    ]
    texts = ["4. LICENCE TEXTS\n\nEach text is followed by the packages it comes with."]
    for h in sorted(order, key=lambda h: (sorted(groups[h][1])[0], h)):
        t, users = groups[h]
        texts.append(
            f"[{h}] used by: {', '.join(sorted(set(users)))}\n\n{t}"
        )
    rule = "\n\n" + "=" * 76 + "\n\n"
    sub = "\n\n" + "-" * 76 + "\n\n"
    return rule.join(parts) + rule + sub.join(texts) + "\n"


def main():
    text = build()
    if "--check" in sys.argv:
        try:
            current = read_text(OUT) + "\n"
        except FileNotFoundError:
            current = ""
        if current != text:
            sys.exit(
                "THIRD-PARTY-NOTICES.txt is out of date: run "
                "`python scripts/make-notices.py` and commit the result."
            )
        print("THIRD-PARTY-NOTICES.txt is up to date.")
        return
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    print(f"Wrote {os.path.relpath(OUT, ROOT)} ({len(text.encode('utf-8')):,} bytes).")


if __name__ == "__main__":
    main()
