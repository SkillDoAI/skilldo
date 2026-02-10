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

## Configuration

Standard configuration and setup:
- Default values for `pd.read_csv()` include `sep=','`, `header='infer'`.
- Common customizations include specifying `dtype`, `na_values` in `pd.read_csv()` or using `index_col` to set an index.

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

### Wrong: Missing Async Await
```python
result = pd.read_csv('data.csv')  # This will fail if read_csv is async
```

### Right: Correct Usage with Async
```python
async def fetch_data():
    result = await pd.read_csv('data.csv')  # Ensure to use await
    return result
```
* Async functions need `await` for asynchronous calls.

### Wrong: Type Hint Gotcha
```python
def process(data: list):
    return pd.Series(data)
```

### Right: Correct Type Hinting
```python
from typing import List

def process(data: List[int]):
    return pd.Series(data)
```
* Using `List` enforces type at runtime.

## References

- [homepage](https://pandas.pydata.org)
- [documentation](https://pandas.pydata.org/docs/)
- [repository](https://github.com/pandas-dev/pandas)

## Migration from v2.x

What changed in this version:
- **Breaking changes**: The behavior of `DataFrame.merge` changed to enforce stricter key matching.
- **Removed**: `DataFrame.append` is no longer available. Use `pd.concat` instead.

## API Reference

Brief reference of the most important public APIs:

- **DataFrame()** - Constructor with key parameters: `data`, `index`, `columns`, `dtype`.
- **Series()** - Creates a one-dimensional labeled array capable of holding any data type.
- **read_csv()** - Reads a comma-separated values (CSV) file into DataFrame.
- **concat()** - Concatenates two or more DataFrames along a particular axis.
- **to_datetime()** - Converts argument to datetime.