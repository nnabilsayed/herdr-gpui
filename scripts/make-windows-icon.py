"""Builds assets/icons/herdr-windows.ico and .res from the app's PNG icon.

Usage: python scripts/make-windows-icon.py assets/icons/herdr-ui-icon-clean.png assets/icons/herdr-windows.ico assets/icons/herdr-windows.res

The .res holds RT_GROUP_ICON id 1 and its RT_ICON frames; GPUI's Windows
platform loads icon resource 1 from the executable for the window and taskbar.
"""
import io
import struct
import sys

from PIL import Image

src, out_ico, out_res = sys.argv[1:4]
sizes = [16, 24, 32, 48, 64, 128, 256]
base = Image.open(src).convert("RGBA")

frames = []
for size in sizes:
    buf = io.BytesIO()
    base.resize((size, size), Image.LANCZOS).save(buf, format="PNG")
    frames.append((size, buf.getvalue()))

# .ico
header = struct.pack("<HHH", 0, 1, len(frames))
entries = b""
offset = 6 + 16 * len(frames)
for size, data in frames:
    dim = 0 if size >= 256 else size
    entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset)
    offset += len(data)
with open(out_ico, "wb") as f:
    f.write(header + entries + b"".join(d for _, d in frames))


# .res
def pad4(b):
    return b + b"\0" * (-len(b) % 4)


def resource(rtype, rid, data):
    # DataSize, HeaderSize, TYPE, NAME, DataVersion, MemoryFlags, Lang, Version, Characteristics
    head = struct.pack("<IIHHHHIHHII", len(data), 32, 0xFFFF, rtype, 0xFFFF, rid, 0, 0x1030, 0, 0, 0)
    return pad4(head + data)


res = struct.pack("<IIHHHHIHHII", 0, 32, 0xFFFF, 0, 0xFFFF, 0, 0, 0, 0, 0, 0)
group = struct.pack("<HHH", 0, 1, len(frames))
for index, (size, data) in enumerate(frames, start=1):
    dim = 0 if size >= 256 else size
    group += struct.pack("<BBBBHHIH", dim, dim, 0, 0, 1, 32, len(data), index)
    res += resource(3, index, data)  # RT_ICON
res += resource(14, 1, group)  # RT_GROUP_ICON, name 1
with open(out_res, "wb") as f:
    f.write(res)
print("wrote", out_ico, out_res, len(res))
