---

name: pandas
description: Data manipulation and analysis library providing DataFrame and Series structures for working with structured data
version: 3.0.0
ecosystem: python
license: BSD-3-Clause
generated_with: claude-sonnet-4-5-20250929
---

## Imports

```python
import pandas as pd
from pandas import DataFrame, Series, Index
from pandas import read_csv, read_excel, read_json, read_sql
from pandas import Timestamp, Timedelta, Period
from pandas import get_option, set_option, option_context
```

## Core Patterns

### Creating DataFrames ✅ Current
```python
import pandas as pd

# From dictionary
df = pd.DataFrame({
    'name': ['Alice', 'Bob', 'Charlie'],
    'age': [25, 30, 35],
    'city': ['NYC', 'SF', 'LA']
})

# From CSV file
df = pd.read_csv('data.csv', index_col=0)

# From Excel file
df = pd.read_excel('data.xlsx', sheet_name='Sheet1')

# From JSON
df = pd.read_json('data.json', orient='records')
```
* DataFrames are two-dimensional labeled data structures with columns of potentially different types
* Use `read_*` functions for loading data from various file formats
* Specify `index_col` to set which column becomes the row index

### Creating Series ✅ Current
```python
import pandas as pd

# From list with index
s = pd.Series([10, 20, 30], index=['a', 'b', 'c'], name='values')

# From dictionary
s = pd.Series({'a': 10, 'b': 20, 'c': 30})

# Extracting from DataFrame
df = pd.DataFrame({'col1': [1, 2, 3], 'col2': [4, 5, 6]})
s = df['col1']  # Returns Series
```
* Series are one-dimensional labeled arrays capable of holding any data type
* Index provides labels for fast lookup
* Series maintain their name attribute for identification

### Data Selection and Filtering ✅ Current
```python
import pandas as pd

df = pd.DataFrame({
    'name': ['Alice', 'Bob', 'Charlie'],
    'age': [25, 30, 35],
    'score': [85, 90, 88]
})

# Select single column (returns Series)
ages = df['age']

# Select multiple columns (returns DataFrame)
subset = df[['name', 'age']]

# Boolean filtering
adults = df[df['age'] >= 30]

# Multiple conditions
high_scorers = df[(df['age'] >= 25) & (df['score'] >= 85)]

# Using .loc for label-based indexing
row = df.loc[0]  # First row by index label
value = df.loc[0, 'name']  # Specific cell

# Using .iloc for position-based indexing
row = df.iloc[0]  # First row by position
value = df.iloc[0, 1]  # First row, second column
```
* Use bracket notation for column selection
* Boolean indexing filters rows based on conditions
* `.loc[]` uses labels, `.iloc[]` uses integer positions
* Combine conditions with `&` (and), `|` (or), `~` (not) - wrap each condition in parentheses

### Working with Time Series ✅ Current
```python
import pandas as pd

# Create Timestamp
ts = pd.Timestamp('2024-01-15 14:30:00')
ts = pd.Timestamp(year=2024, month=1, day=15, hour=14, minute=30)

# Create DatetimeIndex
dates = pd.date_range('2024-01-01', periods=10, freq='D')
df = pd.DataFrame({'value': range(10)}, index=dates)

# Create Timedelta
td = pd.Timedelta('2 days')
td = pd.Timedelta(days=2, hours=3)

# Create TimedeltaIndex
deltas = pd.timedelta_range(start='1 day', periods=5, freq='D')

# Create Period
p = pd.Period('2024-01', freq='M')

# Create PeriodIndex
periods = pd.period_range('2024-01', periods=12, freq='M')
```
* `Timestamp` replaces Python's datetime.datetime with nanosecond precision
* `DatetimeIndex` enables time-based indexing and slicing
* `Timedelta` represents duration between two dates or times
* `Period` represents a span of time at a particular frequency

### Configuring Display Options ✅ Current
```python
import pandas as pd

# Get current option value
max_rows = pd.get_option('display.max_rows')

# Set option value
pd.set_option('display.max_rows', 100)
pd.set_option('display.max_columns', 50)
pd.set_option('display.precision', 2)

# Temporarily set options with context manager
with pd.option_context('display.max_rows', 10, 'display.max_columns', 5):
    print(df)  # Uses temporary settings

# Outside context, original settings restored

# Reset option to default
pd.reset_option('display.max_rows')

# Describe available options
pd.describe_option('display')  # All display options
pd.describe_option('display.max_rows')  # Specific option
```
* Use `get_option()` and `set_option()` for global configuration changes
* `option_context()` provides temporary settings that restore automatically
* Common options: `display.max_rows`, `display.max_columns`, `display.precision`, `display.width`

## Configuration

### Display Settings
```python
# Default values
pd.get_option('display.max_rows')  # 60
pd.get_option('display.max_columns')  # 20
pd.get_option('display.width')  # 80
pd.get_option('display.precision')  # 6

# Common customizations
pd.set_option('display.max_rows', None)  # Show all rows
pd.set_option('display.max_columns', None)  # Show all columns
pd.set_option('display.float_format', '{:.2f}'.format)  # Format floats
```

### File Reading Options
```python
# CSV reading with common parameters
df = pd.read_csv(
    'data.csv',
    sep=',',  # Delimiter (default: ',')
    header=0,  # Row to use as column names (default: 'infer')
    index_col=0,  # Column to use as row index
    usecols=['col1', 'col2'],  # Columns to read
    dtype={'col1': int, 'col2': str},  # Column data types
    parse_dates=['date_col'],  # Parse as datetime
    na_values=['NA', 'null'],  # Additional NA values
    encoding='utf-8',  # File encoding
    nrows=1000,  # Number of rows to read
    skiprows=5  # Rows to skip at start
)
```

### Index and Data Types
```python
# Creating typed indexes
idx = pd.Index([1, 2, 3], dtype='int64', name='id')
cat_idx = pd.CategoricalIndex(['A', 'B', 'C'], name='category')
range_idx = pd.RangeIndex(start=0, stop=10, step=2)
multi_idx = pd.MultiIndex.from_tuples([('A', 1), ('A', 2), ('B', 1)])

# Creating categoricals
cat = pd.Categorical(['A', 'B', 'A', 'C'], categories=['A', 'B', 'C'], ordered=True)

# Creating intervals
interval = pd.Interval(left=0, right=5, closed='right')
interval_idx = pd.IntervalIndex.from_breaks([0, 1, 2, 3])
```

## Pitfalls

### Wrong: Using chained assignment
```python
import pandas as pd
df = pd.DataFrame({'A': [1, 2, 3], 'B': [4, 5, 6]})

# This may not work as expected and raises SettingWithCopyWarning
df[df['A'] > 1]['B'] = 99
```
**Why:** Chained indexing creates intermediate copies, so assignment may not affect the original DataFrame.

### Right: Use .loc for assignment
```python
import pandas as pd
df = pd.DataFrame({'A': [1, 2, 3], 'B': [4, 5, 6]})

# Correctly modifies the original DataFrame
df.loc[df['A'] > 1, 'B'] = 99
```

---

### Wrong: Comparing DataFrame/Series directly to NaN
```python
import pandas as pd
import numpy as np

df = pd.DataFrame({'A': [1, np.nan, 3]})

# This doesn't work - NaN != NaN by definition
null_rows = df[df['A'] == np.nan]  # Returns empty DataFrame
```
**Why:** NaN is not equal to itself, so equality comparisons always return False.

### Right: Use .isna() or .notna() methods
```python
import pandas as pd
import numpy as np

df = pd.DataFrame({'A': [1, np.nan, 3]})

# Correctly identifies null values
null_rows = df[df['A'].isna()]
non_null_rows = df[df['A'].notna()]
```

---

### Wrong: Iterating over DataFrame rows with loops
```python
import pandas as pd

df = pd.DataFrame({'A': [1, 2, 3], 'B': [4, 5, 6]})

# Very slow for large DataFrames
results = []
for i in range(len(df)):
    results.append(df.iloc[i]['A'] + df.iloc[i]['B'])
```
**Why:** Row-by-row iteration is extremely slow and defeats pandas' vectorization.

### Right: Use vectorized operations
```python
import pandas as pd

df = pd.DataFrame({'A': [1, 2, 3], 'B': [4, 5, 6]})

# Much faster - operates on entire columns at once
df['result'] = df['A'] + df['B']
```

---

### Wrong: Not specifying dtype when creating structures
```python
import pandas as pd

# Mixed types cause object dtype (slow operations)
df = pd.DataFrame({'id': ['1', '2', '3'], 'value': [10, 20, 30]})
# df['id'].dtype is 'object', not efficient
```
**Why:** Object dtype prevents optimized operations and uses more memory.

### Right: Specify dtypes explicitly or convert after creation
```python
import pandas as pd

# Specify dtype at creation
df = pd.DataFrame({
    'id': pd.Series([1, 2, 3], dtype='int64'),
    'value': [10, 20, 30]
})

# Or convert after creation
df = pd.DataFrame({'id': ['1', '2', '3'], 'value': [10, 20, 30]})
df['id'] = df['id'].astype('int64')
```

---

### Wrong: Using inplace=True for method chaining
```python
import pandas as pd

df = pd.DataFrame({'A': [1, 2, 3], 'B': [4, 5, 6]})

# Cannot chain - inplace returns None
result = df.drop(columns=['B'], inplace=True).reset_index()  # Error!
```
**Why:** Methods with `inplace=True` return None, breaking method chains.

### Right: Avoid inplace, assign results
```python
import pandas as pd

df = pd.DataFrame({'A': [1, 2, 3], 'B': [4, 5, 6]})

# Chain operations naturally
result = df.drop(columns=['B']).reset_index(drop=True)

# Or assign back if needed
df = df.drop(columns=['B'])
```

## References

- [homepage](https://pandas.pydata.org)
- [documentation](https://pandas.pydata.org/docs/)
- [repository](https://github.com/pandas-dev/pandas)
- [changelog](https://pandas.pydata.org/pandas-docs/stable/whatsnew/index.html)
- [PyPI](https://pypi.org/project/pandas/)

## Migration from v2.x

### Copy Semantics Changed
**v2.x behavior:**
```python
df = pd.DataFrame({'A': [1, 2, 3]})
df2 = df[['A']]  # Creates view in many cases
df2.iloc[0, 0] = 99  # May modify original df
```

**v3.0 behavior:**
```python
df = pd.DataFrame({'A': [1, 2, 3]})
df2 = df[['A']]  # Always creates copy
df2.iloc[0, 0] = 99  # Never modifies original df
```
**Migration:** If you need a view, explicitly use `.copy()` for clarity or review Copy-on-Write behavior changes.

### Deprecated Parameters Removed
Several long-deprecated parameters have been removed in v3.0:
- `infer_datetime_format` in `read_csv()` and similar functions
- Various `convert_*` parameters

**Migration:** Remove these parameters from your code. Check deprecation warnings from v2.x for specific guidance.

### API Breaking Changes
Refer to the full changelog for comprehensive breaking changes: https://pandas.pydata.org/pandas-docs/stable/whatsnew/v3.0.0.html

Key areas to review:
- Index and MultiIndex behavior changes
- DataFrame/Series constructor changes
- IO function parameter updates
- Deprecated method removals

## API Reference

### Core Data Structures
- **DataFrame(data=None, index=None, columns=None, dtype=None, copy=None)** - Two-dimensional labeled data structure with columns of potentially different types
- **Series(data=None, index=None, dtype=None, name=None, copy=None)** - One-dimensional labeled array capable of holding any data type

### Index Types
- **Index(data=None, dtype=None, copy=False, name=None)** - Immutable sequence used for indexing and alignment
- **RangeIndex(start=None, stop=None, step=None)** - Memory-efficient index for monotonic integer ranges
- **MultiIndex(levels=None, codes=None, names=None)** - Multi-level or hierarchical index object
- **DatetimeIndex(data=None, freq=None, tz=None)** - Immutable ndarray of datetime64 data
- **CategoricalIndex(data=None, categories=None, ordered=None)** - Index based on categorical data
- **IntervalIndex(data, closed=None)** - Index of intervals closed on the same side

### Scalars
- **Timestamp(ts_input=None, year=None, month=None, day=None, tz=None)** - Pandas replacement for datetime.datetime with nanosecond precision
- **Timedelta(value=None, unit=None)** - Duration representing difference between two dates or times
- **Period(value=None, freq=None)** - Represents a time period at a particular frequency
- **Interval(left, right, closed='right')** - Immutable object representing an interval

### Data Types
- **Categorical(values, categories=None, ordered=None)** - Represents categorical variable for memory efficiency and operations

### IO Functions
- **read_csv(filepath_or_buffer, sep=None, header='infer', index_col=None, dtype=None)** - Read CSV file into DataFrame
- **read_excel(io, sheet_name=0, header=0, index_col=None)** - Read Excel file into DataFrame
- **read_json(path_or_buf, orient=None, dtype=None, lines=False)** - Convert JSON to DataFrame or Series
- **read_sql(sql, con, index_col=None, parse_dates=None)** - Read SQL query or table into DataFrame

### Configuration
- **get_option(pat)** - Get value of a single configuration option
- **set_option(pat, value)** - Set value of a single configuration option
- **reset_option(pat)** - Reset option to default value
- **option_context(*args)** - Context manager for temporary option changes
- **describe_option(pat='')** - Print description for registered options