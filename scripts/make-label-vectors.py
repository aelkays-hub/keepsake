#!/usr/bin/env python3
"""Rebuild the AI tag vectors (ADR-035) from crates/keepsake-core/src/ai/labels.tsv.

Developer-only. Needs: pip install onnxruntime numpy tokenizers
and the SigLIP 2 text tower and tokenizer from
onnx-community/siglip2-base-patch16-224-ONNX (revision ba1f3b08):
  onnx/text_model.onnx, tokenizer.json

Usage: python make-label-vectors.py <dir with text_model.onnx and tokenizer.json>

Output format (little-endian), read by keepsake_core::ai::labels:
  b"KSLV1\\0\\0\\0"   magic
  u32 n, [n]u8     labels.tsv with \\r removed (a test checks it against the source)
  u32 count, u32 dim
  count x (f32 scale, [dim]i8)   unit vector = i8 * scale
"""
import pathlib
import struct
import sys

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

ROOT = pathlib.Path(__file__).resolve().parent.parent
TSV = ROOT / "crates/keepsake-core/src/ai/labels.tsv"
OUT = ROOT / "crates/keepsake-core/src/ai/labels-siglip2-b16-224.bin"
MAX_LEN = 64
PAD_ID = 0


def parse(text):
    out = []
    for line in text.splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        name, prompts = line.split("\t")
        out.append((name, [p.strip() for p in prompts.split("|") if p.strip()]))
    return out


def main():
    src = pathlib.Path(sys.argv[1])
    raw = TSV.read_bytes().replace(b"\r", b"")
    labels = parse(raw.decode("utf-8"))
    tok = Tokenizer.from_file(str(src / "tokenizer.json"))
    sess = ort.InferenceSession(str(src / "text_model.onnx"), providers=["CPUExecutionProvider"])
    body = bytearray()
    for name, prompts in labels:
        ids = []
        for p in prompts:
            e = tok.encode(f"a photo of {p}.".lower()).ids[:MAX_LEN]
            ids.append(e + [PAD_ID] * (MAX_LEN - len(e)))
        out = sess.run(["pooler_output"], {"input_ids": np.array(ids, dtype=np.int64)})[0]
        out = out / np.linalg.norm(out, axis=1, keepdims=True)
        v = out.mean(axis=0)
        v = v / np.linalg.norm(v)
        scale = float(np.abs(v).max()) / 127.0
        q = np.clip(np.round(v / scale), -127, 127).astype(np.int8)
        body += struct.pack("<f", scale) + q.tobytes()
        print(f"{name}: {len(prompts)} prompt(s)")
    dim = 768
    head = b"KSLV1\0\0\0" + struct.pack("<I", len(raw)) + raw + struct.pack("<II", len(labels), dim)
    OUT.write_bytes(head + bytes(body))
    print(f"wrote {OUT} ({len(labels)} labels)")


if __name__ == "__main__":
    main()
