---
name: numpy
description: Fundamental n-dimensional array library for numerical computing in Python.
version: 1.26
ecosystem: python
license: BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0
---

## Imports

Show the standard import patterns. Most common first:
```python
import numpy as np

from numpy import array, clip, isin, savetxt, unique
from numpy.random import default_rng
from numpy.typing import NDArray
```

## Core Patterns

**CRITICAL: Prioritize PUBLIC APIs over internal/compat modules**
- Use APIs from api_surface with `publicity_score: "high"` first
- Avoid `.compat`, `.internal`, `._private` modules unless they're the only option
- Example: Prefer `numpy.random` over `numpy._build_utils`

**CRITICAL: Mark deprecation status with clear indicators**

### Create arrays and control dtype ✅ Current
```python
import numpy as np
from numpy.typing import NDArray

def normalize(x: NDArray[np.floating]) -> NDArray[np.floating]:
    # np.array(copy=...) is available in NumPy 1.26
    a = np.array(x, dtype=np.float64, copy=True)
    denom = np.clip(np.linalg.norm(a), 1e-12, np.inf)
    return a / denom

x = np.array([3, 4, 0], dtype=np.float64)
print(normalize(x))
```
* Convert inputs to an `ndarray` with a known dtype and safe copy semantics.
* **Status**: Current, stable

### Boolean masking with `np.isin` + `np.clip` ✅ Current
```python
import numpy as np

values = np.array([1, 2, 3, 4, 5, 6])
mask = np.isin(values, [2, 4, 6])  # membership test
picked = values[mask]

# Clip to a range (common for bounds enforcement)
bounded = np.clip(picked, 0, 4)
print(picked, bounded)
```
* Filter arrays using vectorized membership and enforce bounds.
* **Status**: Current, stable

### Unique values (optionally along an axis) ✅ Current
```python
import numpy as np

a = np.array([3, 3, 2, 1, 2, 2])
u, idx, inv, counts = np.unique(
    a,
    return_index=True,
    return_inverse=True,
    return_counts=True,
)
print("unique:", u)
print("first_index:", idx)
print("inverse:", inv)
print("counts:", counts)
```
* Deduplicate and optionally recover indices, inverse mapping, and counts.
* **Status**: Current, stable

### Random integers with the modern Generator API ✅ Current
```python
import numpy as np

rng = np.random.default_rng(12345)
x = rng.integers(low=0, high=10, size=(2, 5), dtype=np.int64)
print(x)
```
* Use `numpy.random.Generator` (via `default_rng`) for reproducible random generation.
* **Status**: Current, stable

### Save numeric data to text with `np.savetxt` ✅ Current
```python
import numpy as np

data = np.array([[1.5, 2.0], [3.25, 4.75]], dtype=np.float64)

# Save to a file path
np.savetxt("out.csv", data, delimiter=",", header="a,b", comments="")
print(open("out.csv", "r", encoding="utf-8").read())
```
* Write arrays to delimited text formats (CSV/TSV-like).
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- NumPy is mostly configured at build/install time (BLAS/LAPACK, SIMD, etc.).
- Runtime introspection:
  - `numpy.__version__` for the installed version string.
  - `numpy.show_config()` prints build configuration (BLAS/LAPACK, compilers).
  - `numpy.show_runtime()` prints runtime CPU/dispatch info when available.

```python
import numpy as np

print(np.__version__)
np.show_config()
np.show_runtime()
```

Notes:
- Prefer `import numpy as np` and access submodules (e.g., `np.linalg`, `np.random`) via the public namespace; NumPy uses lazy imports for some submodules.

## Pitfalls

### Wrong: Assuming `.item()` works on non-scalar arrays
```python
import numpy as np

a = np.array([1, 2, 3])
print(a.item())  # ValueError: can only convert an array of size 1 to a Python scalar
```

### Right: Use `.item()` only for size-1 arrays, otherwise use `.tolist()`
```python
import numpy as np

scalar_arr = np.array([42])
print(scalar_arr.item())  # 42 (Python int)

a = np.array([1, 2, 3])
print(a.tolist())  # [1, 2, 3]
```

### Wrong: Calling `np.unique` and expecting it to preserve input order
```python
import numpy as np

a = np.array([10, 1, 10, 2])
print(np.unique(a))  # array([ 1,  2, 10]) sorted by default
```

### Right: If you need first-seen order, use `return_index` and reorder
```python
import numpy as np

a = np.array([10, 1, 10, 2])
u, idx = np.unique(a, return_index=True)
ordered = u[np.argsort(idx)]
print(ordered)  # array([10,  1,  2])
```

### Wrong: Using `np.isin` with mismatched shapes and expecting broadcasting like arithmetic
```python
import numpy as np

a = np.array([[1, 2], [3, 4]])
b = np.array([[1, 4], [2, 3]])
# np.isin tests membership in the *flattened* "test_elements" by default, not pairwise
print(np.isin(a, b))  # not a pairwise comparison
```

### Right: Use `np.isin` for membership; for pairwise equality use `==`
```python
import numpy as np

a = np.array([[1, 2], [3, 4]])
b = np.array([[1, 4], [2, 3]])

print(np.isin(a, [1, 4]))  # membership in a set/list
print(a == b)              # pairwise elementwise comparison
```

### Wrong: Creating ambiguous structured dtypes (field layout unclear)
```python
import numpy as np

# Ambiguous/fragile: relying on implicit field names and layout
dt = np.dtype([("x", "i4"), ("y", "i4")])
arr = np.zeros(2, dtype=dt)
print(arr.dtype)
```

### Right: Use explicit structured dtype specifications for clarity
```python
import numpy as np

dt = np.dtype({"names": ["x", "y"], "formats": ["<i4", "<i4"]})
arr = np.zeros(2, dtype=dt)
arr["x"] = [1, 2]
arr["y"] = [10, 20]
print(arr)
print(arr.dtype)
```

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

- [homepage](https://numpy.org)
- [documentation](https://numpy.org/doc/)
- [source](https://github.com/numpy/numpy)
- [download](https://pypi.org/project/numpy/#files)
- [tracker](https://github.com/numpy/numpy/issues)
- [release notes](https://numpy.org/doc/stable/release)

## Migration from v[previous]

What changed in this version (if applicable):
- Breaking changes
  - Benchmark tooling: `asv dev` removed in 1.26.0 → use `asv run`.
- Deprecated → Current mapping
  - Prefer the modern RNG API (`numpy.random.default_rng()` + `Generator` methods like `.integers`) for new code.
- Before/after code examples

```python
# Before (benchmark automation; removed in 1.26.0)
# $ asv dev

# After
# $ asv run
```

## API Reference

Brief reference of the most important public APIs:

- **numpy.array(object, dtype=None, *, copy=True, order='K', subok=False, ndmin=0, like=None)** - Create an `ndarray` from array-like input.
- **numpy.ndarray** - Core n-dimensional array type (supports slicing, broadcasting, vectorized ops).
- **numpy.ndarray.tolist()** - Convert an array to nested Python lists.
- **numpy.ndarray.item()** - Convert a size-1 array to a Python scalar.
- **numpy.clip(a, a_min, a_max, ...)** - Limit values to an interval.
- **numpy.isin(element, test_elements, ...)** - Elementwise membership test.
- **numpy.unique(ar, return_index=False, return_inverse=False, return_counts=False, axis=None, *, equal_nan=True)** - Find unique elements (and optional index/inverse/counts).
- **numpy.pad(array, pad_width, mode='constant', **kwargs)** - Pad an array along its edges.
- **numpy.savetxt(fname, X, ...)** - Save an array to a text file.
- **numpy.random.default_rng(seed=None)** - Create a `Generator` for random numbers.
- **numpy.random.Generator.integers(low, high=None, size=None, dtype=np.int64, endpoint=False)** - Random integers from a range.
- **numpy.linalg** - Linear algebra submodule (e.g., norms, solves, decompositions).
- **numpy.test** - Run NumPy’s test suite for the installed build.
- **numpy.show_config()** - Print build-time configuration.
- **numpy.__version__** - Installed NumPy version string.