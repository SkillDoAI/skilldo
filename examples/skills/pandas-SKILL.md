---
name: pandas
description: A fast, powerful, flexible, and easy-to-use open-source data analysis and manipulation tool.
version: 3.0.0
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
import pandas as pd
from pandas import DataFrame, Series
```

## Core Patterns

### Create a DataFrame ✅ Current
```python
import pandas as pd

# Create a DataFrame from a dictionary
data = {
    'Name': ['Alice', 'Bob', 'Charlie'],
    'Age': [25, 30, 35]
}
df = pd.DataFrame(data)

print(df)
```
* This creates a DataFrame from a dictionary of data.
* **Status**: Current, stable

### Read a CSV File ✅ Current
```python
import pandas as pd

# Read a CSV file into a DataFrame
df = pd.read_csv('data.csv')

print(df)
```
* This reads a CSV file and loads it into a DataFrame.
* **Status**: Current, stable

### Concatenate DataFrames ✅ Current
```python
import pandas as pd

# Create two DataFrames
df1 = pd.DataFrame({'A': [1, 2], 'B': [3, 4]})
df2 = pd.DataFrame({'A': [5, 6], 'B': [7, 8]})

# Concatenate the DataFrames
result = pd.concat([df1, df2])

print(result)
```
* This concatenates two DataFrames vertically.
* **Status**: Current, stable

### Convert to Datetime ✅ Current
```python
import pandas as pd

# Convert a string to a datetime object
date_series = pd.to_datetime(['2023-01-01', '2023-01-02'])

print(date_series)
```
* This converts a list of strings to datetime objects.
* **Status**: Current, stable

### Create a Series ✅ Current
```python
import pandas as pd

# Create a Series from a list
s = pd.Series([1, 2, 3, 4])

print(s)
```
* This creates a Series object from a list of values.
* **Status**: Current, stable

### Work with Missing Values ✅ Current
```python
import pandas as pd

# Create a Series with missing values
s = pd.Series([1, 2, pd.NA, 4, pd.NaT])

# Check for missing values
print(s.isna())

# Fill missing values
filled = s.fillna(0)
print(filled)
```
* Use `pd.NA` for general missing values and `pd.NaT` for missing datetime values.
* **Status**: Current, stable

### Use Categorical Data ✅ Current
```python
import pandas as pd

# Create a Categorical
cat = pd.Categorical(['a', 'b', 'c', 'a', 'b', 'c'])

# Create a Series with categorical dtype
s = pd.Series(['a', 'b', 'c', 'a'], dtype='category')

print(s)
print(s.cat.categories)
```
* Categorical data is useful for memory efficiency and ordered operations.
* **Status**: Current, stable

### Work with Time Series ✅ Current
```python
import pandas as pd

# Create a Timestamp
ts = pd.Timestamp('2023-01-01 12:00:00')

# Create a DatetimeIndex
dates = pd.date_range('2023-01-01', periods=5, freq='D')

# Create a Series with datetime index
s = pd.Series([1, 2, 3, 4, 5], index=dates)

print(s)
```
* Use `Timestamp` and `DatetimeIndex` for time series data.
* **Status**: Current, stable

### Use Configuration Options ✅ Current
```python
import pandas as pd

# Get an option
max_rows = pd.get_option('display.max_rows')

# Set an option
pd.set_option('display.max_rows', 100)

# Use option_context for temporary changes
with pd.option_context('display.max_rows', 10):
    print(df)  # Uses max_rows=10
# Outside context, original setting is restored

# Reset to default
pd.reset_option('display.max_rows')

# Describe options
pd.describe_option('display')
```
* Configuration options control pandas behavior globally or temporarily.
* **Status**: Current, stable

### Read Excel Files ✅ Current
```python
import pandas as pd

# Read an Excel file
df = pd.read_excel('data.xlsx', sheet_name='Sheet1')

# Read specific columns
df = pd.read_excel('data.xlsx', usecols=['Name', 'Age'])

print(df)
```
* Reads Excel files into DataFrames (requires openpyxl or xlrd).
* **Status**: Current, stable

### Work with MultiIndex ✅ Current
```python
import pandas as pd

# Create a MultiIndex
arrays = [
    ['bar', 'bar', 'baz', 'baz'],
    ['one', 'two', 'one', 'two']
]
index = pd.MultiIndex.from_arrays(arrays, names=['first', 'second'])

# Create a Series with MultiIndex
s = pd.Series([1, 2, 3, 4], index=index)

print(s)
print(s['bar'])
```
* MultiIndex enables hierarchical indexing for advanced data structures.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Default values for `pd.read_csv()` include `sep=','`, `header='infer'`.
- Common customizations include specifying `dtype`, `na_values` in `pd.read_csv()` or using `index_col` to set an index.
- Use `pd.set_option()` to configure display settings, computation behavior, and I/O defaults.
- Use `pd.option_context()` for temporary configuration changes within a specific code block.
- Configuration options can be queried with `pd.get_option()` and described with `pd.describe_option()`.

## Pitfalls

### Wrong: Mutable Default Arguments
```python
def create_series(data=[], name='default'):
    return pd.Series(data, name=name)
```

### Right: Correct Approach
```python
def create_series(data=None, name='default'):
    if data is None:
        data = []
    return pd.Series(data, name=name)
```
* Mutable default arguments persist across function calls.

### Wrong: Not Using copy Parameter
```python
# Creates a view, not a copy - changes affect original
df2 = pd.DataFrame(df1)
df2['A'] = 0  # May affect df1
```

### Right: Explicit Copy Control
```python
# Use copy parameter when you need an independent DataFrame
df2 = pd.DataFrame(df1, copy=True)
df2['A'] = 0  # Does not affect df1
```
* In pandas 3.0.0, the `copy` parameter is more explicit in constructors.

### Wrong: Importing Optional Dependencies Directly
```python
import matplotlib.pyplot as plt
# This gives inconsistent error messages
```

### Right: Use pandas Import Helper
```python
from pandas.compat._optional import import_optional_dependency
plt = import_optional_dependency('matplotlib.pyplot')
```
* Provides consistent error messages across pandas.

### Wrong: Not Testing for Missing Optional Dependencies
```python
def plot_data(df):
    import matplotlib.pyplot as plt
    df.plot()
```

### Right: Handle ImportError Properly
```python
def plot_data(df):
    from pandas.compat._optional import import_optional_dependency
    plt = import_optional_dependency('matplotlib.pyplot')
    df.plot()
```
* All methods using optional dependencies should handle ImportError appropriately.

### Wrong: Using Removed APIs
```python
# DataFrame.append was removed in pandas 2.0
df = df.append(new_row)
```

### Right: Use concat Instead
```python
# Use pd.concat for appending rows
df = pd.concat([df, pd.DataFrame([new_row])], ignore_index=True)
```
* `DataFrame.append` is no longer available; use `pd.concat`.

## References

- [homepage](https://pandas.pydata.org)
- [documentation](https://pandas.pydata.org/docs/)
- [repository](https://github.com/pandas-dev/pandas)
- [changelog](https://pandas.pydata.org/pandas-docs/stable/whatsnew/index.html)
- [security policy](https://github.com/pandas-dev/pandas/security/policy)

## Migration from v2.x

What changed in this version:
- **Breaking changes**: This is a major version release that enforces deprecations introduced in the 2.x series. All APIs marked as deprecated in 2.x have been removed.
- **Removed**: `DataFrame.append` is no longer available. Use `pd.concat` instead.
- **Changed**: The `copy` parameter is now more explicit in DataFrame and Series constructors. The default behavior may differ from 2.x.
- **Changed**: Stricter key matching in `DataFrame.merge` and related operations.
- **Version support**: Follows SPEC 0 guideline for Python version support.

### Migration Steps
1. Review all deprecation warnings from your pandas 2.x code.
2. Replace `DataFrame.append()` calls with `pd.concat()`.
3. Explicitly set the `copy` parameter in constructors if you rely on specific copy/view behavior.
4. Test merge operations for stricter key matching behavior.
5. Review the official changelog for your specific minor version upgrade path.

## API Reference

Brief reference of the most important public APIs:

### Core Data Structures
- **DataFrame(data=None, index=None, columns=None, dtype=None, copy=None)** - Two-dimensional, size-mutable, potentially heterogeneous tabular data.
- **Series(data=None, index=None, dtype=None, name=None, copy=None)** - One-dimensional ndarray with axis labels.

### Index Types
- **Index(data=None, dtype=None, copy=False, name=None, tupleize_cols=True)** - Immutable sequence used for indexing and alignment.
- **MultiIndex(levels=None, codes=None, sortorder=None, names=None, dtype=None, copy=False, name=None, verify_integrity=True)** - Multi-level or hierarchical index.
- **RangeIndex(start=None, stop=None, step=None, dtype=None, copy=False, name=None)** - Immutable Index implementing a monotonic integer range.
- **DatetimeIndex(data=None, freq=None, tz=None, normalize=False, closed=None, ambiguous='raise', dayfirst=False, yearfirst=False, dtype=None, copy=False, name=None)** - Immutable ndarray-like of datetime64 data.
- **TimedeltaIndex(data=None, unit=None, freq=None, closed=None, dtype=None, copy=False, name=None)** - Immutable ndarray of timedelta64 data.
- **PeriodIndex(data=None, ordinal=None, freq=None, dtype=None, copy=False, name=None)** - Immutable ndarray holding ordinal values indicating regular periods in time.
- **CategoricalIndex(data=None, categories=None, ordered=None, dtype=None, copy=False, name=None)** - Index based on an underlying Categorical.
- **IntervalIndex(data, closed=None, dtype=None, copy=False, name=None, verify_integrity=True)** - Immutable index of intervals that are closed on the same side.

### Data Types and Special Values
- **Categorical(values, categories=None, ordered=None, dtype=None, fastpath=False)** - Represent a categorical variable.
- **Timestamp(ts_input=None, freq=None, tz=None, unit=None, year=None, month=None, day=None, hour=None, minute=None, second=None, microsecond=None, nanosecond=None, tzinfo=None, fold=None)** - Pandas replacement for python datetime.datetime object.
- **Timedelta(value=None, unit=None, **kwargs)** - Represents a duration, the difference between two dates or times.
- **Period(value=None, freq=None, ordinal=None, year=None, month=None, quarter=None, day=None, hour=None, minute=None, second=None)** - Represents a period of time.
- **Interval(left, right, closed='right')** - Immutable object implementing an Interval, a bounded slice-like interval.
- **NA** - Pandas missing value indicator NA (Not Available).
- **NaT** - Not A Time - pandas equivalent to datetime NaN.

### I/O Functions
- **read_csv(filepath_or_buffer, ...)** - Read a comma-separated values (csv) file into DataFrame.
- **read_excel(io, sheet_name=0, ...)** - Read an Excel file into a pandas DataFrame.
- **read_json(...)** - Read JSON data into DataFrame.

### Data Manipulation
- **concat([df1, df2, ...])** - Concatenates two or more DataFrames along a particular axis.
- **to_datetime(...)** - Converts argument to datetime.

### Configuration
- **get_option(key: str)** - Get configuration option value.
- **set_option(key: str, value)** - Set configuration option.
- **reset_option(key: str)** - Reset configuration option to default.
- **describe_option(key: str = None)** - Print description of configuration option.
- **option_context(*args)** - Context manager for temporarily setting options.

## Development Conventions

### Pre-commit Setup
Always run `pre-commit install` from the repository root after setting up your development environment. This enables automatic style checks before each commit. Style check failures will cause CI to fail.

### Optional Dependencies
Import optional dependencies using `pandas.compat._optional.import_optional_dependency` for consistent error messages. All methods using optional dependencies must include tests asserting ImportError is raised when the dependency is missing.

### Code Quality
- Run `./ci/code_checks.sh` to validate doctests, docstring formatting, and imported modules.
- Use pre-commit hooks for code formatting (ruff, isort, clang-format).
- Code style warnings cause CI tests to fail.

### Versioning
- Semantic versioning: MAJOR.MINOR.PATCH
- API breaking changes only in major releases
- Deprecations introduced in minor releases, enforced in major releases
- Backwards compatibility is maintained where possible