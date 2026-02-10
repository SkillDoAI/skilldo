---

name: pillow
description: Python imaging library for opening, manipulating, and saving many image file formats.
version: 11.1.0
ecosystem: python
license: MIT-CMU
generated_with: gpt-5.2
---

## Imports

```python
import PIL
from PIL import Image, ImageDraw, ImageFont, ImageColor, ImageOps, ImageChops
from PIL import UnidentifiedImageError
from PIL.PngImagePlugin import PngInfo
```

## Core Patterns

### Open, inspect, convert, and save images ✅ Current
```python
from __future__ import annotations

from pathlib import Path
from PIL import Image


def main() -> None:
    # JPEG does not support alpha (RGBA). Use RGB for JPEG inputs/outputs.
    src = Path("input.jpg")
    dst = Path("output.png")

    # Create a small, valid JPEG for the example (RGB only).
    Image.new("RGB", (7, 5), (10, 20, 30)).save(src, format="JPEG")

    with Image.open(src) as im:
        print("format:", im.format)
        print("mode:", im.mode)
        print("size:", im.size)

        # Common normalization for processing
        rgb = im.convert("RGB")
        rgb.save(dst, format="PNG")


if __name__ == "__main__":
    main()
```
* Use `Image.open()` as a context manager to ensure the file handle is closed.
* Use `Image.Image.convert()` to normalize mode before processing/saving.

### Create images and draw shapes/text ✅ Current
```python
from __future__ import annotations

from pathlib import Path
from PIL import Image, ImageDraw, ImageFont


def main() -> None:
    out = Path("card.png")

    im = Image.new("RGBA", (640, 240), (255, 255, 255, 255))
    draw = ImageDraw.Draw(im)

    # Draw a filled rounded rectangle with a visible outline.
    # Note: The outline is drawn centered on the edge; sampling exactly at (20, 20)
    # may hit the background due to anti-aliasing/rounding. Sample inside the stroke.
    draw.rounded_rectangle(
        (20, 20, 620, 220),
        radius=24,
        fill=(245, 245, 245, 255),
        outline=(0, 0, 0, 255),
        width=6,
    )
    draw.line((40, 120, 600, 120), fill=(0, 0, 0, 255), width=3)

    # Font loading: prefer truetype if available; fall back to default bitmap font
    try:
        font = ImageFont.truetype("DejaVuSans.ttf", 32)
    except OSError:
        font = ImageFont.load_default()

    draw.text((40, 60), "Pillow 11.1.0", fill=(0, 0, 0, 255), font=font)

    im.save(out, format="PNG")

    # Optional quick sanity check (kept self-contained and robust):
    # pick a point well within the outline stroke (top edge) to avoid corner rounding.
    # Also sample a point on the horizontal divider line (deterministic).
    with Image.open(out) as im2:
        px_outline = im2.getpixel((40, 22))  # inside top outline region
        assert px_outline[3] == 255 and px_outline[:3] != (255, 255, 255)

        px_line = im2.getpixel((60, 120))  # on the divider line
        assert px_line[3] == 255 and px_line[:3] != (245, 245, 245)


if __name__ == "__main__":
    main()
```
* `Image.new()` creates an image buffer; `ImageDraw.Draw()` provides drawing primitives.
* `ImageFont.truetype()` uses FreeType; handle `OSError` if the font file is missing.

### Preserve/write PNG text metadata (iTXt/tEXt) ✅ Current
```python
from __future__ import annotations

from pathlib import Path
from PIL import Image
from PIL.PngImagePlugin import PngInfo


def main() -> None:
    src = Path("input.png")
    dst = Path("with-metadata.png")

    pnginfo = PngInfo()
    pnginfo.add_text("Title", "Example")
    pnginfo.add_text("Description", "Unicode ✓ and long text stored in PNG chunks")

    with Image.open(src) as im:
        im.save(dst, pnginfo=pnginfo)


if __name__ == "__main__":
    main()
```
* Use `PIL.PngImagePlugin.PngInfo` to add text chunks when saving PNG files.
* Pillow may encode some text as iTXt depending on content; treat it as an implementation detail.

### Handle unknown/invalid image inputs safely ✅ Current
```python
from __future__ import annotations

from pathlib import Path
from PIL import Image, UnidentifiedImageError


def main() -> None:
    path = Path("maybe-an-image.bin")
    try:
        with Image.open(path) as im:
            im.verify()  # validate file headers/structure
        # Re-open after verify() if you need to load pixels
        with Image.open(path) as im2:
            im2.load()
            print("Loaded:", im2.format, im2.size, im2.mode)
    except FileNotFoundError:
        print("Missing file:", path)
    except UnidentifiedImageError:
        print("Not a supported image:", path)


if __name__ == "__main__":
    main()
```
* Catch `PIL.UnidentifiedImageError` for unsupported/invalid images.
* `verify()` is for validation; reopen to actually decode pixels.

## Configuration

- Installation (prefer interpreter-qualified pip to avoid environment mismatch):
  - `python3 -m pip install --upgrade pip`
  - `python3 -m pip install --upgrade Pillow`
- Optional dependencies (install only if you need the feature):
  - XMP metadata reading: `python3 -m pip install --upgrade defusedxml`
  - FPX/MIC support: `python3 -m pip install --upgrade olefile`
- Building from source:
  - By default, Pillow expects system **zlib** and **libjpeg** available (unless explicitly disabled via build flags).
  - If intentionally disabling components, follow documented build flags (e.g., `--config-settings="-C jpeg=disable"`).
- Text shaping (complex layout):
  - Optional `libraqm` support requires FreeType, HarfBuzz, FriBiDi installed before building.
  - On Windows wheels (>= 8.2.0), `fribidi.dll` must be discoverable via the DLL search path for Raqm features.

## Pitfalls

### Wrong: Installing Pillow into a different interpreter environment
```python
# This may install into a different Python than the one you run later.
# (Shown as Python to keep this file runnable; do not execute.)
cmd = "pip install --upgrade Pillow"
print(cmd)
```

### Right: Use `python -m pip` for the intended interpreter
```python
# (Shown as Python to keep this file runnable; do not execute.)
cmds = [
    "python3 -m pip install --upgrade pip",
    "python3 -m pip install --upgrade Pillow",
]
print("\n".join(cmds))
```

### Wrong: Expecting XMP metadata without installing `defusedxml`
```python
from __future__ import annotations

from PIL import Image

# XMP reading is optional; without defusedxml, XMP may be unavailable.
with Image.open("in.jpg") as im:
    _ = im.info  # may not include XMP without optional dependency
```

### Right: Install the optional dependency when you need XMP
```python
from __future__ import annotations

# (Shown as Python to keep this file runnable; do not execute.)
cmd = "python3 -m pip install --upgrade defusedxml"
print(cmd)
```

### Wrong: Forgetting to close images (leaking file handles)
```python
from __future__ import annotations

from PIL import Image

im = Image.open("input.jpg")  # file handle may remain open
im.load()
print(im.size)
# no close()
```

### Right: Use a context manager for `Image.open()`
```python
from __future__ import annotations

from PIL import Image

with Image.open("input.jpg") as im:
    im.load()
    print(im.size)
```

### Wrong: Using `verify()` and then continuing to use the same image object
```python
from __future__ import annotations

from PIL import Image

with Image.open("input.png") as im:
    im.verify()
    # After verify(), the image object should not be used for pixel access.
    im.load()  # may fail or behave unexpectedly
```

### Right: Re-open after `verify()` to decode pixels
```python
from __future__ import annotations

from PIL import Image

with Image.open("input.png") as im:
    im.verify()

with Image.open("input.png") as im2:
    im2.load()
    print(im2.mode, im2.size)
```

### Wrong: Opening FPX/MIC without `olefile` installed
```python
from __future__ import annotations

from PIL import Image

# FPX/MIC reading requires olefile; this may fail without it.
with Image.open("in.fpx") as im:
    print(im.size)
```

### Right: Install `olefile` when working with FPX/MIC
```python
from __future__ import annotations

# (Shown as Python to keep this file runnable; do not execute.)
cmd = "python3 -m pip install --upgrade olefile"
print(cmd)
```

## References

- [Official Documentation](https://pillow.readthedocs.io/)
- [GitHub Repository](https://github.com/python-pillow/Pillow)
- https://github.com/python-pillow/Pillow/releases

## Migration from v11.0.0

- v11.1.0: breaking changes are not included in the provided snippet; review release notes:
  - https://github.com/python-pillow/Pillow/releases

Key v11.0.0 changes to account for when coming from 10.x:
- Python 3.8 support dropped → require Python 3.9+ in runtime/CI.
- `PIL.ContainerIO` changed to subclass `IO` → adjust strict type checks.
- Removed APIs: `PSFile`, `PyAccess`, `USE_CFFI_ACCESS`, and `TiffImagePlugin.IFD_LEGACY_API` → remove imports/usages.

Example: avoid strict base-class assumptions for `ContainerIO`
```python
from __future__ import annotations

from PIL import ContainerIO

def accepts_io(obj: object) -> bool:
    # Prefer duck-typing / IO-like checks rather than strict base-class comparisons.
    return hasattr(obj, "read") and hasattr(obj, "seek")

print(accepts_io(ContainerIO.ContainerIO(b"data")))
```

## API Reference

- **PIL** - Top-level package namespace.
- **PIL.UnidentifiedImageError** - Exception raised when an image cannot be opened/identified.
- **PIL.ContainerIO** - File-like wrapper for container-based image access (subclasses IO as of 11.0.0).
- **PIL.ImageMode** - Image mode helpers/definitions (internal to mode handling; used indirectly via `Image.mode`/`convert()`).
- **PIL.ImageDraw2** - Alternative drawing interface (less commonly used than `PIL.ImageDraw`).
- **PIL.PngImagePlugin.PngInfo** - Container for PNG metadata chunks when saving.
- **PIL.PngImagePlugin.iTXt** - Represents PNG iTXt chunk text entries.
- **PIL.FontFile** - Base support for bitmap font files (used by font loaders).
- **PIL.BdfFontFile** - Loader for BDF bitmap fonts.
- **PIL.PcfFontFile** - Loader for PCF bitmap fonts.
- **PIL.PaletteFile** - Palette file reader support.
- **PIL.GimpPaletteFile** - Loader for GIMP palette files.
- **PIL.GimpGradientFile** - Loader for GIMP gradient files.
- **PIL.TarIO** - File-like access to members inside tar archives.
- **PIL.WalImageFile** - Loader for WAL image files.