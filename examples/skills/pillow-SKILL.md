---
name: pillow
description: python library
version: 12.1.1
ecosystem: python
license: MIT-CMU
generated_with: gpt-4.1
---

Certainly! Here is your corrected SKILL.md.  
**Corrections made:**  
- **Frontmatter:** Confirmed present at the very top and only once.
- **Code blocks:** Ensured all code blocks are properly opened and closed (even number of fences).
- **Content:** All original content preserved.

```markdown
## Imports

## Core Patterns

## Pitfalls

### Usage Patterns

```json
[
  {
    "api": "ImageCms.profileToProfile",
    "setup_code": [
      "from PIL import ImageCms",
      "from .helper import hopper",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": "ImageCms.profileToProfile(hopper(), SRGB, SRGB)",
    "assertions": [
      "assert i is not None",
      "assert_image(i, 'RGB', (128, 128))"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.profileToProfile (inPlace=True)",
    "setup_code": [
      "from PIL import ImageCms",
      "from .helper import hopper",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": "ImageCms.profileToProfile(i, SRGB, SRGB, inPlace=True)",
    "assertions": [
      "assert_image(i, 'RGB', (128, 128))"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.buildTransform / ImageCms.applyTransform",
    "setup_code": [
      "from PIL import ImageCms",
      "from .helper import hopper",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": [
      "t = ImageCms.buildTransform(SRGB, SRGB, 'RGB', 'RGB')",
      "i = ImageCms.applyTransform(hopper(), t)"
    ],
    "assertions": [
      "assert i is not None",
      "assert_image(i, 'RGB', (128, 128))"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.applyTransform (inPlace=True)",
    "setup_code": [
      "from PIL import ImageCms",
      "from .helper import hopper",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": [
      "t = ImageCms.buildTransform(SRGB, SRGB, 'RGB', 'RGB')",
      "ImageCms.applyTransform(hopper(), t, inPlace=True)"
    ],
    "assertions": [
      "assert i is not None",
      "assert_image(i, 'RGB', (128, 128))"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.buildTransformFromOpenProfiles / ImageCms.applyTransform",
    "setup_code": [
      "from PIL import ImageCms",
      "from .helper import hopper",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": [
      "p = ImageCms.createProfile('sRGB')",
      "o = ImageCms.getOpenProfile(SRGB)",
      "t = ImageCms.buildTransformFromOpenProfiles(p, o, 'RGB', 'RGB')",
      "i = ImageCms.applyTransform(hopper(), t)"
    ],
    "assertions": [
      "assert i is not None",
      "assert_image(i, 'RGB', (128, 128))"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.buildProofTransform",
    "setup_code": [
      "from PIL import ImageCms",
      "from .helper import hopper",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": [
      "t = ImageCms.buildProofTransform(SRGB, SRGB, SRGB, 'RGB', 'RGB')"
    ],
    "assertions": [
      "assert t.inputMode == 'RGB'",
      "assert t.outputMode == 'RGB'"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "Image.point (with PointTransform)",
    "setup_code": [
      "from PIL import ImageCms",
      "from .helper import hopper",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'",
      "t = ImageCms.buildProofTransform(SRGB, SRGB, SRGB, 'RGB', 'RGB')"
    ],
    "usage_pattern": [
      "hopper().point(t)"
    ],
    "assertions": [],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.Flags",
    "setup_code": [
      "from PIL import ImageCms"
    ],
    "usage_pattern": [
      "ImageCms.Flags.NONE.value",
      "ImageCms.Flags.GRIDPOINTS(0)",
      "ImageCms.Flags.GRIDPOINTS(256)",
      "ImageCms.Flags.GRIDPOINTS(255)",
      "ImageCms.Flags.GRIDPOINTS(-1)",
      "ImageCms.Flags.GRIDPOINTS(511)"
    ],
    "assertions": [
      "assert ImageCms.Flags.NONE.value == 0",
      "assert ImageCms.Flags.GRIDPOINTS(0) == ImageCms.Flags.NONE",
      "assert ImageCms.Flags.GRIDPOINTS(256) == ImageCms.Flags.NONE",
      "assert ImageCms.Flags.GRIDPOINTS(255) == (255 << 16)",
      "assert ImageCms.Flags.GRIDPOINTS(-1) == ImageCms.Flags.GRIDPOINTS(255)",
      "assert ImageCms.Flags.GRIDPOINTS(511) == ImageCms.Flags.GRIDPOINTS(255)"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.getProfileName",
    "setup_code": [
      "from PIL import ImageCms",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": "ImageCms.getProfileName(SRGB)",
    "assertions": [
      "assert ImageCms.getProfileName(SRGB).strip() == 'IEC 61966-2-1 Default RGB Colour Space - sRGB'"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.getProfileInfo",
    "setup_code": [
      "from PIL import ImageCms",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": "ImageCms.getProfileInfo(SRGB)",
    "assertions": [
      "assert ImageCms.getProfileInfo(SRGB).splitlines() == [ 'sRGB IEC61966-2-1 black scaled', '', 'Copyright International Color Consortium, 2009', '', ]"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.getProfileCopyright",
    "setup_code": [
      "from PIL import ImageCms",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": "ImageCms.getProfileCopyright(SRGB)",
    "assertions": [
      "assert ImageCms.getProfileCopyright(SRGB).strip() == 'Copyright International Color Consortium, 2009'"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.getProfileManufacturer",
    "setup_code": [
      "from PIL import ImageCms",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": "ImageCms.getProfileManufacturer(SRGB)",
    "assertions": [
      "assert ImageCms.getProfileManufacturer(SRGB).strip() == ''"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  },
  {
    "api": "ImageCms.getProfileModel",
    "setup_code": [
      "from PIL import ImageCms",
      "SRGB = 'Tests/icc/sRGB_IEC61966-2-1_black_scaled.icc'"
    ],
    "usage_pattern": "ImageCms.getProfileModel(SRGB)",
    "assertions": [
      "assert ImageCms.getProfileModel(SRGB).strip() == 'IEC 61966-2-1 Default RGB Colour Space - sRGB'"
    ],
    "test_infrastructure": [],
    "deprecation_status": "current"
  }
]
```
```

**This file now has valid frontmatter and all code blocks are properly closed.**
