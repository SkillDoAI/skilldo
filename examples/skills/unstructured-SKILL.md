---

name: unstructured
description: Partition unstructured documents into typed elements and optionally chunk them for downstream processing.
version: 0.18.34
ecosystem: python
license: Apache-2.0"
generated_with: gpt-5.2
---

## Imports

```python
import unstructured

from unstructured.partition.auto import partition
from unstructured.partition.pdf import partition_pdf
from unstructured.partition.text import partition_text
from unstructured.partition.html import partition_html

from unstructured.chunking.title import chunk_by_title
from unstructured.chunking.basic import chunk_elements

from unstructured.cleaners.core import clean_bullets
from unstructured.nlp.tokenize import detect_languages
from unstructured.utils import group_elements_by_parent_id

from unstructured.partition.common import UnsupportedFileFormatError
from unstructured.documents.elements import Text, Table, TableChunk, CodeSnippet
```

## Core Patterns

### Auto-partition a file by type ✅ Current
```python
from __future__ import annotations

from pathlib import Path

from unstructured.partition.auto import partition


def main() -> None:
    # NOTE: Auto-partition still requires format-specific extras for some file types.
    # For PDFs install:
    #   pip install "unstructured[pdf]"
    filename = "example.txt"
    path = Path(filename)
    if not path.exists():
        raise FileNotFoundError(filename)

    elements = partition(filename=str(path))
    for el in elements[:10]:
        print(type(el).__name__, getattr(el, "text", "")[:80])


if __name__ == "__main__":
    main()
```
* Prefer `unstructured.partition.auto.partition(...)` when you want file-type detection and dispatch to the correct partitioner.

### Use a file-specific partitioner (PDF) ✅ Current
```python
from __future__ import annotations

from pathlib import Path

from unstructured.partition.auto import partition


def main() -> None:
    # Install a PDF-capable build:
    #   pip install "unstructured[pdf]"
    #
    # NOTE: In minimal containers/CI, importing the PDF-specific partitioner can
    # pull in optional native deps (e.g., OpenCV) that may require system libs
    # like libxcb.so.1. To keep this example robust, use auto-partitioning and
    # explicitly request the PDF "fast" strategy to avoid heavier layout/model deps.
    filename = "example.pdf"
    pdf_path = Path(filename)
    if not pdf_path.exists():
        raise FileNotFoundError(filename)

    elements = partition(
        filename=str(pdf_path),
        strategy="fast",          # text-focused; avoids heavier layout/model deps in many envs
        infer_table_structure=False,
    )

    print(f"Partitioned {len(elements)} elements")
    print("First element type:", type(elements[0]).__name__ if elements else "none")


if __name__ == "__main__":
    main()
```
* Use `partition(..., strategy="fast")` for PDFs when you want PDF behavior but need to avoid importing heavier PDF/layout dependencies that may require missing system libraries in minimal environments.

### Chunk elements by title (character-based) ✅ Current
```python
from __future__ import annotations

from unstructured.partition.text import partition_text
from unstructured.chunking.title import chunk_by_title


def main() -> None:
    elements = partition_text(filename="example.txt")
    chunks = chunk_by_title(elements, max_characters=2000)

    for ch in chunks[:5]:
        print(type(ch).__name__, getattr(ch, "text", "")[:120)


if __name__ == "__main__":
    main()
```
* `chunk_by_title()` groups content by detected titles/headings and then chunks to a size limit (character-based when using `max_characters`).

### Token-based chunking (requires extra) ✅ Current
```python
from __future__ import annotations

from unstructured.partition.text import partition_text
from unstructured.chunking.basic import chunk_elements


def main() -> None:
    # Requires: pip install "unstructured[chunking-tokens]"
    elements = partition_text(filename="example.txt")

    chunks = chunk_elements(
        elements,
        max_tokens=512,
        new_after_n_tokens=512,
        tokenizer="cl100k_base",
    )
    print("Chunks:", len(chunks))


if __name__ == "__main__":
    main()
```
* Token-based chunking was added in 0.18.31; install `unstructured[chunking-tokens]` and pass `max_tokens`, `new_after_n_tokens`, and `tokenizer`.

### Traverse hierarchy using parent IDs ✅ Current
```python
from __future__ import annotations

from unstructured.partition.auto import partition
from unstructured.utils import group_elements_by_parent_id


def main() -> None:
    elements = partition(filename="example.html")
    grouped = group_elements_by_parent_id(elements)

    # grouped maps parent_id -> list of child elements
    for parent_id, children in list(grouped.items())[:3]:
        print("parent_id:", parent_id, "children:", len(children))


if __name__ == "__main__":
    main()
```
* Use `group_elements_by_parent_id()` (added in 0.18.33) to group elements by `element.metadata.parent_id` for hierarchy-aware processing.

## Configuration

- **Install extras per format** (recommended to avoid heavy dependency footprints):
  - Example: `pip install "unstructured[docx,pptx]"` or `pip install "unstructured[all-docs]"`.
- **Token chunking extra**:
  - `pip install "unstructured[chunking-tokens]"` to enable `max_tokens` / `tokenizer` chunking options.
- **Environment variables**
  - `DO_NOT_TRACK=true` to disable analytics ping. Set **before** importing `unstructured`.
  - `ALLOW_PANDOC_NO_SANDBOX=true` to allow pandoc fallback to `sandbox=False` for conversions that fail in sandboxed mode (security tradeoff; set only if you accept the risk).
- **PDF behavior changes across nearby versions**
  - 0.18.30: rendering backend moved from `pdf2image` to PyPDFium2 (validate rendering/OCR output).
  - 0.18.31: default PDF DPI changed to 350 (may affect performance and output geometry).

## Pitfalls

### Wrong: Setting `DO_NOT_TRACK` after importing `unstructured`
```python
import os
import unstructured

os.environ["DO_NOT_TRACK"] = "true"  # too late in many setups
```

### Right: Set `DO_NOT_TRACK` before importing `unstructured`
```python
import os

os.environ["DO_NOT_TRACK"] = "true"

import unstructured  # noqa: E402
```

### Wrong: Using `partition()` on a format without installing the required extra
```python
from unstructured.partition.auto import partition

# Likely to fail at runtime if you only installed `pip install unstructured`
elements = partition(filename="file.docx")
print(len(elements))
```

### Right: Install the needed extra, then call `partition()`
```python
# pip install "unstructured[docx]"

from unstructured.partition.auto import partition

elements = partition(filename="file.docx")
print(len(elements))
```

### Wrong: Using token-based chunking without installing `unstructured[chunking-tokens]`
```python
from unstructured.partition.text import partition_text
from unstructured.chunking.title import chunk_by_title

elements = partition_text(filename="example.txt")
chunks = chunk_by_title(elements, max_tokens=512)  # may fail without token extra
print(len(chunks))
```

### Right: Install token extra and pass a tokenizer
```python
# pip install "unstructured[chunking-tokens]"

from unstructured.partition.text import partition_text
from unstructured.chunking.title import chunk_by_title

elements = partition_text(filename="example.txt")
chunks = chunk_by_title(elements, max_tokens=512, tokenizer="cl100k_base")
print(len(chunks))
```

### Wrong: Expecting pandoc to automatically fall back to `sandbox=False` without opt-in
```python
from unstructured.partition.auto import partition

# For some files (e.g. ODT), conversion can fail if sandboxed conversion isn't sufficient.
elements = partition(filename="file.odt")
print(len(elements))
```

### Right: Explicitly opt in to no-sandbox fallback (only if you accept the risk)
```python
import os

os.environ["ALLOW_PANDOC_NO_SANDBOX"] = "true"

from unstructured.partition.auto import partition  # noqa: E402

elements = partition(filename="file.odt")
print(len(elements))
```

### Wrong: Not handling unsupported formats (crashes pipeline)
```python
from unstructured.partition.auto import partition

elements = partition(filename="file.unknown_ext")
print(len(elements))
```

### Right: Catch `UnsupportedFileFormatError` and handle it
```python
from unstructured.partition.auto import partition
from unstructured.partition.common import UnsupportedFileFormatError


def main() -> None:
    try:
        elements = partition(filename="file.unknown_ext")
    except UnsupportedFileFormatError as exc:
        print("Unsupported format:", exc)
        return

    print("Elements:", len(elements))


if __name__ == "__main__":
    main()
```

## References

- [Official Documentation](https://docs.unstructured.io/)
- [GitHub Repository](https://github.com/Unstructured-IO/unstructured)

## Migration from v0.18.30

- **0.18.31 (behavior change)**: Default PDF DPI changed to **350**.
  - Migration: if performance/memory/output geometry changed, explicitly set DPI where your PDF pipeline supports it or pin versions.
- **0.18.30 (behavior change)**: PDF rendering backend switched from `pdf2image` to **PyPDFium2**.
  - Migration: re-validate PDF rendering fidelity and OCR outputs; adjust system/deployment dependencies.
- **0.18.31 (feature)**: Token-based chunking added to `chunk_by_title()` / `chunk_elements()` via `max_tokens`, `new_after_n_tokens`, `tokenizer`.
  - Migration: install `unstructured[chunking-tokens]` and switch to token parameters if you need token limits.
- **0.18.33 (feature)**: `group_elements_by_parent_id()` added for hierarchy traversal.

Before/after example (token chunking):
```python
from unstructured.chunking.title import chunk_by_title
from unstructured.partition.text import partition_text

elements = partition_text(filename="example.txt")

# Before (character-based)
chunks_chars = chunk_by_title(elements, max_characters=4000)

# After (token-based; requires `unstructured[chunking-tokens]`)
chunks_tokens = chunk_by_title(elements, max_tokens=512, tokenizer="cl100k_base")
print(len(chunks_chars), len(chunks_tokens))
```

## API Reference

- **unstructured.partition.auto.partition()** - Auto-detect file type and partition into elements; key params commonly include `filename=...` (and sometimes file-like/content options depending on format).
- **unstructured.partition.pdf.partition_pdf()** - Partition PDFs into elements; use when you know the input is PDF and want PDF-specific behavior.
- **unstructured.partition.text.partition_text()** - Partition plain text files into elements.
- **unstructured.partition.html.partition_html()** - Partition HTML into elements.
- **unstructured.chunking.title.chunk_by_title()** - Chunk elements using title boundaries; supports `max_characters` and (0.18.31+) `max_tokens`, `tokenizer`.
- **unstructured.chunking.basic.chunk_elements()** - Generic chunking over elements; supports character and (0.18.31+) token-based chunking options.
- **unstructured.documents.elements.Text** - Element type representing narrative/paragraph-like text.
- **unstructured.documents.elements.CodeSnippet** - Element type representing code blocks/snippets.
- **unstructured.documents.elements.Table** - Element type representing tables.
- **unstructured.documents.elements.TableChunk** - Chunk type for table content when chunking splits/represents tables.
- **unstructured.cleaners.core.clean_bullets()** - Cleaner utility to normalize/remove bullet characters in text.
- **unstructured.nlp.tokenize.detect_languages()** - Detect languages for a text; `languages=None` is the modern default behavior (internally treated as auto).
- **unstructured.utils.group_elements_by_parent_id()** - Group elements by `metadata.parent_id` for hierarchy-aware processing (added in 0.18.33).
- **unstructured.partition.common.UnsupportedFileFormatError** - Raised when attempting to partition an unsupported/unknown file type.