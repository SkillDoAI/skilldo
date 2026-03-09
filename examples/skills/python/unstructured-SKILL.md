---  
name: unstructured  
description: Python library for preprocessing unstructured documents (PDFs, HTML, Word, PowerPoint, images, etc.) into structured formats for downstream ML/NLP tasks  
version: 0.21.5  
ecosystem: python  
license: Apache-2.0  
generated_with: gpt-4.1  
---  

# unstructured  

## Imports  

```python
# Auto-detection and partitioning
from unstructured.partition.auto import partition

# Specialized partitioners
from unstructured.partition.html import partition_html

# Plain‑text partitioner
from unstructured.partition.text import partition_text

# Chunking (decorator)
from unstructured.chunking.dispatch import add_chunking_strategy, Chunker
from unstructured.chunking.base import CHUNK_MAX_CHARS_DEFAULT, CHUNK_MULTI_PAGE_DEFAULT

# Serialization
from unstructured.staging.base import (
    elements_to_json,
    elements_from_json,
)

# Element types
from unstructured.documents.elements import (
    Element,
    Title,
    NarrativeText,
    ListItem,
    Table,
    Image,
    Header,
    Footer,
    ElementMetadata,
)

# Metrics and evaluation (optional – may require pandas)
# Element type metrics
from unstructured.metrics.element_type import (
    get_element_type_frequency,
    calculate_element_type_percent_match,
)

# Utilities
from unstructured.utils import (
    save_as_jsonl,
    read_from_jsonl,
    first,
    only,
    requires_dependencies,
    catch_overlapping_and_nested_bboxes,
)

# Error handling
from unstructured.partition.common import UnsupportedFileFormatError
```

> **Note**: The PDF and Markdown partitioners (`partition_pdf`, `partition_md`) and the metrics utilities that depend on *pandas* are optional extras. Install them with the appropriate extra requirements (e.g., `pip install "unstructured[pdf]"` or `pip install "unstructured[metrics]"`). They are omitted from the import list to avoid import‑time failures when the optional dependencies are not present.

## Installation  

```bash
# Base package (text, HTML, XML, JSON, emails only)
pip install unstructured

# All document types
pip install "unstructured[all-docs]"

# Specific document types
pip install "unstructured[docx,pptx]"
pip install "unstructured[pdf]"
```

**⚠️ Python 3.11+ Required**: As of v0.20.0, minimum Python version is 3.11. Supports Python 3.11, 3.12, and 3.13.

## Quick Start  

```python
from unstructured.partition.auto import partition

if __name__ == "__main__":
    # Auto-detect file type and extract elements
    elements = partition(filename="document.pdf")

    # Each element has text, type, and metadata
    for element in elements:
        print(f"{element.category}: {element.text[:100]}")
        print(f"Page: {element.metadata.page_number}")
```

## Core Concepts  

### Elements  
All partitioners return a list of `Element` objects. Common element types:  
- `Title` – Document titles and section headings  
- `NarrativeText` – Paragraphs and prose  
- `ListItem` – Bulleted or numbered list items  
- `Table` – Tabular data  
- `Image` – Image elements  
- `Footer` / `Header` – Page headers and footers  

> **Important**: In the current library version, classes such as `Title`, `NarrativeText`, etc., are *stand‑alone* data containers and **do not inherit** from the base `Element` class. They share a common interface but are not subclasses.

### Partitioning  
The `partition()` function auto‑detects file types and routes to specialized partitioners:

```python
from unstructured.partition.auto import partition

if __name__ == "__main__":
    # From file path
    elements = partition(filename="document.pdf")

    # From file object (keyword‑only)
    with open("document.pdf", "rb") as f:
        elements = partition(file=f)

    # From raw bytes with explicit content type (keyword‑only)
    with open("document.pdf", "rb") as f:
        elements = partition(file=f, content_type="application/pdf")
```

### Chunking  
Chunking is performed via a **decorator** that registers a chunking function. The library also provides a ready‑to‑use `Chunker` class for the default behavior.

```python
from unstructured.chunking.dispatch import add_chunking_strategy, Chunker
from unstructured.partition.auto import partition

# Define a custom chunker – here we simply delegate to the default Chunker
@add_chunking_strategy
def simple_chunker(elements):
    # The Chunker instance respects the default max‑char limit and multi‑page setting
    return Chunker()(elements)

if __name__ == "__main__":
    # Partition and then apply the registered chunking strategy
    raw_elements = partition(filename="long_document.pdf")
    # The decorator registers `simple_chunker`; you can call it directly:
    chunked = simple_chunker(raw_elements)

    for chunk in chunked:
        print(f"Chunk ({len(chunk.text)} chars): {chunk.text[:100]}...")
```

### Serialization  
Convert elements to JSON or other formats:

```python
from unstructured.staging.base import elements_to_json, elements_from_json

if __name__ == "__main__":
    # To JSON string
    json_string = elements_to_json(elements)

    # From JSON string (use keyword `text` to avoid confusion with the `filename` parameter)
    elements = elements_from_json(text=json_string)
```

## Core Patterns  

### Pattern 1: Basic Document Partitioning  

```python
# /// script
# dependencies = ["unstructured"]
# ///

from pathlib import Path
# Use the lightweight text partitioner for plain‑text files
from unstructured.partition.text import partition_text

if __name__ == "__main__":
    # -------------------------------------------------
    # Create a tiny plain‑text file to demonstrate partitioning
    # -------------------------------------------------
    txt_path = Path("sample.txt")
    txt_path.write_text(
        "Quarterly Report\n"
        "This quarter we saw a 15% increase in sales across all regions. "
        "The marketing campaigns performed better than expected, and the "
        "new product line received positive feedback from customers.",
        encoding="utf-8",
    )

    # -------------------------------------------------
    # Partition the text file using the pattern from SKILL.md
    # -------------------------------------------------
    elements = partition_text(filename=str(txt_path))

    # Simple sanity checks (will raise AssertionError if they fail)
    assert elements, "No elements were extracted"

    # Access metadata and print details
    for element in elements:
        print(f"Text: {element.text}")
        print(f"Type: {element.category}")
        if getattr(element.metadata, "page_number", None):
            print(f"Page: {element.metadata.page_number}")

    # -------------------------------------------------
    # Success indicator
    # -------------------------------------------------
    print("✓ Test passed: Pattern 1: Basic Document Partitioning")
```

### Pattern 2: HTML Partitioning  

```python
# /// script
# dependencies = ["unstructured"]
# ///

import pathlib
import threading
import socketserver
import http.server
import tempfile
import time

from unstructured.partition.html import partition_html


class _SimpleHTMLHandler(http.server.BaseHTTPRequestHandler):
    """Serve a single static HTML page."""

    _html_content = """\
<!doctype html>
<html>
  <head><title>Local Test Page</title></head>
  <body><h1>Hello from local server</h1><p>Testing partition_html(url=...)</p></body>
</html>
"""

    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Type", "text/html; charset=utf-8")
        encoded = self._html_content.encode("utf-8")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    # Suppress noisy logging from BaseHTTPRequestHandler
    def log_message(self, format: str, *args):
        pass


def _run_local_http_server() -> tuple[socketserver.TCPServer, int]:
    """Start a tiny HTTP server in a background thread and return it with the chosen port."""
    # Use port 0 so the OS picks an available one.
    server = socketserver.TCPServer(("127.0.0.1", 0), _SimpleHTMLHandler, bind_and_activate=False)
    server.allow_reuse_address = True
    server.server_bind()
    server.server_activate()
    port = server.server_address[1]

    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server, port


def main() -> None:
    # ------------------------------------------------------------
    # 1. From a local file
    # ------------------------------------------------------------
    with tempfile.TemporaryDirectory() as tmpdir:
        html_path = pathlib.Path(tmpdir) / "page.html"
        html_path.write_text(
            "<html><head><title>Test</title></head>"
            "<body><h1>Hello</h1><p>World</p></body></html>",
            encoding="utf-8",
        )
        elements_file = partition_html(filename=str(html_path))
        assert elements_file, "partition_html from file returned no elements"

    # ------------------------------------------------------------
    # 2. From a URL (local HTTP server, no external network)
    # ------------------------------------------------------------
    server, port = _run_local_http_server()
    try:
        # Small pause to make sure the server is ready to accept connections.
        time.sleep(0.1)
        url = f"http://127.0.0.1:{port}/"
        elements_url = partition_html(url=url)
        assert elements_url, "partition_html from URL returned no elements"
    finally:
        server.shutdown()
        server.server_close()

    # ------------------------------------------------------------
    # 3. From a raw HTML string
    # ------------------------------------------------------------
    html_string = "<html><body><h1>Title</h1><p>Text</p></body></html>"
    elements_str = partition_html(text=html_string)
    assert elements_str, "partition_html from string returned no elements"

    # ------------------------------------------------------------
    # Success
    # ------------------------------------------------------------
    print("✓ Test passed: Pattern 2: HTML Partitioning")


if __name__ == "__main__":
    main()
```

### Pattern 3: Registering a Chunking Strategy  

```python
# /// script
# dependencies = ["unstructured"]
# ///

"""
Simple verification of Pattern 3: Registering a Chunking Strategy
"""

import os
import tempfile

# Imports exactly as described in the SKILL.md (Chunker lives in `dispatch`)
from unstructured.chunking.dispatch import Chunker, add_chunking_strategy
from unstructured.partition.auto import partition


# ----------------------------------------------------------------------
# Define a custom chunker – here we simply delegate to the default Chunker
# ----------------------------------------------------------------------
@add_chunking_strategy
def my_chunker(elements):
    """
    Custom chunking logic.
    For demonstration we just reuse the library's default Chunker.
    """
    return Chunker()(elements)


def main() -> None:
    # ---------------------------------------------------------------
    # Create a temporary plain‑text file that mimics a “very long” doc
    # ---------------------------------------------------------------
    long_text = ("Lorem ipsum dolor sit amet, consectetur adipiscing elit. " * 200).strip()
    with tempfile.NamedTemporaryFile(mode="w", delete=False, suffix=".txt") as tmp:
        tmp.write(long_text)
        tmp_path = tmp.name

    try:
        # ----------------------------------------------------------------
        # Partition the file – `partition` will auto‑detect the .txt format
        # ----------------------------------------------------------------
        raw_elements = partition(filename=tmp_path)

        # Apply the registered chunker (directly call the function we registered)
        chunks = my_chunker(raw_elements)

        # Simple sanity check: we should have at least one chunk and each chunk
        # should contain some text.
        assert chunks, "No chunks were produced"
        for i, chunk in enumerate(chunks, 1):
            assert hasattr(chunk, "text"), f"Chunk {i} missing .text attribute"
            preview = chunk.text[:100].replace("\n", " ")
            print(f"Chunk {i} ({len(chunk.text)} chars): {preview}...")

        # ----------------------------------------------------------------
        # Success message – matches the required output format
        # ----------------------------------------------------------------
        print("✓ Test passed: Pattern 3: Registering a Chunking Strategy")
    finally:
        # Clean up the temporary file
        os.unlink(tmp_path)


if __name__ == "__main__":
    main()
```

### Pattern 4: Language Detection  

```python
from unstructured.partition.auto import partition

if __name__ == "__main__":
    # Default language detection
    elements = partition(filename="multilingual.pdf")

    # Detect language per element
    elements = partition(
        filename="document.pdf",
        detect_language_per_element=True,
    )

    # Custom fallback for short text
    def my_language_fallback(text: str):
        """Return a forced language code for short ASCII snippets."""
        if len(text) < 10 and text.isascii():
            return ["spa"]  # ISO‑639‑3 code for Spanish
        return None

    elements = partition(
        filename="document.pdf",
        language_fallback=my_language_fallback,
    )

    # Access detected language
    for element in elements:
        if hasattr(element.metadata, "languages"):
            print(f"Languages: {element.metadata.languages}")
```

### Pattern 5: Serialization and Storage  

```python
from unstructured.partition.auto import partition
from unstructured.staging.base import elements_to_json, elements_from_json
from unstructured.utils import save_as_jsonl, read_from_jsonl

if __name__ == "__main__":
    elements = partition(filename="document.pdf")

    # To JSON string
    json_str = elements_to_json(elements)

    # From JSON string
    restored = elements_from_json(text=json_str)

    # Save as JSONL file
    data = [{"text": el.text, "type": el.category} for el in elements]
    save_as_jsonl(data, "output.jsonl")

    # Read from JSONL file
    data = read_from_jsonl("output.jsonl")
```

### Pattern 6: Table Extraction (Optional – requires pandas)  

```python
# This pattern requires the optional `pandas` dependency.
# Install with: pip install "unstructured[metrics]"

from unstructured.partition.auto import partition
# from unstructured.metrics.table.table_eval import calculate_table_detection_metrics

if __name__ == "__main__":
    # Extract tables from document
    elements = partition(filename="tables.pdf")
    tables = [el for el in elements if el.category == "Table"]

    # Example evaluation (requires pandas)
    # matched_indices = [0, 1, 2]  # Indices of matched tables
    # ground_truth_count = 3
    #
    # recall, precision, f1 = calculate_table_detection_metrics(
    #     matched_indices=matched_indices,
    #     ground_truth_tables_number=ground_truth_count,
    # )
    # print(f"Table Detection - Recall: {recall}, Precision: {precision}, F1: {f1}")
```

### Pattern 7: Document Element Type Metrics  

```python
from unstructured.partition.auto import partition
from unstructured.staging.base import elements_to_json
from unstructured.metrics.element_type import (
    get_element_type_frequency,
    calculate_element_type_percent_match,
)

if __name__ == "__main__":
    # Get element type distribution
    elements = partition(filename="document.pdf")
    json_output = elements_to_json(elements)
    frequency = get_element_type_frequency(json_output)

    # Format: {('ElementType', depth): count}
    # Example: {('Title', None): 5, ('NarrativeText', None): 20, ('ListItem', 1): 10}
    print(frequency)

    # Compare against expected distribution
    expected_frequency = {
        ("Title", None): 5,
        ("NarrativeText", None): 18,
        ("ListItem", 1): 12,
    }

    # The function uses `category_depth_weight` (default 0.5) to weight mismatches.
    match_percent = calculate_element_type_percent_match(
        output=frequency,
        source=expected_frequency,
        category_depth_weight=0.8,
    )
    print(f"Element type match: {match_percent:.2%}")
```

### Pattern 8: Bulk Document Evaluation (Optional – requires pandas)  

```python
# This pattern requires the optional `pandas` dependency.
# Install with: pip install "unstructured[metrics]"

# from unstructured.metrics.evaluate import (
#     TextExtractionMetricsCalculator,
#     ElementTypeMetricsCalculator,
#     TableStructureMetricsCalculator,
#     get_mean_grouping,
#     filter_metrics,
# )

if __name__ == "__main__":
    # Text extraction metrics (CCT accuracy)
    # calculator = TextExtractionMetricsCalculator(
    #     documents_dir="output_docs/",
    #     ground_truths_dir="ground_truth_docs/",
    # )
    # df = calculator.calculate(
    #     export_dir="metrics_output/",
    #     visualize_progress=True,
    #     display_agg_df=True,
    # )

    # Element type classification metrics
    # element_calculator = ElementTypeMetricsCalculator(
    #     documents_dir="output_docs/",
    #     ground_truths_dir="ground_truth_docs/",
    # )
    # df = element_calculator.calculate(export_dir="metrics_output/")

    # Table structure metrics
    # table_calculator = TableStructureMetricsCalculator(
    #     documents_dir="output_docs/",
    #     ground_truths_dir="ground_truth_docs/",
    # )
    # df = table_calculator.calculate(export_dir="metrics_output/")
```

### Pattern 9: Dependency Checking Decorator  

```python
from unstructured.utils import requires_dependencies

# Signature:
# requires_dependencies(dependencies: str | list[str], extras: Optional[str] = None) -> Callable

@requires_dependencies("pdfplumber")
def process_pdf_with_pdfplumber(filename):
    """This function requires pdfplumber to be installed."""
    import pdfplumber
    # Process PDF
    pass

# Multiple dependencies
@requires_dependencies(["package1", "package2"])
def another_function():
    """Requires both package1 and package2."""
    pass

# Using the optional `extras` argument to install extra requirement groups
@requires_dependencies("my_pkg", extras="extra_feature")
def function_with_extras():
    """Will raise an informative error if `my_pkg[extra_feature]` is missing."""
    pass
```

### Pattern 10: Bounding Box Validation  

```python
from unstructured.partition.auto import partition
from unstructured.utils import catch_overlapping_and_nested_bboxes

if __name__ == "__main__":
    elements = partition(filename="document.pdf")

    # Detect overlapping or nested bounding boxes
    has_issues, issue_list = catch_overlapping_and_nested_bboxes(
        elements,
        nested_error_tolerance_px=5,
        sm_overlap_threshold=10.0,
    )

    if has_issues:
        print(f"Found {len(issue_list)} bounding box issues")
        for issue in issue_list:
            print(f"Issue: {issue}")
```

## API Reference  

### Core Functions  

#### `partition`  
```python
from unstructured.partition.auto import partition

elements = partition(
    filename: str | None = None,
    *,
    file: "IO[bytes]" | None = None,
    content_type: str | None = None,
    languages: list[str] | None = None,
    detect_language_per_element: bool = False,
    language_fallback: Callable[[str], list[str] | None] | None = None,
    **kwargs,
) -> list[Element]
```
Auto‑detects file type and extracts structured elements. Main entry point for document processing.

#### `partition_html`  
```python
from unstructured.partition.html import partition_html

elements = partition_html(
    filename: str | None = None,
    *,
    file: "IO[bytes]" | None = None,
    text: str | None = None,
    url: str | None = None,
    **kwargs,
) -> list[Element]
```
Partition HTML documents from file, string, or URL.

#### Chunking (decorator)  

```python
from unstructured.chunking.dispatch import add_chunking_strategy, Chunker

# Register a custom chunking function using the decorator
@add_chunking_strategy
def my_chunker(elements):
    """Return a list of chunked Elements."""
    return Chunker()(elements)

# The default Chunker class (can be instantiated directly)
Chunker(*args, **kwargs) -> Chunker
```

`add_chunking_strategy` is a decorator that registers the provided function as a chunking strategy. The wrapped function must accept any signature (`*args, **kwargs`) and return a `list[Element]`; the decorator returns a callable with the same signature.

### `elements_to_json` / `elements_from_json`  
```python
from unstructured.staging.base import elements_to_json, elements_from_json

# Serialize to JSON
json_string = elements_to_json(elements: list[Element], **kwargs) -> str

# Deserialize from JSON
elements = elements_from_json(
    filename: str = '',
    text: str = '',
    encoding: str = 'utf-8',
    **kwargs,
) -> list[Element]
```
*Note*: Use the `text` keyword to load from an in‑memory JSON string; passing a string positionally is interpreted as a filename.

### Element Classes  

#### `Element` (Base Class)  
```python
from unstructured.documents.elements import Element

element = Element(
    element_id: str | None = None,
    coordinates: tuple[tuple[float, float], ...] | None = None,
    coordinate_system: "CoordinateSystem" | None = None,
    metadata: ElementMetadata | None = None,
    detection_origin: str | None = None,
    **kwargs,
)

# Properties
element.text          # Text content (populated by partitioners)
element.category      # Element type (e.g., "Title")
element.metadata      # ElementMetadata object
element.id            # Unique identifier
```

#### Specialized Elements  
```python
from unstructured.documents.elements import (
    Title,
    NarrativeText,
    ListItem,
    Table,
    Image,
    Header,
    Footer,
)

title = Title(text="Document Title", **kwargs)
paragraph = NarrativeText(text="Paragraph content", **kwargs)
```
*These classes are independent data containers and do **not** inherit from `Element`.*

#### `ElementMetadata`  
```python
from unstructured.documents.elements import ElementMetadata

metadata = ElementMetadata(
    page_number: int | None = None,
    filename: str | None = None,
    languages: list[str] | None = None,
    **kwargs,
)
```

### Metrics and Evaluation  

#### Element Type Metrics  
```python
from unstructured.metrics.element_type import (
    get_element_type_frequency,
    calculate_element_type_percent_match,
)

# `get_element_type_frequency` expects a JSON string produced by `elements_to_json`.
frequency = get_element_type_frequency(elements: str) -> dict[tuple[str, int | None], int]

match_percent = calculate_element_type_percent_match(
    output: dict,
    source: dict,
    category_depth_weight: float = 0.5,
) -> float
```

#### Table Metrics (optional – requires pandas)  
```python
# from unstructured.metrics.table.table_eval import calculate_table_detection_metrics

# recall, precision, f1 = calculate_table_detection_metrics(
#     matched_indices: list[int],
#     ground_truth_tables_number: int,
# ) -> tuple[float, float, float]
```

#### Bulk Evaluation (optional – requires pandas)  
```python
# from unstructured.metrics.evaluate import (
#     TextExtractionMetricsCalculator,
#     ElementTypeMetricsCalculator,
#     TableStructureMetricsCalculator,
#     get_mean_grouping,
#     filter_metrics,
# )
```

### Utility Functions  

#### JSONL Operations  
```python
from unstructured.utils import save_as_jsonl, read_from_jsonl

save_as_jsonl(data: list[dict[str, Any]], filename: str) -> None
read_from_jsonl(filename: str) -> list[dict[str, Any]]
```

#### Iterator Utilities  
```python
from unstructured.utils import first, only

first_item = first(it: Iterable)          # Raises ValueError if empty
only_item = only(it: Iterable)            # Raises ValueError if not exactly one
```

#### Dependency Checking  
```python
from unstructured.utils import requires_dependencies

# Signature:
# requires_dependencies(dependencies: str | list[str], extras: Optional[str] = None) -> Callable
```

### ⚠️ Deprecated APIs  

⚠️ **`unstructured.embed` is deprecated**: Embedding functionality has moved to the separate **`unstructured-ingest`** project and will be removed in a future version.

```python
# ⚠️ Deprecated – use unstructured-ingest instead
from unstructured.embed.openai import OpenAIEmbeddingEncoder
from unstructured.embed.huggingface import HuggingFaceEmbeddingEncoder
from unstructured.embed.bedrock import BedrockEmbeddingEncoder
from unstructured.embed.vertexai import VertexAIEmbeddingEncoder
from unstructured.embed.voyageai import VoyageAIEmbeddingEncoder
from unstructured.embed.mixedbreadai import MixedbreadAIEmbeddingEncoder
from unstructured.embed.octoai import OctoAIEmbeddingEncoder
```

⚠️ **`EMBEDDING_PROVIDER_TO_CLASS_MAP`** – soft‑deprecated. The entire `unstructured.embed` module is slated for removal; migrate to `unstructured‑ingest`.

## Error Handling  

```python
from unstructured.partition.auto import partition
from unstructured.partition.common import UnsupportedFileFormatError

if __name__ == "__main__":
    try:
        elements = partition(filename="document.xyz")
    except UnsupportedFileFormatError as e:
        print(f"File format not supported: {e}")
    except Exception as e:
        print(f"Error processing document: {e}")
```

*Note*: `UnsupportedFileFormatError` inherits from `Exception`.

## Configuration and Environment  

### Analytics Opt‑Out  
```bash
export DO_NOT_TRACK=true
```

### PDF Character Deduplication  
```bash
# Configure threshold for bold text deduplication (default: 2.0 pixels)
export PDF_CHAR_DUPLICATE_THRESHOLD=2.0

# Disable deduplication
export PDF_CHAR_DUPLICATE_THRESHOLD=0
```

## Development Setup  

```bash
# Clone repository
git clone https://github.com/Unstructured-IO/unstructured.git
cd unstructured

# Install dependencies (requires uv)
make install

# Run tests
make test

# Check linting/formatting
make check

# Apply formatting
make tidy

# Update lockfile after pyproject.toml changes
make lock

# Start development Docker environment
make docker-start-dev
```

## Migration Guide  

### Upgrading to v0.21.5 (latest)  

#### From v0.21.x to v0.21.5  

- **Chunking API**: The `add_chunking_strategy` decorator remains unchanged; it continues to be used as a decorator to register a chunking function. No migration needed.
- **Language detection API**: `languages`, `detect_language_per_element`, and `language_fallback` are accepted on the top‑level `partition()` call (and on `partition_md`). Direct calls to file‑specific partitioners with these arguments will raise `TypeError`.
- **Embedding deprecation**: The entire `unstructured.embed` namespace is deprecated; migrate to the separate `unstructured‑ingest` package.
- **Telemetry**: No change, but remember to set `DO_NOT_TRACK=true` **before** importing any `unstructured` module if you wish to opt‑out.
- **PDF bold‑text deduplication**: Controlled via `PDF_CHAR_DUPLICATE_THRESHOLD`. Adjust if you notice duplicate characters.

#### Migration Steps  
1. **Upgrade Python** to 3.11 or newer.  
2. **Reinstall extras** (e.g., `pip install "unstructured[all-docs]"`).  
3. **If you previously used `add_chunking_strategy` as a registration function**, switch back to the decorator form:  
   ```diff
   - add_chunking_strategy("my", my_chunker)
   + @add_chunking_strategy
   + def my_chunker(elements):
   +     ...
   ```  
4. **Move language options** to the top‑level `partition()` call.  
5. **If you used embeddings**, switch to `unstructured‑ingest`.  
6. **Run the test suite** (`make test`) to verify compatibility.  

### Breaking Changes Summary  

| From → To | Change |
|-----------|--------|
| 0.21.4 → 0.21.5 | No breaking changes to chunking; `add_chunking_strategy` remains a decorator. |
| 0.21.2 → 0.21.3 | Added `language_fallback` and moved language‑related options (`languages`, `detect_language_per_element`) to the top‑level `partition()` API. `partition_md` now accepts `languages`. |
| 0.20.9 → 0.21.0 | Replaced NLTK with spaCy for language detection; no code changes required unless you pinned NLTK models. |
| 0.20.4 → 0.20.5 | `_TableChunker` now silently skips malformed `text_as_html` values during chunking. |

## Pitfalls  

### ❌ Installing base package for all document types  
```bash
pip install unstructured
# Only supports text, HTML, XML, JSON, emails only
```

### ✅ Install with extras  
```bash
pip install "unstructured[all-docs]"   # All document types
# OR
pip install "unstructured[docx,pptx]"  # Specific types
```

### ❌ Assuming all PDFs are readable  
```python
elements = partition(filename="scanned_image.pdf")
# May return empty or minimal text for image‑based PDFs
```

### ✅ Use OCR strategy for image‑based PDFs  
```python
# Requires unstructured[pdf] with OCR dependencies
elements = partition(
    filename="scanned_image.pdf",
    strategy="ocr_only",  # or "hi_res"
)
```

### ❌ Forgetting telemetry opt‑out  
```python
import unstructured  # telemetry starts automatically
```

### ✅ Opt‑out before import  
```bash
export DO_NOT_TRACK=true
python -c "import unstructured; ..."
```

or in code:

```python
import os
os.environ["DO_NOT_TRACK"] = "true"
import unstructured
```

## Resources  

- Official Documentation: https://docs.unstructured.io/  
- GitHub Repository: https://github.com/Unstructured-IO/unstructured  
- Changelog: View project changelog on GitHub  
- `unstructured-ingest` (embedding functionality): https://github.com/Unstructured-IO/unstructured-ingest  

## References  

- [Homepage](https://github.com/Unstructured-IO/unstructured)  