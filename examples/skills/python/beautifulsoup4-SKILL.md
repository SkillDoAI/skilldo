---
name: beautifulsoup4
description: A Python library for parsing and navigating HTML and XML documents
license: MIT License
metadata:
  version: "4.14.3"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
import bs4
from bs4 import BeautifulSoup, Tag, Comment
from bs4.exceptions import FeatureNotFound, ParserRejectedMarkup
from bs4.dammit import UnicodeDammit
```

## Core Patterns

### Parse markup with an explicit parser ✅ Current
```python
from __future__ import annotations

from bs4 import BeautifulSoup

html_doc = "<html><body><p class='body strikeout'>Hello</p></body></html>"

# Always choose the parser explicitly for consistent behavior across environments.
soup = BeautifulSoup(html_doc, "html.parser")

p = soup.find("p")
assert p is not None
print(p.name)          # "p"
print(p.get_text())    # "Hello"
```
* Prefer `BeautifulSoup(markup, "html.parser")`, `"lxml"`, `"html5lib"`, or `"xml"/"lxml-xml"` depending on your needs; different parsers can produce different trees for invalid documents.

### Parse from a file handle (context manager) ✅ Current
```python
from __future__ import annotations

from pathlib import Path
from bs4 import BeautifulSoup

path = Path("example.html")
path.write_text("<html><body><a href='/x'>Link</a></body></html>", encoding="utf-8")

with path.open("r", encoding="utf-8") as fp:
    soup = BeautifulSoup(fp, "html.parser")

a = soup.find("a")
assert a is not None
print(a.get("href"))  # "/x"
```
* Pass an open file handle directly to `BeautifulSoup` to let the builder stream/handle encodings appropriately.

### Find elements and navigate relatives ✅ Current
```python
from __future__ import annotations

from typing import Optional
from bs4 import BeautifulSoup, Tag

html_doc = """
<div id="root">
  <h1>Title</h1>
  <p>First</p>
  <p>Second <span>inner</span></p>
</div>
"""

soup = BeautifulSoup(html_doc, "html.parser")

root: Optional[Tag] = soup.find(id="root")
assert root is not None

h1: Optional[Tag] = root.find("h1")
assert h1 is not None

# Navigate
second_p: Optional[Tag] = h1.find_next("p")
assert second_p is not None
print(second_p.get_text(strip=True))  # "First"

all_ps = root.find_all("p")
print([p.get_text(" ", strip=True) for p in all_ps])  # ["First", "Second inner"]
```
* Use `find`, `find_all`, and the `find_next*` / `find_previous*` / sibling / parent variants for tree navigation.

### Work with tag attributes (including multi-valued `class`) ✅ Current
```python
from __future__ import annotations

from bs4 import BeautifulSoup, Tag

soup = BeautifulSoup("<p id='x' class='body strikeout'></p>", "html.parser")
p = soup.find("p")
assert isinstance(p, Tag)

# Dict-like access
print(p["id"])         # "x"
print(p.get("id"))     # "x"

# Multi-valued HTML attributes like class are lists by default.
print(p["class"])      # ["body", "strikeout"]

# If you always want a list (even for non-multivalued attrs), use get_attribute_list.
print(p.get_attribute_list("id"))     # ["x"]
print(p.get_attribute_list("class"))  # ["body", "strikeout"]

# Mutation
p["data-role"] = "demo"
del p["id"]
print(p.attrs)  # {'class': ['body', 'strikeout'], 'data-role': 'demo'}
```
* In HTML mode, `class`, `rel`, etc. are typically stored as `list[str]`. Use `Tag.get_attribute_list(key, default=None)` to normalize to a list.

### Handle text nodes and comments safely ✅ Current
```python
from __future__ import annotations

from bs4 import BeautifulSoup, Comment
from bs4.element import NavigableString

soup = BeautifulSoup("<p>Hello<!--secret--></p>", "html.parser")
p = soup.find("p")
assert p is not None

# Comments are special text nodes.
comment = p.find(string=lambda s: isinstance(s, Comment))
assert isinstance(comment, Comment)
print(comment)  # "secret"

# NavigableString is immutable; replace the node instead of editing in place.
text = p.find(string=lambda s: isinstance(s, NavigableString) and not isinstance(s, Comment))
assert isinstance(text, NavigableString)
text.replace_with("Hi")

print(p.get_text())  # "Hi"
```
* Treat `NavigableString` as immutable; use `replace_with(...)` to change text.

### CSS selector search ✅ Current
```python
from __future__ import annotations

from typing import Optional
from bs4 import BeautifulSoup, Tag

html_doc = """
<div id="main">
  <p class="story">First</p>
  <p class="story">Second</p>
  <a href="/x" class="link">Link</a>
</div>
"""

soup = BeautifulSoup(html_doc, "html.parser")

# select_one returns the first match or None
first: Optional[Tag] = soup.select_one("p.story")
assert first is not None
print(first.get_text())  # "First"

# select returns all matches
stories = soup.select("p.story")
print([p.get_text() for p in stories])  # ["First", "Second"]

# Scoped to a subtree
main: Optional[Tag] = soup.find(id="main")
assert main is not None
links = main.select("a.link")
print([a.get("href") for a in links])  # ["/x"]
```
* `select` and `select_one` are convenience wrappers; for the full Soup Sieve API (`match`, `filter`, `closest`, `iselect`), use the `.css` property.

### CSS‑Sieve API via `.css` ✅ New
```python
from __future__ import annotations

from bs4 import BeautifulSoup, Tag

html = """
<ul class="menu">
  <li data-id="1">Home</li>
  <li data-id="2">About</li>
  <li data-id="3">Contact</li>
</ul>
"""

soup = BeautifulSoup(html, "html.parser")

# Use the full Soup Sieve API
first_item: Tag | None = soup.css.select_one("li[data-id='2']")
assert first_item is not None
print(first_item.get_text())  # "About"

# Filter a ResultSet with a CSS selector
all_items = soup.css.select("li")
print([li.get_text() for li in all_items])  # ["Home", "About", "Contact"]
```
* The `.css` attribute gives direct access to the Soup Sieve API; it supersedes the older `select` shortcuts for complex queries.

### Build and modify a tree ✅ Current
```python
from __future__ import annotations

from typing import Optional
from bs4 import BeautifulSoup, Tag

soup = BeautifulSoup("<ul></ul>", "html.parser")
ul = soup.find("ul")
assert ul is not None

# Create new tags and strings
for text in ("Alpha", "Beta", "Gamma"):
    li = soup.new_tag("li")
    li.string = text
    ul.append(li)

print(soup.prettify())
# <ul>
#  <li>Alpha</li>
#  <li>Beta</li>
#  <li>Gamma</li>
# </ul>

# Remove an element
first_li: Optional[Tag] = ul.find("li")
assert first_li is not None
first_li.decompose()
print(len(ul.find_all("li")))  # 2
```

### Encode/decode output ✅ Current
```python
from __future__ import annotations

from bs4 import BeautifulSoup

soup = BeautifulSoup("<p>café</p>", "html.parser")

# str output (Unicode)
html_str: str = soup.decode()
print(type(html_str))  # <class 'str'>

# bytes output
html_bytes: bytes = soup.encode("utf-8")
print(type(html_bytes))  # <class 'bytes'>

# Pretty‑printed str
print(soup.prettify())
```

### Using HTMLFormatter / XMLFormatter ✅ New
```python
from __future__ import annotations

from bs4 import BeautifulSoup
from bs4.formatter import HTMLFormatter, XMLFormatter

html = "<div><p>Text</p></div>"
soup = BeautifulSoup(html, "html.parser")

# Instantiate a formatter (constructors return a formatter instance)
formatter = HTMLFormatter()

# Pass the formatter to encode or prettify
encoded = soup.encode(formatter=formatter)
print(encoded)  # b'<div><p>Text</p></div>'

pretty = soup.prettify(formatter=formatter)
print(pretty)
```
* Formatter constructors return a formatter instance; create the formatter object and pass it via the `formatter=` argument to `encode` or `prettify`.

## Configuration

- **Parser selection (`features`)**:
  - `"html.parser"`: built‑in, decent baseline.
  - `"lxml"`: fast (requires `lxml`).
  - `"html5lib"`: most lenient (slow; requires `html5lib`).
  - `"xml"` / `"lxml-xml"`: XML parsing mode (attribute handling differs from HTML).
- **`parse_only`**: pass a `SoupStrainer` to parse only parts of a document for speed/memory.
- **`from_encoding` / `exclude_encodings`**: hint or restrict encoding detection when input is bytes.
- **Large text nodes with lxml**: when using an lxml builder and documents may contain a single text node > 10 000 000 bytes, pass `huge_tree=True` to `BeautifulSoup(...)` to avoid lxml security limits truncating the parse.
- **Multi‑valued attributes**:
  - Default (HTML): `class`, `rel` etc. become lists.
  - To disable list conversion: `BeautifulSoup(markup, "html.parser", multi_valued_attributes=None)`
  - In XML mode, multi‑valued attributes are not enabled by default; you can opt in via `multi_valued_attributes={'*': ['class']}`.
- **`.css` property**: entry point for the full Soup Sieve CSS selector API (`soup.css.select(...)`, `soup.css.select_one(...)`, `soup.css.match(...)`, `soup.css.filter(...)`, `soup.css.closest(...)`, `soup.css.iselect(...)`).

## Pitfalls

### Wrong: Not specifying a parser (inconsistent trees)
```python
from bs4 import BeautifulSoup

html_doc = "<p><b>badly nested</p></b>"
soup = BeautifulSoup(html_doc)  # parser not specified — raises GuessedAtParserWarning
print(soup.find("b"))
```

### Right: Choose a parser explicitly
```python
from bs4 import BeautifulSoup

html_doc = "<p><b>badly nested</p></b>"
soup = BeautifulSoup(html_doc, "html.parser")
print(soup.find("b"))
```

### Wrong: Treating `class` as a string in HTML mode
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<p class='body strikeout'></p>", "html.parser")
# In HTML mode, soup.p["class"] is a list, so this fails.
classes = soup.p["class"].split()  # type: ignore[attr-defined]
print(classes)
```

### Right: Use the list directly (or normalize with `get_attribute_list`)
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<p class='body strikeout'></p>", "html.parser")
classes = soup.p["class"]
print(classes)  # ["body", "strikeout"]

ids = soup.p.get_attribute_list("id")
print(ids)  # []
```

### Wrong: Assuming multi‑valued attributes exist in XML mode
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<p class='body strikeout'></p>", "xml")
# In XML mode, "class" is a string by default; indexing returns a character.
first = soup.p["class"][0]
print(first)  # "b" (not "body")
```

### Right: Opt in to multi‑valued attributes when parsing XML
```python
from bs4 import BeautifulSoup

class_is_multi = {"*": ["class"]}
soup = BeautifulSoup(
    "<p class='body strikeout'></p>", "xml", multi_valued_attributes=class_is_multi
)
first = soup.p["class"][0]
print(first)  # "body"
```

### Wrong: Editing a `NavigableString` "in place"
```python
from bs4 import BeautifulSoup
from bs4.element import NavigableString

soup = BeautifulSoup("<p>Hello</p>", "html.parser")
text = soup.p.string
assert isinstance(text, NavigableString)

# Strings are immutable; this does not update the parse tree.
text = NavigableString("Hi")
print(soup.p.get_text())  # still "Hello"
```

### Right: Replace the existing node with `replace_with`
```python
from bs4 import BeautifulSoup
from bs4.element import NavigableString

soup = BeautifulSoup("<p>Hello</p>", "html.parser")
text = soup.p.string
assert isinstance(text, NavigableString)

text.replace_with("Hi")
print(soup.p.get_text())  # "Hi"
```

### Wrong: lxml builder truncation with huge text nodes (missing `huge_tree=True`)
```python
from bs4 import BeautifulSoup

# If this markup contains a single >10,000,000 byte text node, lxml may stop early.
markup_with_huge_text = "<root>" + ("x" * 11_000_000) + "</root>"
soup = BeautifulSoup(markup_with_huge_text, "lxml")
print(soup.find("root") is not None)
```

### Right: Enable huge tree support when needed
```python
from bs4 import BeautifulSoup

markup_with_huge_text = "<root>" + ("x" * 11_000_000) + "</root>"
soup = BeautifulSoup(markup_with_huge_text, "lxml", huge_tree=True)
print(soup.find("root") is not None)
```

### Wrong: Accessing a `<css>` tag via shorthand dot notation (4.12.0+)
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<css id='x'></css>", "html.parser")
# soup.css is now the Soup Sieve API entry point, not the <css> tag.
print(soup.css["id"])  # AttributeError or unexpected result
```

### Right: Use `find()` to access a `<css>` tag by name
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<css id='x'></css>", "html.parser")
css_tag = soup.find("css")
print(css_tag["id"])  # "x"
```

### Wrong: Storing `NavigableString` outside the parse tree without converting
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<b>Hello</b>", "html.parser")
# Holding a NavigableString keeps the entire parse tree alive in memory.
my_text = soup.b.string
del soup
# my_text still holds a reference to the tree
```

### Right: Convert to `str` before storing outside the tree
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<b>Hello</b>", "html.parser")
my_text = str(soup.b.string)  # plain Python str, no tree reference
del soup
print(my_text)  # "Hello"
```

## References

- [Download](https://www.crummy.com/software/BeautifulSoup/bs4/download/)
- [Homepage](https://www.crummy.com/software/BeautifulSoup/bs4/)

### Migration from v4.13.x

- **Typing changes (4.14.0+)**: `find_*` methods gained overloads to improve type safety.
  - Prefer annotating results as `Optional[Tag]`, `Optional[NavigableString]`, `Sequence[Tag]`, etc.
  - Known edge case: `find_all("a", string="...")` may still confuse type checkers; refactor or use `typing.cast`.

```python
from __future__ import annotations

from typing import Optional, Sequence, cast
from bs4 import BeautifulSoup, Tag

soup = BeautifulSoup("<a>b</a>", "html.parser")

# Preferred: reflect optionality
a: Optional[Tag] = soup.find("a")

# Edge case: mixed filters may require a cast for static type checkers
tags = cast(Sequence[Tag], soup.find_all("a", string="b"))
print([t.get_text() for t in tags])
```

- **`ResultSet` typing churn across 4.14.x**: inheritance changed in 4.14.0/4.14.1/4.14.2; avoid depending on specific ABC inheritance.
  - If you need a stable container type at boundaries: `results = list(soup.find_all(...))`.

- **lxml huge text nodes (4.14.3 note)**: if using an lxml builder and expecting extremely large text nodes, pass `huge_tree=True`.

### Migration from v4.11.x / v4.12.x

- **`.css` property (4.12.0+)**: `soup.css` and `tag.css` are now reserved for the Soup Sieve CSS selector API. Replace any `soup.css` or `tag.css` shorthand access to a `<css>` element with `soup.find("css")`.
- **`copy.copy()` / `copy.deepcopy()` (4.12.1+)**: copying a `BeautifulSoup` object no longer re‑parses the document; it copies the existing tree. For a fresh parse, create a new `BeautifulSoup` instance from the original markup string.
- **Deep tree serialization (4.12.1+)**: `str(soup)` on very deeply nested documents no longer raises `RecursionError`; non‑recursive serialization is used automatically.
- **`BeautifulStoneSoup`** ⚠️ **Deprecated (hard)**: use `BeautifulSoup(markup, features="xml")` instead.
- **`soup.find(text=…)` parameter** ⚠️ **Deprecated**: use `string=` instead.

### API Reference

- **BeautifulSoup(markup, features=…, parse_only=…, from_encoding=…, exclude_encodings=…, element_classes=…, **kwargs)** — parse markup into a tree; specify `features` (parser) explicitly. `markup` may be `str`, `bytes`, or a file‑like `IO` object.
- **BeautifulSoup.find(name=None, attrs={}, recursive=True, string=None, **kwargs)** — return the first matching element (`Tag | None`); supports tag name, attrs, and other filters.
- **BeautifulSoup.find_all(name=None, attrs={}, recursive=True, string=None, limit=None, **kwargs)** — return all matching elements (`ResultSet`); convert to `list(...)` for a stable container type. `limit=0` means no limit.
- **BeautifulSoup.find_next(...) / find_all_next(...)** — search forward in document order from a starting node.
- **BeautifulSoup.find_previous(...) / find_all_previous(...)** — search backward in document order from a starting node.
- **BeautifulSoup.find_next_sibling(...) / find_next_siblings(...)** — search among following siblings.
- **BeautifulSoup.find_previous_sibling(...) / find_previous_siblings(...)** — search among preceding siblings.
- **BeautifulSoup.find_parent(...) / find_parents(...)** — search upward to parents/ancestors.
- **BeautifulSoup.select(selector, namespaces=None, limit=None, **kwargs)** — CSS selector search; returns `ResultSet`. Convenience wrapper over `soup.css.select(...)`.
- **BeautifulSoup.select_one(selector, namespaces=None, **kwargs)** — CSS selector search; returns first match or `None`. Convenience wrapper over `soup.css.select_one(...)`.
- **BeautifulSoup.css** — entry point for the full Soup Sieve API (`select`, `select_one`, `match`, `filter`, `closest`, `iselect`, `escape`, `compile`).
- **BeautifulSoup.get_text(separator="...", strip=False)** — extract combined text content from a subtree.
- **BeautifulSoup.prettify()** — render formatted markup for debugging/inspection.
- **BeautifulSoup.decode(indent_level=None, eventual_encoding='utf-8', formatter='minimal', iterator=None, **kwargs) -> str** — render the tree as a Unicode string. *(Signature matches current implementation.)*
- **BeautifulSoup.encode(encoding=None, formatter=None, **kwargs)** — render tree as bytes. *(Signature updated to match current implementation.)*
- **BeautifulSoup.new_tag(name, namespace=None, nsprefix=None, attrs={}, sourceline=None, sourcepos=None, **kwattrs)** — create a new `Tag` node not yet attached to the tree.
- **BeautifulSoup.new_string(s, subclass=None)** — create a new `NavigableString` (or subclass) node.
- **BeautifulSoup.contains_replacement_characters** — flag indicating replacement characters were introduced during encoding handling (builder‑dependent).
- **Tag.name** — the tag's name (e.g., `"a"`, `"p"`).
- **Tag.attrs** — dict of attributes; multi‑valued HTML attributes may be lists.
- **Tag.get(key, default=None)** — safe attribute lookup.
- **Tag.get_attribute_list(key, default=None)** — normalize an attribute to a list regardless of internal storage; returns a list (empty if attribute missing).
- **Tag.string** — the sole `NavigableString` child of this tag, or `None` if there are multiple children or none.
- **Tag.contents** — list of direct children.
- **Tag.decode_contents(formatter=None, **kwargs)** — render inner content as a Unicode string (excluding the tag's own open/close tags). *(Signature updated to match current implementation.)*
- **Tag.decompose()** — remove the tag and all its contents from the tree and destroy them.
- **Tag.extract()** — remove the tag from the tree and return it.
- **UnicodeDammit(...)** — helper for detecting/decoding unknown encodings before parsing.
- **Comment** — class for HTML/XML comment nodes (a specialized string‑like node).
- **ParserRejectedMarkup** — exception raised when the underlying parser rejects markup.
- **FeatureNotFound** — exception raised when the requested parser feature/builder is unavailable.
- **GuessedAtParserWarning** — warning raised when no parser is specified and Beautiful Soup guesses.
- **MarkupResemblesLocatorWarning** — warning raised when markup looks like a filename or URL rather than markup.
- **BeautifulStoneSoup** ⚠️ **Deprecated (soft)** — use `BeautifulSoup(markup, features="xml")` instead.

## Pitfalls (continued)

### Wrong: Using the undocumented `.hidden` attribute
```python
from bs4 import BeautifulSoup

soup = BeautifulSoup("<p>Hello</p>", "html.parser")
soup.p.hidden = True  # ⚠️ Undocumented; behavior changed in 4.12.3
```
**Why**: `.hidden` is not part of the public API and its semantics were altered, leading to unpredictable results.

**Right**: Remove the tag or set a CSS style instead.
```python
soup.p.decompose()               # Remove entirely
# or
soup.p["style"] = "display:none"  # Hide via CSS
```

### Wrong: Copying a soup object expecting a fresh parse
```python
import copy
from bs4 import BeautifulSoup

orig = BeautifulSoup("<p>Hi</p>", "html.parser")
clone = copy.copy(orig)  # ⚠️ No re‑parse; both share the same tree structure
```
**Why**: Since 4.12.1 `copy.copy`/`copy.deepcopy` produce a shallow clone of the existing tree; any mutation affects both objects.

**Right**: Re‑parse the original markup when a fresh tree is needed.
```python
clone = BeautifulSoup(str(orig), "html.parser")
```

## Migration Guide (4.12 → latest)

1. **`.css` property** – If your XML documents contain a literal `<css>` element, replace any `soup.css` attribute access with the classic selector methods (`soup.select_one('css')` or `soup.find('css')`).  
2. **Copy semantics** – `copy.copy`/`copy.deepcopy` now produce shallow clones. Re‑parse markup when a fresh parse is required.  
3. **Formatter constructors** – `HTMLFormatter()` and `XMLFormatter()` return formatter instances. Create the formatter and pass it via the `formatter=` argument to `encode` or `prettify`.  
4. **`.hidden` attribute** – Remove any usage; it is undocumented and its behavior changed in 4.12.3. Use CSS or tag removal instead.  
5. **`BeautifulStoneSoup`** ⚠️ – Deprecated; replace with `BeautifulSoup(markup, features="xml")`.  
6. **`soup.find(text=…)`** – Deprecated; use `string=` instead.

### Example Migration
```python
# Before 4.12.0
soup = BeautifulSoup(xml_data, "xml")
value = soup.css['id']   # broke if <css> tag existed

# After 4.12.0
soup = BeautifulSoup(xml_data, "xml")
value = soup.select_one('css')['id']   # works for any <css> element
```

### Additional Resources
* Full changelog: https://bazaar.launchpad.net/~leonardr/beautifulsoup/bs4/view/head:/CHANGELOG  
* Migration guide in the documentation: https://www.crummy.com/software/BeautifulSoup/bs4/doc/#porting-code-to-bs4  

---  

*All listed APIs are those that appear in the public documentation (Sphinx `.. py:class::`, `.. py:attribute::`, and code examples). Private helpers (prefixed with an underscore) are intentionally omitted.*