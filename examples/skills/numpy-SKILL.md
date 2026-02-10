---

name: numpy
description: N-dimensional array computing library providing array types, vectorized operations, linear algebra, FFT, random sampling, and testing utilities.
version: 1.26
ecosystem: python
license: BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0
generated_with: gpt-5.2
---

## Imports

```python
import numpy as np
from numpy import array, asarray, arange, zeros, ones, empty, linspace
from numpy import dtype, reshape, concatenate, stack, where
from numpy import sum, mean, std, min, max
from numpy import dot
from numpy.linalg import norm, solve
```

## Core Patterns

### Create arrays and control dtype/shape ✅ Current
```python
import numpy as np

def main() -> None:
    a: np.ndarray = np.array([1, 2, 3], dtype=np.int64)
    b: np.ndarray = np.zeros((2, 3), dtype=np.float64)
    c: np.ndarray = np.arange(0, 10, 2, dtype=np.int32)

    d: np.dtype = np.dtype([("x", np.int32), ("y", np.float64)])
    rec: np.ndarray = np.zeros(3, dtype=d)

    # Print in a way that reliably includes dtype names and field names in stdout.
    print("a dtype:", a.dtype)
    print("b dtype:", b.dtype)
    print("c dtype:", c.dtype)
    print("rec dtype names:", rec.dtype.names)

if __name__ == "__main__":
    main()
```
* Use `np.array`/`np.asarray` for explicit conversion, `np.zeros`/`np.ones`/`np.empty` for allocation, and `np.dtype(...)` to define dtypes (including structured/record dtypes).

### Vectorized computation, masking, and selection ✅ Current
```python
import numpy as np

def main() -> None:
    x: np.ndarray = np.linspace(-2.0, 2.0, 9)
    y: np.ndarray = x**2 - 1.0

    mask: np.ndarray = y > 0
    y_pos: np.ndarray = y[mask]

    y_clipped: np.ndarray = np.clip(y, -0.5, 2.0)
    y_piecewise: np.ndarray = np.where(x < 0, -y, y)

    print("x:", x)
    print("y:", y)
    print("mask:", mask)
    print("y[mask]:", y_pos)
    print("clip:", y_clipped)
    print("where:", y_piecewise)

if __name__ == "__main__":
    main()
```
* Prefer ufuncs and vectorized expressions over Python loops; use boolean masks and `np.where` for selection.

### Reshape, stack, and concatenate ✅ Current
```python
import numpy as np

def main() -> None:
    a: np.ndarray = np.arange(12)
    m: np.ndarray = a.reshape(3, 4)

    top: np.ndarray = m[:2, :]
    bottom: np.ndarray = m[2:, :]

    v: np.ndarray = np.concatenate([top, bottom], axis=0)
    h: np.ndarray = np.concatenate([m[:, :2], m[:, 2:]], axis=1)

    stacked0: np.ndarray = np.stack([m, m + 100], axis=0)

    print("m:\n", m)
    print("concat axis=0:\n", v)
    print("concat axis=1:\n", h)
    print("stack axis=0 shape:", stacked0.shape)

if __name__ == "__main__":
    main()
```
* Use `reshape` for view-like shape changes when possible; use `concatenate`/`stack` for combining arrays along axes.

### Linear algebra with `numpy.linalg` ✅ Current
```python
import numpy as np

def main() -> None:
    A: np.ndarray = np.array([[3.0, 1.0], [1.0, 2.0]], dtype=np.float64)
    b: np.ndarray = np.array([9.0, 8.0], dtype=np.float64)

    x: np.ndarray = np.linalg.solve(A, b)
    r: np.ndarray = A @ x - b
    r_norm: float = float(np.linalg.norm(r))

    print("x:", x)
    print("residual norm:", r_norm)

if __name__ == "__main__":
    main()
```
* Use `np.linalg.solve` for linear systems and `np.linalg.norm` for vector/matrix norms; prefer `@` for matrix multiplication.

### Run NumPy’s test suite from Python ✅ Current
```python
import numpy as np

def main() -> None:
    # Runs NumPy's own test suite (requires pytest; may take time).
    result = np.test()
    print("numpy.test() returned:", result)

if __name__ == "__main__":
    main()
```
* Use the public `numpy.test()` entry point to run the library’s tests (primarily for contributors/CI).

## Configuration

- NumPy has minimal runtime “configuration” in typical user code; behavior is mainly controlled via:
  - **Dtypes**: choose `dtype=` explicitly (`np.float64`, `np.int32`, structured `np.dtype([...])`) to avoid platform-dependent defaults.
  - **Printing**: `np.set_printoptions(...)` to control precision, suppress scientific notation, etc.
  - **Error handling**: `np.seterr(...)` / `np.errstate(...)` to configure floating-point warnings/errors.
- Testing (contributors/CI):
  - `numpy.test()` requires `pytest` and (for parts of the suite) `hypothesis`.

## Pitfalls

### Wrong: Assuming list-based structured dtypes create custom field names
```python
import numpy as np

def main() -> None:
    dt = [np.int32, np.float64]  # list form => default field names f0, f1 (not "x", "y")
    a = np.zeros(3, dtype=dt)
    print(a["x"])  # raises ValueError: no field of name x

if __name__ == "__main__":
    main()
```

### Right: Specify names explicitly for structured dtypes
```python
import numpy as np

def main() -> None:
    dt = {"names": ["x", "y"], "formats": [np.int32, np.float64]}
    a = np.zeros(3, dtype=dt)
    a["x"] = [1, 2, 3]
    print(a["x"])

if __name__ == "__main__":
    main()
```

### Wrong: Using `numpy._core` (private) instead of public top-level APIs
```python
import numpy as np

def main() -> None:
    # Private module; not stable API.
    import numpy._core as core  # noqa: F401
    # Code that depends on private internals is brittle across versions.
    print(core)

if __name__ == "__main__":
    main()
```

### Right: Use public `numpy` APIs (top-level) and documented submodules
```python
import numpy as np

def main() -> None:
    a = np.arange(5)
    print(np.sum(a))
    print(np.__version__)

if __name__ == "__main__":
    main()
```

### Wrong: Expecting `np.asarray` to copy input data
```python
import numpy as np

def main() -> None:
    base = np.array([1, 2, 3], dtype=np.int64)
    view = np.asarray(base)  # may share memory
    view[0] = 999
    print("base changed:", base)  # base changed too

if __name__ == "__main__":
    main()
```

### Right: Use `np.array(..., copy=True)` when you need an explicit copy
```python
import numpy as np

def main() -> None:
    base = np.array([1, 2, 3], dtype=np.int64)
    copied = np.array(base, copy=True)
    copied[0] = 999
    print("base:", base)
    print("copied:", copied)

if __name__ == "__main__":
    main()
```

### Wrong: Running `numpy.test()` without test dependencies installed
```python
import numpy as np

def main() -> None:
    # If pytest/hypothesis are missing, this can error or skip large parts.
    np.test()

if __name__ == "__main__":
    main()
```

### Right: Ensure `pytest` (and often `hypothesis`) are installed before calling `numpy.test()`
```python
import importlib.util
import numpy as np

def main() -> None:
    if importlib.util.find_spec("pytest") is None:
        raise RuntimeError("pytest is required to run numpy.test()")
    # hypothesis is also used by parts of the suite; install if needed.
    np.test()

if __name__ == "__main__":
    main()
```

## References

- [homepage](https://numpy.org)
- [documentation](https://numpy.org/doc/)
- [source](https://github.com/numpy/numpy)
- [download](https://pypi.org/project/numpy/#files)
- [tracker](https://github.com/numpy/numpy/issues)
- [release notes](https://numpy.org/doc/stable/release)

## Migration from v[previous]

- No explicit runtime breaking changes were provided in the supplied excerpts for NumPy 1.26.x.
- Practical upgrade notes from the provided context:
  - If you rely on NumPy’s typing stubs, expect incremental signature/typing refinements in 1.26 (may require updating annotations or type-checker expectations).
  - Prefer documented structured dtype forms (e.g., dict with `names`/`formats`) to avoid implicit default field names.
  - For contributors using the C-API: follow dtype descriptor (`PyArray_Descr*`) ownership/steals-reference rules (reference counting correctness).

## API Reference

- **numpy.test** - Run NumPy’s test suite via `pytest`; useful for contributors/CI.
- **numpy.__version__** - NumPy version string.
- **numpy.array(obj, dtype=..., copy=..., ndmin=...)** - Create an `ndarray` from array-like input.
- **numpy.asarray(a, dtype=...)** - Convert to `ndarray` without unnecessary copying.
- **numpy.arange([start,] stop[, step], dtype=...)** - Create evenly spaced values in a half-open interval.
- **numpy.linspace(start, stop, num=..., dtype=...)** - Create `num` evenly spaced samples over an interval.
- **numpy.zeros(shape, dtype=...)** - Allocate an array filled with zeros.
- **numpy.ones(shape, dtype=...)** - Allocate an array filled with ones.
- **numpy.empty(shape, dtype=...)** - Allocate an uninitialized array (contents arbitrary).
- **numpy.dtype(spec)** - Construct/normalize a dtype (including structured dtypes).
- **numpy.reshape(a, newshape)** - Return a reshaped view/copy of an array.
- **numpy.concatenate(seq, axis=...)** - Join arrays along an existing axis.
- **numpy.stack(seq, axis=...)** - Join arrays along a new axis.
- **numpy.where(condition, x, y)** - Elementwise selection based on a boolean condition.
- **numpy.linalg.solve(A, b)** - Solve a linear system `A @ x = b`.
- **numpy.linalg.norm(x, ord=..., axis=..., keepdims=...)** - Compute vector/matrix norms.