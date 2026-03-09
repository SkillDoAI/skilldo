---
name: scipy
description: SciPy is an open‑source Python library that provides a collection of mathematical algorithms and convenience functions built on the NumPy extension of Python, covering areas such as linear algebra, optimization, integration, interpolation, special functions, FFT, signal and image processing, and more.
license: BSD-3-Clause
metadata:
  version: "1.17.1"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
# Core NumPy import (required for most examples)
import numpy as np

# SciPy top‑level imports
from scipy import sparse
from scipy import fft
from scipy import optimize
from scipy import linalg
from scipy import signal
from scipy import stats
from scipy import integrate
from scipy import interpolate
from scipy import ndimage
from scipy import special
from scipy import odr
from scipy import spatial
from scipy import constants
from scipy import io
from scipy import cluster
from scipy import datasets

# Expose sparse matrix classes for convenient use
coo_matrix = sparse.coo_matrix
csr_matrix = sparse.csr_matrix
csc_matrix = sparse.csc_matrix
bsr_matrix = sparse.bsr_matrix
dia_matrix = sparse.dia_matrix
dok_matrix = sparse.dok_matrix
lil_matrix = sparse.lil_matrix

# Sparse linear algebra and graph utilities
from scipy.sparse import linalg as splinalg
from scipy.sparse import csgraph
```

## Core Patterns

### Sparse Matrix Workflow

```python
from scipy import sparse
import numpy as np

# Pattern: Build → Convert → Compute
# 1. Build in COO or DOK format (matrix API)
row = np.array([0, 1, 2, 0])
col = np.array([0, 1, 2, 2])
data = np.array([1, 2, 3, 4])
coo = sparse.coo_matrix((data, (row, col)), shape=(3, 3))

# 2. Convert to efficient format for operations
csr = coo.tocsr()
csr.sum_duplicates()
csr.eliminate_zeros()

# 3. Perform computations
result = csr @ csr.T
```

### Optimization Workflow

```python
from scipy import optimize

# Pattern: Define → Initialize → Optimize → Validate
# 1. Define objective and constraints
def objective(x):
    return (x[0] - 1) ** 2 + (x[1] - 2.5) ** 2

def constraint(x):
    return x[0] + x[1] - 2

# 2. Initialize
x0 = [0, 0]
cons = {"type": "ineq", "fun": constraint}
bounds = [(0, None), (0, None)]

# 3. Optimize
result = optimize.minimize(objective, x0, method="SLSQP", constraints=cons, bounds=bounds)

# 4. Validate
if result.success:
    print(f"Solution: {result.x}")
```

### ODR Workflow

```python
from scipy import odr
import numpy as np

# Pattern: Model → Data → Fit → Analyze
# 1. Define model
def model_func(B, x):
    return B[0] * x + B[1]

# 2. Prepare data with errors
x = np.array([1, 2, 3, 4, 5])
y = np.array([2.1, 3.9, 6.2, 7.8, 10.1])
sx = np.full_like(x, 0.1, dtype=float)
sy = np.full_like(y, 0.2, dtype=float)

data = odr.RealData(x, y, sx=sx, sy=sy)
model = odr.Model(model_func)

# 3. Fit
odr_obj = odr.ODR(data, model, beta0=[1.0, 0.0])
output = odr_obj.run()

# 4. Analyze results
output.pprint()
print(f"Chi-square: {output.res_var}")
```

### Iterative Solver Pattern

```python
from scipy.sparse import linalg as splinalg
from scipy import sparse
import numpy as np

# Pattern: Precondition → Solve → Check Convergence
A = sparse.csr_matrix([[3, 0, 1], [0, 4, 0], [1, 0, 2]])
b = np.array([1, 2, 3])

# 1. Create preconditioner
M = sparse.diags(1.0 / A.diagonal())

# 2. Solve with callback
def callback(xk):
    print(f"Residual: {np.linalg.norm(A @ xk - b)}")

# Note: SciPy's cg uses `rtol` (relative tolerance) and `atol` (absolute tolerance)
x, info = splinalg.cg(A, b, M=M, callback=callback, rtol=1e-5)

# 3. Check convergence
if info == 0:
    print("Converged successfully")
elif info > 0:
    print(f"Converged after {info} iterations")
else:
    print("Failed to converge")
```

### Signal Processing Pipeline

```python
from scipy import signal
import numpy as np

# Pattern: Design → Filter → Analyze
# 1. Design filter
fs = 1000  # Sample rate
b, a = signal.butter(4, [10, 100], btype="band", fs=fs)

# 2. Apply filter
data = np.random.randn(10000)
filtered = signal.filtfilt(b, a, data)

# 3. Analyze
f, Pxx = signal.welch(filtered, fs=fs)
peaks, _ = signal.find_peaks(Pxx, height=0.1)
```

### Numerical Differentiation

```python
# SciPy provides finite‑difference helpers via `scipy.optimize.approx_fprime`
# and manual implementations for Jacobian/Hessian approximations.

import numpy as np
from scipy.optimize import approx_fprime

# Scalar derivative (using central finite differences via approx_fprime)
def f(x):
    return x ** 3

eps = np.sqrt(np.finfo(float).eps)
deriv = approx_fprime(np.array([2.0]), lambda v: f(v[0]), eps)[0]
print(f"Derivative at x=2.0: {deriv}")

# Jacobian of a vector function (using approx_fprime on each component)
def g(x):
    return np.array([x[0] ** 2 + x[1], x[0] * x[1]])

jac = np.column_stack(
    [approx_fprime(np.array([1.0, 2.0]), lambda v: g(v)[i], eps) for i in range(2)]
)
print(f"Jacobian:\n{jac}")

# Hessian of a scalar function (simple finite‑difference implementation)
def h(x):
    return x[0] ** 2 + x[0] * x[1] + x[1] ** 2

def hessian(func, x, eps=1e-5):
    n = len(x)
    H = np.zeros((n, n))
    fx = func(x)
    for i in range(n):
        x_i1 = np.array(x, copy=True)
        x_i1[i] += eps
        fxi1 = func(x_i1)
        for j in range(i, n):
            x_ij = np.array(x, copy=True)
            x_ij[i] += eps
            x_ij[j] += eps
            fxy = func(x_ij)
            x_j1 = np.array(x, copy=True)
            x_j1[j] += eps
            fxj1 = func(x_j1)
            H[i, j] = (fxy - fxi1 - fxj1 + fx) / (eps ** 2)
            H[j, i] = H[i, j]
    return H

hess = hessian(h, np.array([1.0, 1.0]))
print(f"Hessian:\n{hess}")
```

### Sparse Matrices

```python
import numpy as np
from scipy import sparse

# COO (Coordinate) format - good for construction
row = np.array([0, 1, 2, 0])
col = np.array([0, 1, 2, 2])
data = np.array([1, 2, 3, 4])
coo = sparse.coo_matrix((data, (row, col)), shape=(3, 3))

# CSR (Compressed Sparse Row) - efficient row operations
csr = sparse.csr_matrix([[1, 0, 2], [0, 3, 0]])

# CSC (Compressed Sparse Column) - efficient column operations
csc = sparse.csc_matrix([[1, 0, 2], [0, 3, 0]])

# DOK (Dictionary of Keys) - incremental construction
dok = sparse.dok_matrix((5, 5))
dok[0, 0] = 1
dok[1, 2] = 2

# LIL (List of Lists) - incremental construction
lil = sparse.lil_matrix((10, 10))
lil[0, :5] = 1
lil[1, 5:] = 2

# Format conversion
csr = coo.tocsr()
csc = coo.tocsc()
dense = coo.toarray()

# Operations
a = sparse.csr_matrix([[1, 0, 2], [0, 3, 0]])
b = sparse.csr_matrix([[0, 1], [2, 0], [0, 3]])
c = a @ b
a.eliminate_zeros()
a.sum_duplicates()
```

### Sparse Linear Algebra

```python
from scipy.sparse import linalg as splinalg
from scipy import sparse
import numpy as np

A = sparse.csr_matrix([[3, 0, 1], [0, 4, 0], [1, 0, 2]])
b = np.array([1, 2, 3])

# Direct solve
x = splinalg.spsolve(A, b)

# Iterative solvers
x, info = splinalg.cg(A, b)    # Conjugate gradient
x, info = splinalg.gmres(A, b) # GMRES

# Eigenvalue problems
eigenvalues, eigenvectors = splinalg.eigs(A, k=2)

# Matrix norms
norm = splinalg.norm(A)
```

### Sparse Graph Algorithms

```python
from scipy.sparse import csgraph
from scipy import sparse
import numpy as np

graph = sparse.csr_matrix([
    [0, 1, 2, 0],
    [1, 0, 0, 1],
    [2, 0, 0, 3],
    [0, 1, 3, 0]
])

dist_matrix = csgraph.dijkstra(graph, indices=0)
all_pairs = csgraph.floyd_warshall(graph)
mst = csgraph.minimum_spanning_tree(graph)
n_components, labels = csgraph.connected_components(graph)
bfs_tree = csgraph.breadth_first_tree(graph, 0)
dfs_tree = csgraph.depth_first_tree(graph, 0)
```

### Fast Fourier Transform

```python
from scipy import fft
import numpy as np

x = np.array([1.0, 2.0, 1.0, -1.0, 1.5])
y = fft.fft(x)
x_recovered = fft.ifft(y)

# Real FFT (more efficient for real input)
y = fft.rfft(x)
x_recovered = fft.irfft(y)

# 2D FFT
image = np.random.rand(100, 100)
freq = fft.fft2(image)
image_recovered = fft.ifft2(freq)

# FFT with shifting
freq_shifted = fft.fftshift(freq)
freq_unshifted = fft.ifftshift(freq_shifted)

# Frequency bins
freqs = fft.fftfreq(len(x), d=0.1)
```

### Minimization

```python
from scipy import optimize
import numpy as np

def rosenbrock(x):
    return sum(100.0 * (x[1:] - x[:-1] ** 2) ** 2 + (1 - x[:-1]) ** 2)

x0 = np.array([1.3, 0.7, 0.8, 1.9, 1.2])
result = optimize.minimize(rosenbrock, x0, method="BFGS")

# With constraints (2D example: minimize x0^2 + x1^2 subject to x0 + x1 = 1)
def objective_2d(x):
    return x[0] ** 2 + x[1] ** 2

def constraint_eq(x):
    return x[0] + x[1] - 1  # equality: x0 + x1 == 1

x0_2d = np.array([0.5, 0.5])  # feasible starting point
cons = {"type": "eq", "fun": constraint_eq}
result = optimize.minimize(objective_2d, x0_2d, method="SLSQP", constraints=cons)

# With bounds
bounds = [(0, None)] * 5
result = optimize.minimize(
    rosenbrock,
    np.array([1.3, 0.7, 0.8, 1.9, 1.2]),
    method="L-BFGS-B",
    bounds=bounds,
)
```

### Global Optimization

```python
from scipy import optimize
import numpy as np

def objective(x):
    return x[0] ** 2 + x[1] ** 2

bounds = [(-5, 5), (-5, 5)]
result = optimize.differential_evolution(
    objective,
    bounds,
    strategy="best1bin",
    maxiter=1000,
    popsize=15,
    atol=0.01,
    mutation=(0.5, 1),
    recombination=0.7,
    workers=4,
)
```

### Root Finding

```python
from scipy import optimize

def f(x):
    return x ** 3 - 1

root = optimize.brentq(f, -2, 2)
root = optimize.newton(f, x0=0.5)

def equations(vars):
    x, y = vars
    return [x ** 2 + y ** 2 - 1, x - y]

solution = optimize.fsolve(equations, [1, 1])
```

### Curve Fitting

```python
from scipy import optimize
import numpy as np

xdata = np.array([0, 1, 2, 3, 4])
ydata = np.array([1, 3, 5, 7, 9])

def model(x, a, b):
    return a * x + b

params, cov = optimize.curve_fit(model, xdata, ydata)
```

### ODR with Measurement Errors

```python
from scipy import odr
import numpy as np

x = np.array([0.0, 0.9, 1.8, 2.6, 3.3, 4.4, 5.2, 6.1, 6.5, 7.4])
y = np.array([5.9, 5.4, 4.4, 4.6, 3.5, 3.7, 2.8, 2.8, 2.4, 1.5])
x_err = np.array([0.03, 0.03, 0.04, 0.035, 0.07, 0.11, 0.13, 0.22, 0.74, 1.0])
y_err = np.array([1.0, 0.74, 0.5, 0.35, 0.22, 0.22, 0.12, 0.12, 0.1, 0.04])

def func(B, x):
    return B[0] * x + B[1]

data = odr.RealData(x, y, sx=x_err, sy=y_err)
model = odr.Model(func)
odr_obj = odr.ODR(data, model, beta0=[0.0, 1.0])
output = odr_obj.run()
output.pprint()
```

### Built-in ODR Models

```python
from scipy import odr
import numpy as np

x = np.linspace(0.0, 5.0, 50)

# Linear: y = B[0] + B[1]*x
y = 10.0 + 5.0 * x
data = odr.Data(x, y)
output = odr.ODR(data, odr.multilinear, beta0=[1.0, 1.0]).run()

# Exponential: y = B[0] + exp(B[1] * x)
y = -10.0 + np.exp(0.5 * x)
data = odr.Data(x, y)
output = odr.ODR(data, odr.exponential, beta0=[-10.0, 0.5]).run()

# Polynomial of arbitrary degree (cubic)
y = 1.0 + 2.0 * x + 3.0 * x ** 2 + 4.0 * x ** 3
poly_model = odr.polynomial(3)
data = odr.Data(x, y)
output = odr.ODR(data, poly_model, beta0=[1.0, 2.0, 3.0, 4.0]).run()

# Unilinear: y = B[0]*x + B[1]
y = 1.0 * x + 2.0
data = odr.Data(x, y)
output = odr.ODR(data, odr.unilinear).run()

# Quadratic: y = B[0]*x**2 + B[1]*x + B[2]
y = 1.0 * x ** 2 + 2.0 * x + 3.0
data = odr.Data(x, y)
output = odr.ODR(data, odr.quadratic).run()
```

### Linear Algebra

```python
from scipy import linalg
import numpy as np

A = np.array([[1, 2], [3, 4]])
b = np.array([5, 6])

x = linalg.solve(A, b)
A_inv = linalg.inv(A)
det = linalg.det(A)
eigenvalues, eigenvectors = linalg.eig(A)
U, s, Vh = linalg.svd(A)
Q, R = linalg.qr(A)
L = linalg.cholesky(A @ A.T)
exp_A = linalg.expm(A)
sqrt_A = linalg.sqrtm(A)
```

### Integration

```python
from scipy import integrate
import numpy as np

def f(x):
    return x ** 2

result, error = integrate.quad(f, 0, 1)

def f2(y, x):
    return x * y

result = integrate.dblquad(f2, 0, 1, 0, 1)

x = np.linspace(0, 1, 100)
y = x ** 2
result = integrate.trapezoid(y, x)
result = integrate.simpson(y, x=x)

def deriv(y, t):
    return -2 * y

y0 = 1
t = np.linspace(0, 5, 100)
solution = integrate.odeint(deriv, y0, t)
```

### Interpolation

```python
from scipy import interpolate
import numpy as np

x = np.array([0, 1, 2, 3, 4])
y = np.array([0, 2, 1, 3, 2])

f = interpolate.interp1d(x, y)
f_cubic = interpolate.interp1d(x, y, kind="cubic")
x_new = np.linspace(0, 4, 100)
y_new = f_cubic(x_new)

tck = interpolate.splrep(x, y, s=0)
y_new = interpolate.splev(x_new, tck)
```

### Signal Processing

```python
from scipy import signal
import numpy as np

b, a = signal.butter(4, 0.1)
sos = signal.butter(4, 0.1, output="sos")

data = np.random.randn(1000)
filtered = signal.filtfilt(b, a, data)

x = np.array([1, 2, 3])
h = np.array([0, 1, 0.5])
y = signal.convolve(x, h)

peaks, properties = signal.find_peaks(data, height=0.5, distance=10)
f, t, Sxx = signal.spectrogram(data, fs=1000)
window = signal.windows.hann(50)
```

### Statistics

```python
from scipy import stats
import numpy as np

norm = stats.norm(loc=0, scale=1)
pdf = norm.pdf(0)
cdf = norm.cdf(1.96)
quantile = norm.ppf(0.975)
samples = norm.rvs(size=1000)

data1 = np.random.randn(100)
data2 = np.random.randn(100) + 0.5

# ttest_ind signature
statistic, pvalue = stats.ttest_ind(
    data1,
    data2,
    axis=0,
    equal_var=True,
    nan_policy="propagate",
    alternative="two-sided",
    trim=0,
    method=None,
    keepdims=False,
)
statistic, pvalue = stats.kstest(data1, "norm")

observed = np.array([10, 20, 30])
expected = np.array([15, 15, 30])
statistic, pvalue = stats.chisquare(observed, expected)

corr, pvalue = stats.pearsonr(data1[:50], data2[:50])
desc = stats.describe(data1)
```

### Image Processing

```python
from scipy import ndimage
import numpy as np

image = np.random.rand(100, 100)

smoothed = ndimage.gaussian_filter(image, sigma=2)
edges = ndimage.sobel(image)
median = ndimage.median_filter(image, size=5)

binary = image > 0.5
dilated = ndimage.binary_dilation(binary)
eroded = ndimage.binary_erosion(binary)

rotated = ndimage.rotate(image, 45)
zoomed = ndimage.zoom(image, 2.0)

labeled, num_features = ndimage.label(binary)
sizes = ndimage.sum_labels(image, labeled, range(num_features + 1))
```

### Spatial Algorithms

```python
from scipy import spatial
import numpy as np

# Non‑degenerate set of points for geometry algorithms
points1 = np.array([[0, 0], [1, 0], [0, 1]])  # triangle (non‑collinear)
points2 = np.array([[0, 1], [1, 2]])

# Pairwise distances
dist_matrix = spatial.distance.cdist(points1, points2)

# KD-Tree for nearest neighbor search
tree = spatial.KDTree(points1)
distances, indices = tree.query(points2, k=1)

# Convex hull
hull = spatial.ConvexHull(points1)

# Delaunay triangulation
tri = spatial.Delaunay(points1)

# Voronoi diagram
vor = spatial.Voronoi(points1)
```

### Configuration

### Subpackages Overview

SciPy is organized into subpackages, each focused on a specific domain:

- **cluster**: Clustering algorithms (k-means, hierarchical)  
- **constants**: Physical and mathematical constants  
- **datasets**: Example datasets for testing  
- **differentiate**: Finite‑difference differentiation  
- **fft**: Fast Fourier Transform algorithms  
- **integrate**: Integration and ODE solvers  
- **interpolate**: Interpolation and smoothing  
- **io**: Data input/output (MATLAB, WAV, etc.)  
- **linalg**: Linear algebra routines  
- **ndimage**: N‑dimensional image processing  
- **odr**: Orthogonal Distance Regression  
- **optimize**: Optimization and root finding  
- **signal**: Signal processing  
- **sparse**: Sparse matrix support (matrix API)  
- **spatial**: Spatial data structures and algorithms  
- **special**: Special mathematical functions  
- **stats**: Statistical functions and distributions  

### Parallelism

SciPy defaults to single‑threaded execution for most high‑level functions.  
Parallel execution is exposed via the `workers=` keyword argument where officially supported (e.g., `scipy.fft.fft`, `scipy.optimize.differential_evolution`). When using this together with BLAS/LAPACK‑backed routines, explicitly limit BLAS threads **if** the `threadpoolctl` package is available to avoid oversubscription.  

BLAS/LAPACK (used by `scipy.linalg`) defaults to multi‑threaded execution using all available CPU cores. Use `threadpoolctl` (when installed) to control BLAS threading. Free‑threaded CPython (Python 3.13+) support is experimental since SciPy 1.15.0.

### Lazy Loading

SciPy uses lazy loading — submodules are imported only when first accessed to reduce startup time.

## Pitfalls

### Wrong: Using legacy matrix classes instead of the array API

```python
# scipy.sparse.csr_matrix is a legacy class; prefer the array API instead
from scipy.sparse import csr_matrix
A = csr_matrix([[1, 0], [0, 2]])
```

### Right: Use matrix classes (the only option in SciPy 1.17)

```python
# In SciPy 1.17 the matrix API is the supported interface.
from scipy.sparse import csr_matrix
A = csr_matrix([[1, 0], [0, 2]])
```

### Wrong: Modifying COO matrices in‑place for incremental construction

```python
from scipy.sparse import coo_matrix
import numpy as np

# COO does not support efficient element‑wise assignment
coo = coo_matrix((3, 3))
coo[0, 0] = 1  # Error or inefficient
```

### Right: Use DOK or LIL for incremental construction

```python
from scipy.sparse import dok_matrix

dok = dok_matrix((3, 3))
dok[0, 0] = 1
dok[1, 2] = 5
csr = dok.tocsr()  # Convert when done building
```

### Wrong: Using b/a filter coefficients for long filters

```python
from scipy import signal
import numpy as np

# Numerically unstable for high‑order filters
b, a = signal.butter(10, 0.1)
filtered = signal.filtfilt(b, a, np.random.randn(1000))
```

### Right: Use second‑order sections (SOS) format

```python
from scipy import signal
import numpy as np

sos = signal.butter(10, 0.1, output="sos")
filtered = signal.sosfiltfilt(sos, np.random.randn(1000))
```

### Wrong: Using `optimize.minimize` without checking success

```python
from scipy import optimize

result = optimize.minimize(lambda x: x[0] ** 2, [10.0])
print(result.x)  # May be wrong if optimization failed
```

### Right: Always check `result.success`

```python
from scipy import optimize

result = optimize.minimize(lambda x: x[0] ** 2, [10.0])
if result.success:
    print(result.x)
else:
    raise RuntimeError(f"Optimization failed: {result.message}")
```

### Wrong: Performing arithmetic on COO format directly

```python
from scipy.sparse import coo_matrix
import numpy as np

coo = coo_matrix([[1, 0], [0, 2]])
result = coo @ coo  # Inefficient; COO not designed for arithmetic
```

### Right: Convert to CSR/CSC before arithmetic

```python
from scipy.sparse import coo_matrix

coo = coo_matrix([[1, 0], [0, 2]])
csr = coo.tocsr()
result = csr @ csr
```

### Wrong: Using `integrate.quad` on discontinuous functions without specifying breakpoints

```python
from scipy import integrate

# Inaccurate if discontinuity is not declared
result, _ = integrate.quad(lambda x: abs(x), -1, 1)
```

### Right: Pass breakpoints via the `points` argument

```python
from scipy import integrate

result, _ = integrate.quad(lambda x: abs(x), -1, 1, points=[0])
```

### Wrong: Subscripting DIA sparse arrays directly

```python
from scipy import sparse

# DIA arrays do not support element‑wise subscripting
dia = sparse.coo_matrix([[1, 0], [0, 2]]).todia()
val = dia[0, 0]  # TypeError
```

### Right: Convert to CSR before indexing

```python
from scipy import sparse

dia = sparse.coo_matrix([[1, 0], [0, 2]]).todia()
val = dia.tocsr()[0, 0]
```

### Wrong: Assuming BLAS/LAPACK is single‑threaded when using multiprocessing

```python
import multiprocessing
from scipy import linalg
import numpy as np

# BLAS may spawn threads per process, causing oversubscription
pool = multiprocessing.Pool(8)
results = pool.map(lambda A: linalg.svd(A), data_list)
```

### Right: Limit BLAS threads when combining with multiprocessing

```python
import multiprocessing

# If threadpoolctl is available, limit BLAS threads; otherwise proceed without limiting
try:
    from threadpoolctl import threadpool_limits
    limits_ctx = threadpool_limits(limits=1, user_api="blas")
except ImportError:
    limits_ctx = None

if limits_ctx:
    with limits_ctx:
        pool = multiprocessing.Pool(8)
        results = pool.map(lambda A: linalg.svd(A), data_list)
else:
    pool = multiprocessing.Pool(8)
    results = pool.map(lambda A: linalg.svd(A), data_list)
```

## Migration

### Highlights for SciPy 1.17.1

* **Parallelism (`workers=`)** – More high‑level functions now accept a `workers=` keyword. When using this together with BLAS/LAPACK‑backed routines, explicitly limit BLAS threads with `threadpoolctl` **if** it is installed to avoid oversubscription.
* **Sparse matrix API** – The array‑style sparse classes (`*_array`) are **not** available in this release; use the classic matrix classes (`*_matrix`) instead. After constructing a sparse matrix from `(data, (row, col))` tuples, call `.sum_duplicates()` if you need a canonical representation. Use `.eliminate_zeros()` to drop explicit zeros before heavy arithmetic.
* **No breaking API removals** – All patterns shown remain valid. No deprecations are flagged for this release, but keep an eye on future release notes.

### General upgrade checklist

1. Verify you are running on Python 3.13 (or later) if you plan to use free‑threaded mode.
2. Update any calls to FFT, differential evolution, or other functions that support `workers=` to explicitly pass the desired number of workers if you previously relied on environment variables.
3. Review sparse‑matrix construction code: add `.eliminate_zeros()` or `.sum_duplicates()` where appropriate to keep `nnz` minimal.
4. If you control BLAS/LAPACK threading, consider adding `import threadpoolctl; threadpoolctl.threadpool_limits(...)` to your startup script.
5. Run your test suite to catch any subtle behavior changes related to in‑place mutation of sparse objects.

## Documentation & Conventions

### Documented APIs (relevant to SciPy 1.17.1)

- `scipy`
- `scipy.sparse`
- `scipy.sparse.coo_matrix`
- `scipy.sparse.csr_matrix`
- `scipy.sparse.csc_matrix`
- `scipy.sparse.bsr_matrix`
- `scipy.sparse.dia_matrix`
- `scipy.sparse.dok_matrix`
- `scipy.sparse.lil_matrix`
- `scipy.sparse.linalg`
- `scipy.sparse.csgraph`
- `scipy.fft.fft`
- `scipy.optimize.differential_evolution`
- `threadpoolctl` (optional, for thread control)

### Conventions

- Use snake_case for function and method names (e.g., `coo_matrix`, `eliminate_zeros`).
- Prefer the dedicated sparse **matrix** constructors (`coo_matrix`, `csr_matrix`, `csc_matrix`, etc.) instead of converting from dense arrays when possible.
- Explicitly control parallel execution with the `workers=` keyword on APIs that support it (e.g., `scipy.fft.fft`, `scipy.optimize.differential_evolution`).
- When you need to limit BLAS/LAPACK threading, use the `threadpoolctl` package to set the number of threads.
- Remove explicit zeros from a sparse matrix with `eliminate_zeros()`; this mutates the matrix in‑place.
- Collapse duplicate entries in a sparse matrix with `sum_duplicates()` before heavy computations.
- Convert a sparse matrix to a dense `numpy.ndarray` with `toarray()` when a fully explicit representation is required.
- All reduction methods (`max`, `argmax`, `mean`, etc.) behave like their NumPy counterparts and return NumPy scalars or arrays.
- SciPy defaults to single‑threaded execution; only BLAS/LAPACK back‑ends are multi‑threaded by default.

### Pitfalls (updated)

- **Array‑API not available** – Do not attempt to import `csr_array` or similar; they are introduced in later SciPy versions.
- **Explicit zeros** – Stored zeros increase `nnz` and can affect performance; call `eliminate_zeros()` if they are not needed.
- **Duplicate entries** – Build sparse matrices with duplicate coordinates only when intentional; use `sum_duplicates()` to canonicalize.
- **In‑place mutation** – Methods like `eliminate_zeros()` modify the object in place; copy if you need an unchanged version.

### Breaking changes

None for the jump to SciPy 1.17.1.
## References

- [homepage](https://scipy.org/)
- [documentation](https://docs.scipy.org/doc/scipy/)
- [source](https://github.com/scipy/scipy)
- [download](https://github.com/scipy/scipy/releases)
- [tracker](https://github.com/scipy/scipy/issues)
