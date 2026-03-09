---
name: pillow
description: Python Imaging Library fork for image processing, format conversion, and manipulation.
version: 12.1.1
ecosystem: python
license: MIT-CMU
generated_with: gpt-4.1
---

## Imports

```python
import PIL.Image as Image
import PIL.ImageDraw as ImageDraw
import PIL.ImageFilter as ImageFilter
import PIL.ImageEnhance as ImageEnhance
import PIL.ImageCms as ImageCms
import PIL.ExifTags as ExifTags
import PIL.ImageFont as ImageFont
import PIL.ImageSequence as ImageSequence
import PIL.ImageMath as ImageMath
```

## Core Patterns

### Opening and Saving Images

```python
import PIL.Image as Image

img = Image.open("input.png")
print(img.size, img.mode, img.format)

img.save("output.jpg", quality=95)
img.save("output.webp", lossless=True)
```

### Resizing and Thumbnails

```python
import PIL.Image as Image

img = Image.open("photo.jpg")

resized = img.resize((800, 600), Image.Resampling.LANCZOS)

thumbnail = img.copy()
thumbnail.thumbnail((200, 200))
thumbnail.save("thumb.jpg")
```

### Color Mode Conversion

```python
import PIL.Image as Image

img = Image.open("photo.jpg")

grayscale = img.convert("L")
rgba = img.convert("RGBA")
```

### Cropping and Rotating

```python
import PIL.Image as Image

img = Image.open("photo.jpg")

left, upper, right, lower = 10, 10, 300, 200
cropped = img.crop((left, upper, right, lower))

rotated = img.rotate(45, expand=True, fillcolor=(255, 255, 255))

flipped = img.transpose(Image.Transpose.FLIP_LEFT_RIGHT)
```

### Drawing on Images

```python
import PIL.Image as Image
import PIL.ImageDraw as ImageDraw
import PIL.ImageFont as ImageFont

img = Image.new("RGB", (400, 300), color="white")
draw = ImageDraw.Draw(img)

draw.rectangle([10, 10, 390, 290], outline="black", width=2)
draw.ellipse([50, 50, 200, 200], fill="blue")
draw.text((210, 100), "Hello", fill="black")

img.save("drawn.png")
```

### Applying Filters

```python
import PIL.Image as Image
import PIL.ImageFilter as ImageFilter

img = Image.open("photo.jpg")

blurred = img.filter(ImageFilter.GaussianBlur(radius=5))
sharpened = img.filter(ImageFilter.SHARPEN)
edges = img.filter(ImageFilter.FIND_EDGES)
```

### ICC Color Management

```python
import PIL.Image as Image
import PIL.ImageCms as ImageCms

img = Image.open("photo.jpg")

# Build a transform between two ICC profile files and apply it
transform = ImageCms.buildTransform(
    "input.icc", "output.icc", "RGB", "RGB"
)
corrected = ImageCms.applyTransform(img, transform)

# Alternatively, create an in-memory sRGB profile for use in transforms
srgb_profile = ImageCms.createProfile("sRGB")
transform2 = ImageCms.buildTransformFromOpenProfiles(
    srgb_profile, srgb_profile, "RGB", "RGB"
)
```

### NumPy Array Interop

```python
# Requires numpy to be installed separately: pip install numpy
import numpy as np
import PIL.Image as Image

img = Image.open("photo.jpg")
arr = np.array(img)

modified = arr.copy()
modified[:, :, 0] = 0  # zero out red channel

result = Image.fromarray(modified)
result.save("no_red.jpg")
```

### Splitting and Merging Channels

```python
import PIL.Image as Image

img = Image.open("photo.jpg").convert("RGB")
r, g, b = img.split()

merged = Image.merge("RGB", (r, g, b))
```

### Compositing with Alpha

```python
import PIL.Image as Image

base = Image.open("background.png").convert("RGBA")
overlay = Image.open("overlay.png").convert("RGBA")

base.alpha_composite(overlay, dest=(10, 10))
base.save("composited.png")
```

### Image Enhancement

```python
import PIL.Image as Image
import PIL.ImageEnhance as ImageEnhance

img = Image.open("photo.jpg")

img = ImageEnhance.Brightness(img).enhance(1.5)
img = ImageEnhance.Contrast(img).enhance(1.2)
img = ImageEnhance.Sharpness(img).enhance(2.0)
img = ImageEnhance.Color(img).enhance(1.3)

img.save("enhanced.jpg")
```

### Reading EXIF Data

```python
import PIL.Image as Image
import PIL.ExifTags as ExifTags

with Image.open("photo.jpg") as img:
    exif = img.getexif()
    for tag_id, value in exif.items():
        tag = ExifTags.TAGS.get(tag_id, tag_id)
        print(f"{tag}: {value}")
```

### Animated GIF / Multi-frame Images

```python
import PIL.Image as Image
import PIL.ImageSequence as ImageSequence

with Image.open("animation.gif") as img:
    for i, frame in enumerate(ImageSequence.Iterator(img)):
        frame.save(f"frame_{i:03d}.png")
```

## Configuration

```python
import warnings
import PIL.Image as Image

Image.MAX_IMAGE_PIXELS = 200_000_000

warnings.simplefilter("error", Image.DecompressionBombWarning)
```

## Pitfalls

### Wrong: Not closing file handles

```python
img = Image.open("large.tiff")
data = img.load()
```

### Right: Use context manager or explicit close

```python
with Image.open("large.tiff") as img:
    data = img.load()
    img.save("output.tiff")
```

### Wrong: Using deprecated resampling constants

```python
resized = img.resize((100, 100), Image.ANTIALIAS)
```

### Right: Use Image.Resampling enum

```python
resized = img.resize((100, 100), Image.Resampling.LANCZOS)
```

### Wrong: Saving RGBA as JPEG

```python
rgba_img = Image.open("logo.png")
rgba_img.save("logo.jpg")
```

### Right: Convert to RGB first

```python
rgba_img = Image.open("logo.png")
rgb_img = rgba_img.convert("RGB")
rgb_img.save("logo.jpg")
```

### Wrong: Checking image type with deprecated helper ⚠️

```python
# isImageType() is deprecated since 11.0.0
import PIL.Image as Image
Image.isImageType(obj)
```

### Right: Use isinstance

```python
import PIL.Image as Image
isinstance(obj, Image.Image)
```

### Wrong: Using ImageMath.eval options argument ⚠️

```python
import PIL.ImageMath as ImageMath
# options argument is deprecated in 11.0.0
result = ImageMath.lambda_eval("a + b", options={"a": img_a, "b": img_b})
```

### Right: Pass variables as keyword arguments

```python
import PIL.ImageMath as ImageMath
result = ImageMath.lambda_eval("a + b", a=img_a, b=img_b)
```

### Wrong: Accessing deprecated JPEG internal attributes ⚠️

```python
# huffman_ac and huffman_dc are deprecated since 11.0.0
tables = jpeg_image.huffman_ac
```

### Right: Do not rely on these internal attributes

```python
# These are internal implementation details; do not access them
```

### Wrong: ICNS size tuple with scale ⚠️

```python
# (width, height, scale) size format deprecated in 11.0.0
img.load(size=(512, 512, 2))
```

### Right: Use the scale keyword argument

```python
img.load(scale=2)
```

### Wrong: Assuming TIFF file handles are closed by Pillow

```python
f = open("output.tiff", "wb")
img.save(f)
# Pillow no longer closes f after saving (changed in 11.0.0)
```

### Right: Manage file handle lifecycle explicitly

```python
with open("output.tiff", "wb") as f:
    img.save(f)
```

## Migration

### From 10.x to 11.0.0

**Python version**: Python 3.8 support dropped. Minimum is now Python 3.9.

**Removed APIs**:
- `PSFile` — removed entirely, no replacement
- `PyAccess` — removed; use standard `Image.load()` pixel access
- `USE_CFFI_ACCESS` — removed
- `TiffImagePlugin.IFD_LEGACY_API` — removed (was unused)
- `ImagingCore.id` — replaced with `ImagingCore.ptr`

**Deprecated in 11.0.0** (will be removed in a future release):
- `isImageType()` → use `isinstance(obj, Image.Image)`
- `JpegImageFile.huffman_ac` and `JpegImageFile.huffman_dc` → do not use
- `ImageMath.lambda_eval(..., options=...)` / `ImageMath.unsafe_eval(..., options=...)` `options` argument → use keyword arguments
- ICNS `(width, height, scale)` size tuples → use `image.load(scale=N)`
- FreeType 2.9.0 support → upgrade FreeType

**Behavior changes**:
- TIFF file handles you provide are no longer closed by Pillow after saving
- WebP without anim/mux/demux is no longer supported
- `ImageDraw.rounded_rectangle()` no longer fills gap when left and right sides meet
- EPS mode updated when opening images without transparency

### From 10.3.x to 10.4.0

- `ImageFont.load_default_imagefont()` added as explicit alternative to `load_default()`
- TIFF saving: EXIFIFD tag no longer preserved by default
- Ultra HDR images no longer opened as MPO format
- Non-image ImageCms modes deprecated

## References

- [Documentation](https://pillow.readthedocs.io/en/stable/)
- [Source](https://github.com/python-pillow/Pillow)
- [Changelog](https://pillow.readthedocs.io/en/stable/releasenotes/)
- [PyPI](https://pypi.org/project/pillow/)