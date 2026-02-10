---

name: scipy
description: Scientific computing library for numerical routines including sparse matrices, optimization, FFTs, and linear algebra.
version: 1.18.0
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import scipy as sp
import numpy as np

from scipy import sparse
from scipy.fft import fft
from scipy.optimize import differential_evolution
from scipy.sparse import coo_array, csr_array, csc_array
from scipy.sparse import linalg as splinalg
from scipy.sparse import csgraph
```

## Core Patterns

### Build sparse arrays in the right format ✅ Current
```python
import numpy as np
import scipy as sp
from scipy.sparse import coo_array, csr_array

# Construct via coordinates (COO is good for construction/modification)
row = np.array([0, 0, 1, 2], dtype=np.int64)
col = np.array([0, 2, 1, 2], dtype=np.int64)
data = np.array([1.0, 3.0, 2.0, 4.0], dtype=np.float64)

A_coo = coo_array((data, (row, col)), shape=(3, 3))

# Canonicalize (sum duplicates if any) before heavy use
A_coo.sum_duplicates()

# Convert to CSR for fast arithmetic / matvec / row slicing
A = A_coo.tocsr()

x = np.array([1.0, 2.0, 3.0])
y = A @ x

print("A.nnz:", A.nnz)
print("y:", y)
```
* Prefer constructing sparse arrays directly (e.g., `coo_array((data, (row, col)))`) rather than densifying first.
* Use COO for construction; convert to CSR/CSC for efficient operations.

### Clean sparse structure: remove explicit zeros and duplicates ✅ Current
```python
import numpy as np
from scipy.sparse import csr_array, coo_array

# Explicit zeros are stored entries and count toward nnz
row = np.array([0, 0, 1], dtype=np.int64)
col = np.array([0, 1, 1], dtype=np.int64)
data = np.array([1.0, 0.0, 2.0], dtype=np.float64)

A = csr_array((data, (row, col)), shape=(2, 2))
print("nnz before:", A.nnz)

A.eliminate_zeros()
print("nnz after eliminate_zeros:", A.nnz)

# Duplicates in COO are stored separately until summed
row2 = np.array([0, 0], dtype=np.int64)
col2 = np.array([0, 0], dtype=np.int64)
data2 = np.array([1.0, 3.0], dtype=np.float64)
B = coo_array((data2, (row2, col2)), shape=(1, 1))
print("B.nnz before:", B.nnz)

B.sum_duplicates()
print("B.nnz after sum_duplicates:", B.nnz)
print("B.todense():", B.todense())
```
* Use `csr_array.eliminate_zeros()` to drop stored zeros when you mean “no entry”.
* Use `coo_array.sum_duplicates()` to canonicalize coordinate duplicates.

### Sparse reductions and dense outputs ✅ Fixed
```python
import numpy as np
from scipy.sparse import csr_array

A = csr_array(
    (
        np.array([1.0, 2.0, 3.0]),
        (np.array([0, 1, 1]), np.array([0, 0, 2])),
    ),
    shape=(3, 3),
)

# Axis reductions on sparse arrays can return different dense container types
# depending on SciPy version (ndarray, matrix, etc.). To get predictable shapes,
# convert to a dense ndarray first, then reduce.
A_dense = A.toarray()

row_means = A_dense.mean(axis=1, keepdims=True)  # (m, 1)
col_max = A_dense.max(axis=0, keepdims=True)     # (1, n)

overall_max = A_dense.max()
argmax_flat = int(A_dense.argmax())

print("row_means shape:", row_means.shape, "value:\n", row_means)
print("col_max shape:", col_max.shape, "value:\n", col_max)
print("overall_max:", overall_max)
print("argmax (flattened):", argmax_flat)
```
* Expect reductions like `mean(axis=...)` / `max(axis=...)` to produce dense results, but don’t rely on an exact container/shape across versions—coerce with `np.asarray(...)` and reshape when needed.
* Use `nnz` to inspect stored structure, not the number of nonzero values in a dense sense (explicit zeros can exist).

### FFT for 1D signals ✅ Current
```python
import numpy as np
from scipy.fft import fft

# Real-valued time series
t = np.linspace(0.0, 1.0, 256, endpoint=False)
x = np.sin(2.0 * np.pi * 10.0 * t) + 0.5 * np.sin(2.0 * np.pi * 40.0 * t)

X = fft(x)  # complex frequency-domain representation
print("FFT length:", X.shape[0])
print("First 5 bins:", X[:5])
```
* Use `scipy.fft.fft` for FFT computations; output is complex-valued.

### Global optimization with differential evolution ✅ Current
```python
from __future__ import annotations

import numpy as np
from scipy.optimize import differential_evolution

def objective(v: np.ndarray) -> float:
    x, y = float(v[0]), float(v[1])
    return (x - 1.0) ** 2 + (y + 2.0) ** 2

bounds = [(-5.0, 5.0), (-5.0, 5.0)]

result = differential_evolution(objective, bounds=bounds, seed=0)
print("x*:", result.x)
print("f(x*):", result.fun)
print("success:", result.success)
```
* `scipy.optimize.differential_evolution` minimizes an objective over bounds; use `seed=` for reproducibility.

## Configuration

- **Threading defaults**
  - SciPy itself is typically single-threaded.
  - Many linear algebra operations call into BLAS/LAPACK (often via NumPy) and are commonly **multi-threaded** by default.
- **Controlling BLAS/LAPACK threads**
  - Use the external package `threadpoolctl` to cap BLAS/LAPACK thread usage when needed (e.g., to avoid oversubscription when you add your own parallelism).
- **Opt-in parallelism**
  - Some SciPy APIs expose parallel execution via a `workers=` keyword argument (accepting an integer worker count, and for some APIs a map-like callable). Do not assume parallelism unless you explicitly enable it.
- **Sparse array format selection**
  - COO: construction/modification; CSR: fast arithmetic/matvec and row slicing; CSC: column slicing; DOK/LIL: incremental construction; DIA: diagonal structure; BSR: block structure.
  - Operations may change output format for efficiency; convert explicitly when you need a specific format.

## Pitfalls

### Wrong: Indexing a format that is not subscriptable (`dia_array`) 
```python
import numpy as np
import scipy as sp

dense = np.array([[1, 0], [0, 2]])
a = sp.sparse.coo_array(dense)

# Not all sparse formats support indexing/slicing
b = a.todia()
print(b[1, 1])  # TypeError: 'dia_array' object is not subscriptable
```

### Right: Convert to an indexable format (e.g., CSR) before subscripting
```python
import numpy as np
import scipy as sp

dense = np.array([[1, 0], [0, 2]])
a = sp.sparse.coo_array(dense)

print(a.tocsr()[1, 1])
```

### Wrong: Assuming sparse operations preserve the input format
```python
import numpy as np
import scipy as sp

x = sp.sparse.coo_array(np.eye(3))
y = x @ x.T

# SciPy may return CSR/CSC for efficiency; don't assume y is COO
print(type(y))
```

### Right: Convert explicitly to the format you require downstream
```python
import numpy as np
import scipy as sp

x = sp.sparse.coo_array(np.eye(3))
y = x @ x.T

y_csr = y.tocsr()
print(type(y_csr))
```

### Wrong: Treating explicit zeros as “no entry”
```python
import numpy as np
import scipy as sp

row = np.array([0, 0], dtype=np.int64)
col = np.array([0, 1], dtype=np.int64)
data = np.array([1.0, 0.0], dtype=np.float64)

A = sp.sparse.csr_array((data, (row, col)), shape=(1, 2))
print("nnz:", A.nnz)  # counts explicit zeros too
```

### Right: Remove stored zeros when you intend “no stored entry”
```python
import numpy as np
import scipy as sp

row = np.array([0, 0], dtype=np.int64)
col = np.array([0, 1], dtype=np.int64)
data = np.array([1.0, 0.0], dtype=np.float64)

A = sp.sparse.csr_array((data, (row, col)), shape=(1, 2))
A.eliminate_zeros()
print("nnz:", A.nnz)
```

### Wrong: Assuming duplicates in COO are automatically merged
```python
import numpy as np
import scipy as sp

row = np.array([0, 0], dtype=np.int64)
col = np.array([0, 0], dtype=np.int64)
data = np.array([1.0, 3.0], dtype=np.float64)

A = sp.sparse.coo_array((data, (row, col)), shape=(1, 1))
assert A.nnz == 1  # fails: nnz is 2 because duplicates are stored separately
```

### Right: Canonicalize with `sum_duplicates()` before relying on `nnz`/structure
```python
import numpy as np
import scipy as sp

row = np.array([0, 0], dtype=np.int64)
col = np.array([0, 0], dtype=np.int64)
data = np.array([1.0, 3.0], dtype=np.float64)

A = sp.sparse.coo_array((data, (row, col)), shape=(1, 1))
A.sum_duplicates()
assert A.nnz == 1
print(A.todense())
```

## References

- [homepage](https://scipy.org/)
- [documentation](https://docs.scipy.org/doc/scipy/)
- [source](https://github.com/scipy/scipy)
- [download](https://github.com/scipy/scipy/releases)
- [tracker](https://github.com/scipy/scipy/issues)

## Migration from v[previous]

No SciPy 1.18.0-specific breaking changes or deprecations were provided in the supplied inputs. If you provide the SciPy 1.18.0 release notes, add a concrete mapping here (deprecated → modern alternative) with before/after code.

## API Reference

- **scipy.sparse** - Sparse array/matrix subpackage; provides sparse formats and conversions.
- **scipy.sparse.coo_array((data, (row, col)), shape=...)** - COO sparse array construction from coordinates.
- **scipy.sparse.csr_array((data, (row, col)), shape=...)** - CSR sparse array construction; efficient arithmetic/matvec.
- **scipy.sparse.csc_array((data, (row, col)), shape=...)** - CSC sparse array construction; efficient column slicing.
- **scipy.sparse.bsr_array(...)** - Block sparse row format for block-structured sparsity.
- **scipy.sparse.dia_array(...)** - Diagonal sparse format for banded/diagonal structure.
- **scipy.sparse.dok_array(...)** - Dictionary-of-keys sparse format for incremental construction.
- **scipy.sparse.lil_array(...)** - List-of-lists sparse format for incremental construction.
- **coo_array.sum_duplicates()** - Sum duplicate (row, col) entries in-place to canonicalize COO data.
- **coo_array.tocsr()** - Convert COO to CSR.
- **coo_array.todia()** - Convert COO to DIA (note: DIA may not support indexing).
- **csr_array.eliminate_zeros()** - Remove explicitly stored zeros in-place (affects `nnz` and structure).
- **sparse_array.nnz** - Number of stored entries (includes explicit zeros, counts duplicates in COO pre-canonicalization).
- **sparse_array.todense()** - Convert sparse array to a dense NumPy `ndarray`.
- **sparse_array.max(axis=None)** - Max reduction; axis reductions often produce dense results.
- **sparse_array.argmax(axis=None)** - Argmax; typically over flattened array if `axis=None`.
- **sparse_array.mean(axis=None)** - Mean reduction; axis reductions often produce dense results.
- **scipy.sparse.linalg** - Sparse linear algebra routines (iterative solvers, decompositions, etc.).
- **scipy.sparse.csgraph** - Graph algorithms operating on sparse adjacency matrices.
- **scipy.fft.fft(x, n=None, axis=-1, norm=None)** - 1D FFT.
- **scipy.optimize.differential_evolution(func, bounds, ...)** - Global optimization over bounded domains.