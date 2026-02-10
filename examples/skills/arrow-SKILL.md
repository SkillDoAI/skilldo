---

name: arrow
description: Date, time, and timezone handling with an Arrow datetime type plus parsing/formatting helpers.
version: 1.4.0
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import arrow
from arrow import Arrow, ArrowFactory, get, now, utcnow
from arrow.formatter import FORMAT_RFC3339, FORMAT_RFC3339_STRICT, FORMAT_RFC2822
```

## Core Patterns

### Create Arrow instances (now/utcnow/get) ✅ Current
```python
import arrow

# Current time in UTC (timezone-aware)
a_utc = arrow.utcnow()

# Current time in a specific timezone (IANA tz name or "local"/"utc")
a_local = arrow.now("local")
a_pacific = arrow.now("US/Pacific")

# Parse ISO-8601-like strings (Arrow will infer many common formats)
a_iso = arrow.get("2024-01-15T10:30:00+02:00")

# Avoid asserting exact offsets for zones with DST; instead, assert tzinfo exists.
# Also avoid relying on tzinfo.key being present across tzinfo implementations.
assert a_utc.tzinfo is not None
assert a_pacific.tzinfo is not None

print(a_utc)
print(a_local)
print(a_pacific)
print(a_iso)
```
* Prefer `arrow.utcnow()` / `arrow.now()` / `arrow.get(...)` as the primary constructors.

### Parse non-ISO strings with Arrow tokens ✅ Current
```python
import arrow

# Arrow tokens (NOT datetime.strptime tokens)
a = arrow.get("2013-05-05 12:30:45", "YYYY-MM-DD HH:mm:ss")

# Formatting with Arrow tokens
s1 = a.format("YYYY-MM-DD")
s2 = a.format("YYYY-MM-DD HH:mm:ss")

print(s1)
print(s2)
```
* When parsing/formatting custom strings, use Arrow token strings (e.g., `YYYY-MM-DD`), not `%Y-%m-%d`.

### Shift/replace/to for time arithmetic and timezone handling ✅ Current
```python
import arrow

a = arrow.get("2024-02-01T12:00:00Z")

# Relative offsets (returns a new Arrow)
b = a.shift(days=+7, hours=-3)

# Set specific components (returns a new Arrow)
c = a.replace(hour=9, minute=15, second=0, microsecond=0)

# Convert the same instant to another timezone
d = a.to("Europe/Paris")

print("a:", a)
print("b:", b)
print("c:", c)
print("d:", d)
```
* Use `.shift(...)` for relative offsets and `.replace(...)` for setting components.
* Use `.to("Zone/Name")` to convert the same instant into another timezone.

### Humanize with a stable reference time (especially in tests) ✅ Current
```python
import arrow

present = arrow.utcnow()
past = present.shift(hours=-1, minutes=-5)

# Stable output: compare against an explicit reference time
print(past.humanize(present))
print(past.humanize(present, only_distance=True))
```
* Pass a reference time to `humanize(...)` to avoid time-sensitive test failures.
* `only_distance=True` omits “ago”/“in”.

### Use ArrowFactory for Arrow subclasses ✅ Current
```python
from __future__ import annotations

import arrow
from arrow import Arrow

class CustomArrow(Arrow):
    """Arrow subclass for project-specific helpers."""
    def iso_date(self) -> str:
        return self.format("YYYY-MM-DD")

custom = arrow.ArrowFactory(CustomArrow)

a = custom.utcnow()
print(type(a).__name__)
print(a.iso_date())
```
* For custom behavior, subclass `arrow.Arrow` and build a `arrow.ArrowFactory(CustomArrow)`.

## Configuration

- **Timezones**
  - `arrow.utcnow()` returns a timezone-aware Arrow in UTC.
  - `arrow.now(tz)` accepts common timezone expressions such as `"utc"`, `"local"`, or IANA names like `"Europe/Paris"`, `"US/Pacific"`.
  - Convert instants with `Arrow.to(tz)`. Reinterpret wall time (change tz metadata without converting the instant) with `Arrow.replace(tzinfo=...)` when that is explicitly desired.
- **DST ambiguity (fold)**
  - For ambiguous local times during DST fall-back, pass `fold=0` or `fold=1` when constructing/replacing an `Arrow` (e.g., `Arrow(..., fold=0)` then `.replace(fold=1)`).
- **Formatting constants**
  - Common formatter constants are available (e.g., `arrow.formatter.FORMAT_RFC3339`, `FORMAT_RFC2822`, `FORMAT_ATOM`) for consistent output formats.

## Pitfalls

### Wrong: Using `strptime`-style tokens with `arrow.get()` / `Arrow.format()`
```python
import arrow

# Looks reasonable but uses datetime.strptime tokens, not Arrow tokens
a = arrow.get("2013-05-05 12:30:45", "%Y-%m-%d %H:%M:%S")
print(a.format("%Y-%m-%d"))
```

### Right: Use Arrow tokens (YYYY, MM, DD, etc.)
```python
import arrow

a = arrow.get("2013-05-05 12:30:45", "YYYY-MM-DD HH:mm:ss")
print(a.format("YYYY-MM-DD"))
```

### Wrong: Using `replace(tzinfo=...)` when you mean “convert the instant”
```python
import arrow

arw = arrow.utcnow()

# This reinterprets wall time in US/Pacific (changes the instant)
pacific_wrong = arw.replace(tzinfo="US/Pacific")
print(arw)
print(pacific_wrong)
```

### Right: Use `.to(...)` to convert the same instant to another timezone
```python
import arrow

arw = arrow.utcnow()

pacific = arw.to("US/Pacific")
print(arw)
print(pacific)
```

### Wrong: Ignoring DST ambiguity for an ambiguous local time
```python
import arrow

# Ambiguous time during fall-back; fold not specified
paris = arrow.Arrow(2019, 10, 27, 2, 0, 0, tzinfo="Europe/Paris")
print(paris)
```

### Right: Specify `fold` and switch with `.replace(fold=...)`
```python
import arrow

paris_early = arrow.Arrow(2019, 10, 27, 2, 0, 0, tzinfo="Europe/Paris", fold=0)
paris_late = paris_early.replace(fold=1)

print(paris_early)
print(paris_late)
```

### Wrong: Unstable `humanize()` assertions that depend on “now”
```python
import arrow

present = arrow.utcnow()
past = arrow.utcnow().shift(hours=-1)

# Output can vary depending on runtime timing/rounding
print(past.humanize())
```

### Right: Provide an explicit reference time
```python
import arrow

present = arrow.utcnow()
past = present.shift(hours=-1)

print(past.humanize(present))
```

## References

- [Documentation](https://arrow.readthedocs.io)
- [Source](https://github.com/arrow-py/arrow)
- [Issues](https://github.com/arrow-py/arrow/issues)

## Migration from v[previous]

No version-specific breaking changes were provided in the inputs. If migrating between Arrow versions:
- Ensure parsing/formatting uses **Arrow tokens** (e.g., `YYYY-MM-DD`) rather than `strptime` tokens.
- Prefer `.to(...)` for timezone conversion; reserve `.replace(tzinfo=...)` for explicit reinterpretation.
- Audit DST fall-back behavior and set `fold` where ambiguous local times matter.

## API Reference

- **arrow.__version__** - Library version string.
- **arrow.get(*args, **kwargs)** - Primary constructor/parser for Arrow instances (supports strings, datetimes, timestamps, formats).
- **arrow.utcnow()** - Current time as a timezone-aware Arrow in UTC.
- **arrow.now(tz: Optional[TZ_EXPR] = None)** - Current time in a specified timezone (`"utc"`, `"local"`, IANA name, etc.).
- **arrow.Arrow(...)** - Arrow datetime type (timezone-aware); key params: `year, month, day, hour, minute, second, microsecond, tzinfo`.
- **Arrow.now(tzinfo: Optional[tzinfo] = None)** - Classmethod variant of “now”.
- **Arrow.utcnow()** - Classmethod variant of “utcnow”.
- **Arrow.fromtimestamp(timestamp, tzinfo=None)** - Build from Unix timestamp (int/float/str) with optional timezone.
- **Arrow.utcfromtimestamp(timestamp)** - Build from Unix timestamp interpreted in UTC.
- **Arrow.fromdatetime(dt, tzinfo=None)** - Build from `datetime.datetime` with optional timezone override.
- **Arrow.fromdate(date, tzinfo=None)** - Build from `datetime.date` with optional timezone.
- **Arrow.strptime(date_str, fmt, tzinfo=None)** - Parse with a specified format string.
- **arrow.ArrowFactory(type: Type[Arrow] = Arrow)** - Factory for producing Arrow objects (useful for Arrow subclasses).
- **arrow.ParserError** - Raised for parsing failures.
- **arrow.formatter.FORMAT_RFC3339 / FORMAT_RFC3339_STRICT / FORMAT_RFC2822 / FORMAT_ATOM** - Common predefined format strings.