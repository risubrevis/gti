#!/usr/bin/env python3
"""
Convert any GIF into per-frame ASCII art text files + a metadata file
that the Cargo build script (build.rs) auto-discovers.

Usage:
    python3 convert.py <gif_path> <output_dir> [--cols N]

Example:
    python3 scripts/convert.py assets/dance1/dance.gif assets/dance1/
    python3 scripts/convert.py assets/dance2/dance.gif assets/dance2/ --cols 60

Auto-detected from the GIF:
    • Frame count   (ffprobe nb_frames)
    • Frame rate     (ffprobe r_frame_rate, e.g. "25/2" → 12.5 FPS)
    • Aspect ratio  → ROWS = round(height × COLS / width / 2)
      (terminal chars are ~2:1 so we halve the row count)

Generated output:
    frame_001.txt … frame_NNN.txt   — ASCII art per frame
    dance.json                     — metadata for build.rs:
        { "name": "dance1", "fps_ms": 80, "cols": 70, "rows": 45, "frames": 38 }

To add a new dance to the project:
    1.  mkdir assets/dance3
    2.  cp new.gif assets/dance3/dance.gif
    3.  python3 scripts/convert.py assets/dance3/dance.gif assets/dance3/
    4.  cargo build   ← build.rs auto-discovers assets/dance3/dance.json
"""

import argparse
import json
import os
import subprocess
import sys

# ── ASCII brightness ramp ─────────────────────────────────────────────
# Dark (space) → bright (dense chars), inverted so that bright GIF
# pixels become spaces (invisible on a dark terminal) and dark pixels
# become dense characters (the visible figure).
RAMP = " .'`^\",:;Il!i><~+_-?][}{1)(|/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"


def brightness_to_char(b: int) -> str:
    """Map 0-255 brightness to an ASCII character (inverted)."""
    b_inv = 255 - b
    idx = min(int(b_inv * len(RAMP) / 256), len(RAMP) - 1)
    return RAMP[idx]


# ── ffprobe helpers ────────────────────────────────────────────────────


def probe_int(gif_path: str, field: str) -> int:
    """Query a single integer stream field from a GIF via ffprobe."""
    result = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            f"stream={field}",
            "-of",
            "csv=p=0",
            gif_path,
        ],
        capture_output=True,
        text=True,
    )
    return int(result.stdout.strip())


def probe_fps(gif_path: str) -> float:
    """Return the frame rate as a float (parses fractions like '25/2')."""
    result = subprocess.run(
        [
            "ffprobe",
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=r_frame_rate",
            "-of",
            "csv=p=0",
            gif_path,
        ],
        capture_output=True,
        text=True,
    )
    raw = result.stdout.strip()  # e.g. "25/2" or "10"
    if "/" in raw:
        num, den = raw.split("/", 1)
        return float(num) / float(den)
    return float(raw)


# ── Main conversion ────────────────────────────────────────────────────


def convert(gif_path: str, out_dir: str, cols: int) -> dict:
    """Convert a GIF to ASCII frames + metadata.  Returns the metadata dict."""
    if not os.path.isfile(gif_path):
        print(f"Error: {gif_path} not found", file=sys.stderr)
        sys.exit(1)

    # ── Auto-detect GIF properties ──────────────────────────────────
    width = probe_int(gif_path, "width")
    height = probe_int(gif_path, "height")
    nb_frames = probe_int(gif_path, "nb_frames")
    fps = probe_fps(gif_path)

    # Calculate rows accounting for ~2:1 character aspect ratio
    rows = max(1, round(height * cols / width / 2))
    fps_ms = round(1000 / fps) if fps > 0 else 100

    # Derive dance name from the parent directory name (e.g. "dance1")
    dance_name = os.path.basename(os.path.normpath(out_dir))

    print(f"GIF      : {gif_path}")
    print(f"Size     : {width}×{height} px  →  {cols}×{rows} chars")
    print(f"Frames   : {nb_frames}")
    print(f"FPS      : {fps:.2f}  ({fps_ms} ms/frame)")
    print(f"Output   : {out_dir}/")

    # ── Extract all frames as raw grayscale ────────────────────────
    result = subprocess.run(
        [
            "ffmpeg",
            "-i",
            gif_path,
            "-vf",
            f"scale={cols}:{rows}",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "pipe:1",
        ],
        capture_output=True,
    )

    frame_size = cols * rows
    total = len(result.stdout)
    if total < frame_size:
        print(f"Error: got {total} bytes, need at least {frame_size}", file=sys.stderr)
        sys.exit(1)

    extracted = total // frame_size
    if extracted < nb_frames:
        print(
            f"Warning: expected {nb_frames} frames, extracted {extracted}",
            file=sys.stderr,
        )
    nb_frames = min(nb_frames, extracted)

    # ── Convert each frame to ASCII ────────────────────────────────
    os.makedirs(out_dir, exist_ok=True)

    # Remove any stale frame files from a previous run
    for fname in os.listdir(out_dir):
        if fname.startswith("frame_") and fname.endswith(".txt"):
            os.remove(os.path.join(out_dir, fname))

    print(f"Converting {nb_frames} frames …")

    for i in range(nb_frames):
        offset = i * frame_size
        frame_data = result.stdout[offset : offset + frame_size]

        lines = []
        for row in range(rows):
            start = row * cols
            line = "".join(
                brightness_to_char(frame_data[start + col]) for col in range(cols)
            ).rstrip()
            lines.append(line)

        while lines and not lines[-1].strip():
            lines.pop()

        fname = f"frame_{i + 1:03d}.txt"
        with open(os.path.join(out_dir, fname), "w") as f:
            f.write("\n".join(lines) + "\n")

    # ── Write metadata for build.rs ────────────────────────────────
    meta = {
        "name": dance_name,
        "fps_ms": fps_ms,
        "cols": cols,
        "rows": rows,
        "frames": nb_frames,
    }
    meta_path = os.path.join(out_dir, "dance.json")
    with open(meta_path, "w") as f:
        json.dump(meta, f, indent=2)
        f.write("\n")

    print(f"Done — {nb_frames} frames + dance.json → {out_dir}/")
    return meta


def main():
    parser = argparse.ArgumentParser(
        description="Convert a GIF into ASCII art frame files + metadata."
    )
    parser.add_argument("gif", help="Path to the source GIF file")
    parser.add_argument("outdir", help="Output directory (e.g. assets/dance1/)")
    parser.add_argument(
        "--cols",
        type=int,
        default=70,
        help="ASCII art width in columns (default: 70). "
        "Rows are calculated automatically from the GIF aspect ratio.",
    )
    args = parser.parse_args()

    convert(args.gif, args.outdir, args.cols)


if __name__ == "__main__":
    main()
