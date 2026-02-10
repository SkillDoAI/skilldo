---
name: arrow
version: 1.4.0
ecosystem: python
# license: Unknown
---

```markdown
---
name: arrow
description: A Python library for working with dates and times with a focus on timezone-aware operations and a fluent API.
version: 1.4.0
ecosystem: python
license: MIT
---

## Imports

```python
# Core factory functions and Arrow class
import arrow
from arrow import Arrow, now, utcnow, get

# For parsing and error handling
from arrow.parser import DateTimeParser, ParserError

# For custom Arrow types
from arrow.factory import ArrowFactory
```

## Core Patterns

### Creating Arrow Objects ✅ Current

```python
import arrow

# Current time in UTC
utc_now = arrow.utcnow()

# Current time in local timezone
local_now = arrow.now()

# Current time in specific timezone
pacific_now = arrow.now('US/Pacific')

# From datetime values
arr = arrow.get(2013, 5, 5, 12, 30, 45)

# From string with format
arr = arrow.get('2013-05-05 12:30:45', 'YYYY-MM-DD HH:mm:ss')

# From ISO string (auto-detected)
arr = arrow.get('2013-05-05T12:30:45+00:00')

# From Unix timestamp
arr = arrow.get(1367746245, 'X')

# From datetime object
from datetime import datetime
dt = datetime(2013, 5, 5, 12, 30, 45)
arr = arrow.get(dt, 'UTC')
```

* **Status**: Current, stable API for all date/time creation
* These are the primary entry points for creating Arrow objects
* Arrow objects are timezone-aware and immutable
* `arrow.utcnow()` preferred over `datetime.datetime.utcnow()`

### Manipulating Dates and Times ✅ Current

```python
import arrow

arr = arrow.utcnow()

# Shift by time units (returns new Arrow object)
future = arr.shift(hours=2, minutes=30)
past = arr.shift(days=-1, hours=-6)

# Replace specific components
arr2 = arr.replace(hour=14, minute=0, second=0)

# Clone for creating independent copies
arr3 = arr.clone()

# Add/subtract time
tomorrow = arr.add(days=1)
last_week = arr.subtract(weeks=1)
```

* **Status**: Current, stable API
* Arrow objects are immutable - `shift()`, `replace()`, `add()`, `subtract()` return new objects
* Must capture return values: `arr = arr.shift(hours=1)` not `arr.shift(hours=1)`
* `shift()` accepts multiple parameters; `replace()` for setting absolute values; `add()`/`subtract()` for time deltas

### Timezone Conversion ✅ Current

```python
import arrow

utc_time = arrow.utcnow()

# Convert to different timezone (changes wall time, same instant)
pacific = utc_time.to('US/Pacific')
paris = utc_time.to('Europe/Paris')

# Convert using timezone object
from dateutil import tz
eastern = utc_time.to(tz.gettz('US/Eastern'))

# Also works with pytz
import pytz
tokyo = utc_time.to(pytz.timezone('Asia/Tokyo'))

# Get the underlying datetime and tzinfo
dt = pacific.datetime  # Python datetime object
tzinfo = pacific.tzinfo  # tzinfo object
naive_dt = pacific.naive  # Naive (tz-unaware) datetime
```

* **Status**: Current, stable API
* `.to()` converts to target timezone (same instant in time, different wall time)
* Accepts string timezone names, `dateutil.tz`, `pytz`, or `ZoneInfo` objects
* Use `.datetime` property to get underlying Python datetime
* All Arrow objects maintain timezone information throughout their lifetime

### Formatting and Parsing ✅ Current

```python
import arrow

arr = arrow.utcnow()

# Format with Arrow tokens (not strptime format!)
formatted = arr.format('YYYY-MM-DD HH:mm:ss ZZ')  # 2024-01-15 14:30:45 +0000
iso_format = arr.format('YYYY-MM-DDTHH:mm:ssZZ')  # 2024-01-15T14:30:45+0000
us_format = arr.format('MM/DD/YYYY')  # 01/15/2024

# Parse with single format
arr2 = arrow.get('2024-01-15', 'YYYY-MM-DD')

# Parse with multiple format options (tries each in order)
arr3 = arrow.get('15/01/2024', ['DD/MM/YYYY', 'MM/DD/YYYY'])

# Humanized relative time
now = arrow.utcnow()
past = now.shift(hours=-2)
print(past.humanize(now))  # "2 hours ago"

future = now.shift(days=5, hours=3)
print(future.humanize(now, granularity='hour'))  # "5 days and 3 hours"

# Parse humanized strings (reverse of humanize)
arr4 = arrow.utcnow()
arr5 = arr4.dehumanize('2 days ago')
```

* **Status**: Current, stable API
* **CRITICAL**: Use Arrow tokens (`YYYY-MM-DD`) not strptime tokens (`%Y-%m-%d`)
* Arrow tokens: `YYYY` (4-digit year), `YY` (2-digit), `MM` (month), `DD` (day), `HH` (hour), `mm` (minute), `ss` (second), `ZZ` (timezone offset)
* `.humanize()` returns relative time string; `granularity` controls precision
* `.dehumanize()` parses human-readable strings (v1.1.0+)

### Time Ranges and Spans ✅ Current

```python
import arrow

start = arrow.get(2024, 1, 1, 0, 0, 0)
end = arrow.get(2024, 1, 31, 23, 59, 59)

# Iterate over hours
for hour in arrow.Arrow.range('hour', start, end):
    print(hour.format('YYYY-MM-DD HH:00:00'))

# Iterate over days with step
for day in arrow.Arrow.range('day', start, end, 3):  # Every 3 days
    print(day.format('YYYY-MM-DD'))

# Get span of time period (beginning and end)
now = arrow.utcnow()
day_start, day_end = now.span('day')  # Start and end of today
month_start, month_end = now.span('month')
year_start, year_end = now.span('year')

# Floor and ceil for single boundary
week_floor = now.floor('week')  # Start of current week
hour_ceil = now.ceil('hour')  # End of current hour
```

* **Status**: Current, stable API
* `.range()` is a class method for iterating over time periods
* `.span()` returns tuple of (start, end) Arrow objects
* `.floor()` and `.ceil()` return single Arrow object
* Valid frames: `'second'`, `'minute'`, `'hour'`, `'day'`, `'week'`, `'month'`, `'quarter'`, `'year'`

### Custom Arrow Types ✅ Current

```python
from arrow import Arrow, ArrowFactory

# Create custom Arrow subclass with additional behavior
class CustomArrow(Arrow):
    def is_weekend(self):
        """Check if date is on weekend"""
        return self.weekday() >= 5  # Saturday=5, Sunday=6
    
    def business_days_until(self, other):
        """Count business days between dates"""
        count = 0
        current = self.clone()
        while current <= other:
            if current.weekday() < 5:  # Monday=0 to Friday=4
                count += 1
            current = current.shift(days=1)
        return count

# Use custom type with ArrowFactory
factory = ArrowFactory(CustomArrow)

# Now factory methods return CustomArrow instances
today = factory.utcnow()
print(today.is_weekend())  # Access custom methods

# Get from factory
arr = factory.get('2024-01-15', 'YYYY-MM-DD')
```

* **Status**: Current, stable API
* Subclass `Arrow` to add custom methods
* Use `ArrowFactory` to bind custom Arrow type to factory methods
* Factory returns instances of custom type for `.utcnow()`, `.now()`, `.get()`

## Configuration

### Parser Configuration

```python
from arrow.parser import DateTimeParser

# Create parser with custom locale and cache
parser = DateTimeParser(locale='fr_FR', cache_size=100)

# Parse with custom parser instance
result = parser.parse('15 janvier 2024', 'DD MMMM YYYY')

# Disable caching (cache_size=0)
parser_no_cache = DateTimeParser(cache_size=0)

# Default locale is 'en_us'; custom locales require babel
```

* **Default cache size**: 10 (LRU cache for regex patterns)
* **Supported locales**: All locales supported by Babel (en_US, fr_FR, de_DE, etc.)
* **Cache disabling**: Set `cache_size=0` to disable caching (slower but useful for debugging)

### DST and Fold Parameter

```python
import arrow

# During DST transitions, some times occur twice (ambiguous)
# Use fold parameter to disambiguate: fold=0 (first occurrence), fold=1 (second)

# Example: Europe/Paris transition on 2019-10-27 at 3:00 AM
# 2:30 AM occurs twice - use fold to specify which one

arr1 = arrow.Arrow(2019, 10, 27, 2, 30, 0, 
                   tzinfo='Europe/Paris', fold=0)  # First 2:30 AM
arr2 = arrow.Arrow(2019, 10, 27, 2, 30, 0, 
                   tzinfo='Europe/Paris', fold=1)  # Second 2:30 AM

# Also use fold in replace()
arr3 = arrow.utcnow().replace(fold=1)
```

* **Fold parameter**: 0 (first occurrence, default) or 1 (second occurrence during DST fall-back)
* Only relevant during DST transitions when local time repeats
* Without specifying fold during ambiguous times, behavior may be inconsistent

## Pitfalls

### Wrong: Using naive datetimes instead of Arrow

```python
from datetime import datetime

# Naive datetime - loses timezone information
dt = datetime(2024, 1, 15, 14, 30, 0)  # What timezone is this?
# Comparisons and conversions become ambiguous
```

### Right: Always use Arrow or specify timezone explicitly

```python
import arrow
from datetime import datetime
from dateutil import tz

# Arrow - always timezone-aware
arr = arrow.get(2024, 1, 15, 14, 30, 0)  # UTC by default

# Or provide timezone when creating from datetime
dt = datetime(2024, 1, 15, 14, 30, 0)
arr = arrow.get(dt, 'US/Pacific')

# Use arrow.utcnow() instead of datetime.utcnow()
now = arrow.utcnow()  # Timezone-aware
```

### Wrong: Forgetting to capture return value from shift/replace

```python
import arrow

arr = arrow.utcnow()
arr.shift(hours=1)  # This creates new object but doesn't reassign arr
# arr is still the original time!
print(arr)  # Still utcnow, not shifted
```

### Right: Capture the return value or chain methods

```python
import arrow

arr = arrow.utcnow()

# Reassign to new value
arr = arr.shift(hours=1)

# Or chain methods
result = arrow.utcnow().shift(hours=1).to('US/Pacific')
```

### Wrong: Using strptime format tokens instead of Arrow tokens

```python
import arrow

# This fails - arrow uses YYYY-MM-DD not %Y-%m-%d
arr = arrow.get('2024-01-15', '%Y-%m-%d')  # ParserError!
```

### Right: Use Arrow's custom format tokens

```python
import arrow

# Arrow tokens: YYYY (year), MM (month), DD (day), HH (hour), mm (minute), ss (second)
arr = arrow.get('2024-01-15', 'YYYY-MM-DD')

arr2 = arrow.get('15/01/2024 14:30:45', 'DD/MM/YYYY HH:mm:ss')

# Format with Arrow tokens too
formatted = arr.format('YYYY-MM-DD HH:mm:ssZZ')  # 2024-01-15 00:00:00+0000
```

### Wrong: Forgetting timezone conversion when displaying in different timezone

```python
import arrow

utc_time = arrow.utcnow()
# Just reassigning without converting doesn't change the time
local = utc_time  # Still UTC, not converted!
print(local)  # Will show as UTC+0000
```

### Right: Use .to() to convert timezone

```python
import arrow

utc_time = arrow.utcnow()

# Convert to local timezone
local = utc_time.to('local')

# Or specific timezone
pacific = utc_time.to('US/Pacific')

# All three refer to same instant, different wall times
print(f"UTC: {utc_time}")  # 2024-01-15 14:00:00+00:00
print(f"Local: {local}")  # 2024-01-15 06:00:00-08:00 (example)
```

### Wrong: Parsing with list of formats without handling all formats

```python
import arrow

# If none of the formats match, ParserError is raised
arr = arrow.get('15 Jan 2024', ['YYYY-MM-DD', 'MM/DD/YYYY'])  # ParserError - no match
```

### Right: Provide all possible format variations or catch ParserError

```python
import arrow
from arrow.parser import ParserError

# Include all possible formats
arr = arrow.get('15 Jan 2024', [
    'YYYY-MM-DD',
    'MM/DD/YYYY',
    'DD MMM YYYY'  # This one matches
])

# Or use try/except for robustness
try:
    arr = arrow.get(date_string, ['YYYY-MM-DD', 'MM/DD/YYYY'])
except ParserError:
    print(f"Could not parse: {date_string}")
```

## References

- [Documentation](https://arrow.readthedocs.io)
- [Source](https://github.com/arrow-py/arrow)
- [Issues](https://github.com/arrow-py/arrow/issues)

## Migration from v0.15.x to v1.4.0

### Breaking Changes

**Arrow objects are now timezone-aware by default**

```python
# OLD (v0.15.x) - Naive datetime returned
import arrow
arr = arrow.get('2024-01-15')  # Naive datetime object

# NEW (v1.4.0) - Timezone-aware (UTC assumed)
import arrow
arr = arrow.get('2024-01-15')  # Arrow object, UTC timezone
```

**Format/parse tokens changed from strptime to Arrow custom format**

```python
# OLD (v0.15.x)
arr.format('%Y-%m-%d %H:%M:%S')
arr = arrow.get('2024-01-15', '%Y-%m-%d')

# NEW (v1.4.0)
arr.format('YYYY-MM-DD HH:mm:ss')
arr = arrow.get('2024-01-15', 'YYYY-MM-DD')
```

**Timezone parameter now required or inferred**

```python
# OLD (v0.15.x)
from datetime import datetime
dt = datetime(2024, 1, 15, 14, 30, 0)
arr = arrow.get(dt)  # Ambiguous timezone

# NEW (v1.4.0)
from datetime import datetime
dt = datetime(2024, 1, 15, 14, 30, 0)
arr = arrow