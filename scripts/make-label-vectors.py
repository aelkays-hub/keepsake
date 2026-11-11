#!/usr/bin/env python3
"""Rebuild the AI tag vectors (ADR-035) from crates/keepsake-core/src/ai/labels.tsv,
or, with `aircraft-types` as the second argument, the aircraft type vectors
(ADR-041) from crates/keepsake-core/src/ai/aircraft/aircraft-types.tsv.

Developer-only. Needs: pip install onnxruntime numpy tokenizers
and the SigLIP 2 text tower and tokenizer from
onnx-community/siglip2-base-patch16-224-ONNX (revision ba1f3b08):
  onnx/text_model.onnx, tokenizer.json

Usage: python make-label-vectors.py <dir with text_model.onnx and tokenizer.json> [aircraft-types]

Output format (little-endian), read by keepsake_core::ai::labels:
  b"KSLV2\\0\\0\\0"   magic
  u32 n, [n]u8     labels.tsv with \\r removed (a test checks it against the source)
  u32 count, u32 dim
  count x (u32 phrases, phrases x (f32 scale, [dim]i8))   unit vector = i8 * scale
"""
import pathlib
import struct
import sys

import numpy as np
import onnxruntime as ort
from tokenizers import Tokenizer

ROOT = pathlib.Path(__file__).resolve().parent.parent
AI = ROOT / "crates/keepsake-core/src/ai"
LISTS = {
    "labels": (AI / "labels.tsv", AI / "labels-siglip2-b16-224.bin"),
    "aircraft-types": (
        AI / "aircraft/aircraft-types.tsv",
        AI / "aircraft/aircraft-types-siglip2-b16-224.bin",
    ),
}
MAX_LEN = 64
PAD_ID = 0


def parse(text):
    out = []
    for line in text.splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        name, prompts = line.split("\t")[:2]
        out.append((name, [p.strip() for p in prompts.split("|") if p.strip()]))
    return out


def main():
    src = pathlib.Path(sys.argv[1])
    tsv, out_path = LISTS[sys.argv[2] if len(sys.argv) > 2 else "labels"]
    raw = tsv.read_bytes().replace(b"\r", b"")
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
        body += struct.pack("<I", len(out))
        for v in out:
            scale = float(np.abs(v).max()) / 127.0
            q = np.clip(np.round(v / scale), -127, 127).astype(np.int8)
            body += struct.pack("<f", scale) + q.tobytes()
        print(f"{name}: {len(prompts)} prompt(s)")
    dim = 768
    head = b"KSLV2\0\0\0" + struct.pack("<I", len(raw)) + raw + struct.pack("<II", len(labels), dim)
    out_path.write_bytes(head + bytes(body))
    print(f"wrote {out_path} ({len(labels)} labels)")


if __name__ == "__main__":
    main()
