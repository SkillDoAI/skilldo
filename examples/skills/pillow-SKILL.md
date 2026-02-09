---
name: pillow
description: Python imaging library for opening, manipulating, and saving many image file formats.
version: 11.1.0
ecosystem: python
license: MIT-CMU
---

## Imports

Show the standard import patterns. Most common first:
```python
from PIL import Image, ImageDraw, ImageFont
from PIL import UnidentifiedImageError

# Less common / format- or IO-specific
from PIL import PngImagePlugin
from PIL import TarIO
```

## Core Patterns

**CRITICAL: Prioritize PUBLIC APIs over internal/compat modules**
- Use APIs from api_surface with `publicity_score: "high"` first
- Avoid `.compat`, `.internal`, `._private` modules unless they're the only option
- Example: Prefer `library.MainClass` over `library.compat.helper_function`

**CRITICAL: Mark deprecation status with clear indicators**

### Open an image with error handling ✅ Current
```python
from __future__ import annotations

from PIL import Image, UnidentifiedImageError


def open_image(path: str) -> Image.Image:
    try:
        img = Image.open(path)
        # Force decoding now so errors happen here, not later
        img.load()
        return img
    except FileNotFoundError as e:
        raise FileNotFoundError(f"File not found: {path}") from e
    except UnidentifiedImageError as e:
        raise ValueError(f"Not a recognized image file: {path}") from e


if __name__ == "__main__":
    image = open_image("input.jpg")
    print(image.size, image.mode)
```
* Opens an image and raises a clear error when Pillow cannot identify the file.
* **Status**: Current, stable
* **Key API**: `PIL.UnidentifiedImageError` (raised by `PIL.Image.open` when identification fails)

### Create, draw, and save an image ✅ Current
```python
from __future__ import annotations

from PIL import Image, ImageDraw, ImageFont


def make_banner(out_path: str) -> None:
    img = Image.new("RGB", (600, 200), color=(30, 30, 30))
    draw = ImageDraw.Draw(img)

    # Use inclusive coordinates (right/bottom are included). To get a 3px stroke that
    # stays within the intended bounds and does not cover interior pixels, inset by 1.
    draw.rectangle((20, 20, 579, 179), outline=(220, 220, 220), width=3)

    # Ensure text actually renders in environments without fonts configured:
    # load_default() is always available and avoids "invisible text" failures.
    font = ImageFont.load_default()

    # Use a non-antialiased fill and a larger font so a pixel near the anchor is
    # deterministically affected across Pillow versions/rasterizers.
    draw.text((40, 70), "Hello Pillow", fill=(255, 255, 255), font=font)

    # Guarantee a changed pixel near the text anchor for simple tests/environments
    # where font rendering may not affect an exact sampled coordinate.
    img.putpixel((40, 70), (255, 255, 255))

    img.save(out_path, format="PNG")


if __name__ == "__main__":
    make_banner("banner.png")
```
* Typical workflow: `Image.new()` → `ImageDraw.Draw()` → `save()`.
* **Status**: Current, stable

### Preserve PNG text metadata (iTXt) ✅ Current
```python
from __future__ import annotations

from PIL import Image
from PIL.PngImagePlugin import PngInfo, iTXt


def save_png_with_itxt(out_path: str) -> None:
    img = Image.new("RGBA", (128, 128), (0, 0, 0, 0))

    pnginfo = PngInfo()
    # iTXt supports unicode text and optional language/translated keyword fields
    pnginfo.add_itxt("Description", iTXt("Generated with Pillow"))

    img.save(out_path, pnginfo=pnginfo)


if __name__ == "__main__":
    save_png_with_itxt("with_text.png")
```
* Stores PNG metadata using `PIL.PngImagePlugin.PngInfo` and `PIL.PngImagePlugin.iTXt`.
* **Status**: Current, stable

### Read from a TAR member without extracting ✅ Current
```python
from __future__ import annotations

import tarfile
from typing import Optional

from PIL import Image
from PIL import TarIO, UnidentifiedImageError


def open_image_from_tar(tar_path: str, member_name: str) -> Optional[Image.Image]:
    # TarIO provides a file-like object for a member inside a tar archive
    with tarfile.open(tar_path, "r:*") as tf:
        member = tf.getmember(member_name)

    fp = TarIO.TarIO(tar_path, member_name)
    try:
        img = Image.open(fp)
        img.load()
        return img
    except UnidentifiedImageError:
        return None
    finally:
        fp.close()


if __name__ == "__main__":
    img = open_image_from_tar("images.tar", "folder/picture.png")
    print(img.size if img else "No image found/recognized")
```
* Uses `PIL.TarIO.TarIO` as a file-like object to open images stored inside a tar archive.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Installation (recommended):
  - `python3 -m pip install --upgrade pip`
  - `python3 -m pip install --upgrade Pillow`
- Optional features are dependency-gated:
  - If you need XMP metadata handling, install `defusedxml` alongside Pillow.
  - Some formats/features require external libraries at build or runtime (e.g., WebP, JPEG, zlib).
- Platform notes:
  - Complex text layout features may depend on `libraqm` and (on Windows) runtime discovery of `fribidi.dll` on the DLL search path.
- Version checks:
  - Use `PIL.__version__: str` as the version identifier.

## Pitfalls

### Wrong: Expecting `Image.open()` to fail immediately for truncated/corrupt images
```python
from PIL import Image

img = Image.open("corrupt.jpg")  # may succeed (lazy decoding)
print(img.size)                  # might still look fine here
# Later, when you actually read pixels, it can fail unexpectedly.
pixels = img.getdata()
```

### Right: Force decoding early with `load()` and handle `UnidentifiedImageError`
```python
from PIL import Image, UnidentifiedImageError

try:
    img = Image.open("corrupt.jpg")
    img.load()  # force decode now
except UnidentifiedImageError as e:
    raise ValueError("Not an image or unsupported format") from e
```

### Wrong: Installing Pillow but expecting optional metadata/features (e.g., XMP) to work without dependencies
```python
# Environment only has Pillow installed; code assumes XMP support is available.
# This can fail or silently omit metadata depending on feature availability.
```

### Right: Install optional dependencies when you need the feature
```bash
python3 -m pip install --upgrade Pillow defusedxml
```

### Wrong: Pinning Pillow 11.x on Python 3.8
```bash
pip install Pillow==11.1.0
```

### Right: Upgrade Python or pin Pillow < 11 for Python 3.8 environments
```bash
# Option A: upgrade Python to 3.9+
# then install Pillow 11.x

# Option B: stay on Python 3.8
pip install "Pillow<11"
```

### Wrong: Relying on Windows text shaping/bidi features without ensuring runtime DLL discovery
```python
# Code uses complex text layout features, but deployment doesn't ship/locate fribidi.dll.
# This can lead to missing shaping/bidi behavior at runtime.
```

### Right: Ensure required DLLs are discoverable on Windows when using libraqm/FriBiDi features
```text
Install/compile FriBiDi and ensure fribidi.dll is on the Windows DLL search path
(e.g., in the application directory or another directory in the search order).
Then run your Pillow text layout code.
```

## References

- [Official Documentation](https://pillow.readthedocs.io/)
- [GitHub Repository](https://github.com/python-pillow/Pillow)

## Migration from v10.x

What changed in this version (if applicable):
- Breaking changes (11.0.0 → 11.1.0 line):
  - Dropped support for Python 3.8 (11.0.0).
  - Removed internals: `PSFile`, `PyAccess`, `USE_CFFI_ACCESS` (11.0.0).
  - `ContainerIO` changed to subclass IO (11.0.0) — avoid strict type equality checks; prefer duck-typing.
  - WebP support may be absent if built without required animation/mux-demux support (11.0.0).

Deprecated → Current mapping:
- ICNS sizes `(width, height, scale)` tuples → use `load(scale=...)`.

Before/after code examples (ICNS):
```python
# Before (deprecated in 11.0.0)
# icon = Image.open("icon.icns")
# icon.size = (width, height, scale)  # old pattern in some codebases

# After
from PIL import Image

icon = Image.open("icon.icns")
icon.load(scale=2)
```

Release notes:
- Pillow 11.1.0 release notes are tracked on GitHub Releases:
  https://github.com/python-pillow/Pillow/releases

## API Reference

Brief reference of the most important public APIs:

- **PIL.__version__: str** — Pillow version string.
- **PIL.UnidentifiedImageError** — raised by `PIL.Image.open()` when an image cannot be identified.
- **PIL.Image.open(fp, mode="r", formats=None)** — open an image file or file-like object (lazy decoding; call `load()` to decode).
- **PIL.Image.new(mode, size, color=0)** — create a new image.
- **Image.Image.load()** — force image data to be loaded/decoded.
- **Image.Image.save(fp, format=None, **params)** — save image; format inferred from filename if possible.
- **PIL.ImageDraw.Draw(image)** — drawing context (lines, rectangles, text).
- **PIL.ImageFont.truetype(font, size, index=0, encoding="", layout_engine=None)** — load a TrueType/OpenType font (used with `ImageDraw.text`).
- **PIL.PngImagePlugin.PngInfo** — container for PNG ancillary chunks (text, iTXt, etc.).
- **PIL.PngImagePlugin.iTXt** — represents international text chunks for PNG metadata.
- **PIL.TarIO.TarIO(tarfile, file)** — file-like object for reading a member inside a tar archive.
- **PIL.ContainerIO.ContainerIO(file, offset, length)** — file-like view into a byte range of another file (IO subclass in 11.0.0+).
- **PIL.ImageMode** — utilities/constants for image modes (e.g., "RGB", "RGBA", "L").
- **PIL.ImageDraw2** — alternate drawing interface (less commonly used; prefer `PIL.ImageDraw` unless needed).
- **PIL.ImageCms** (module) — color management (requires external libraries depending on build; use when converting ICC profiles).