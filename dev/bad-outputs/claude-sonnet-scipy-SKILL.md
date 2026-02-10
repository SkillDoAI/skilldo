---
name: scipy
description: Scientific computing library providing algorithms for optimization, integration, interpolation, eigenvalue problems, algebraic equations, differential equations, statistics and more
version: 1.18.0
ecosystem: python
license: BSD-3-Clause
---

## Imports

```python
# Most common: import subpackages directly
import scipy
from scipy import optimize, linalg, stats, sparse, integrate, interpolate, signal

# Specific functions from submodules
from scipy.optimize import minimize, differential_evolution, curve_fit
from scipy.linalg import inv, solve, eig, svd
from scipy.stats import norm, ttest_ind
from scipy.sparse import csr_array, coo_array
from scipy.integrate import quad, odeint

# Low-level callback for performance-critical code
from scipy import LowLevelCallable
```

## Core Patterns

### Unconstrained Optimization ✅ Current
```python
from scipy.optimize import minimize
import numpy as np

def objective(x):
    """Rosenbrock function"""
    return (1 - x[0])**2 + 100 * (x[1] - x[0]**2)**2

# Starting point
x0 = np.array([0.0, 0.0])

# Minimize with default method (BFGS)
result = minimize(objective, x0)

print(f"Optimal x: {result.x}")
print(f"Optimal f(x): {result.fun}")
print(f"Success: {result.success}")
print(f"Function evaluations: {result.nfev}")
```
* Unified interface for multiple optimization algorithms
* Returns `OptimizeResult` object with solution and metadata
* **Status**: Current, stable

### Constrained Optimization with Bounds ✅ Current
```python
from scipy.optimize import minimize, Bounds
import numpy as np

def objective(x):
    return (x[0] - 1)**2 + (x[1] - 2.5)**2

# Box constraints: -5 <= x[0] <= 5, 0 <= x[1] <= 10
bounds = Bounds([-5, 0], [5, 10])

x0 = np.array([0.0, 0.0])
result = minimize(objective, x0, method='trust-constr', bounds=bounds)

print(f"Constrained solution: {result.x}")
```
* Use `Bounds` class for box constraints (lower and upper bounds)
* Specify `method='trust-constr'` or other constrained optimizer
* **Status**: Current, stable

### Optimization with Linear and Nonlinear Constraints ✅ Current
```python
from scipy.optimize import minimize, LinearConstraint, NonlinearConstraint
import numpy as np

def objective(x):
    return (x[0] - 1)**2 + (x[1] - 2.5)**2

# Linear constraint: x[0] + x[1] >= 2
A = np.array([[1, 1]])
linear_constraint = LinearConstraint(A, lb=2, ub=np.inf)

# Nonlinear constraint: x[0]^2 + x[1]^2 <= 4
def circle_constraint(x):
    return x[0]**2 + x[1]**2

nonlinear_constraint = NonlinearConstraint(circle_constraint, lb=-np.inf, ub=4)

x0 = np.array([0.0, 0.0])
result = minimize(
    objective, 
    x0, 
    method='trust-constr',
    constraints=[linear_constraint, nonlinear_constraint]
)

print(f"Solution: {result.x}")
print(f"Constraint violations: {result.constr_violation}")
```
* `LinearConstraint(A, lb, ub)` for linear constraints: `lb <= A @ x <= ub`
* `NonlinearConstraint(fun, lb, ub)` for nonlinear constraints: `lb <= fun(x) <= ub`
* Use `np.inf` for unbounded constraints
* **Status**: Current, stable

### Global Optimization ✅ Current
```python
from scipy.optimize import differential_evolution
import numpy as np

def objective(x):
    """Rastrigin function - has many local minima"""
    return 10 * len(x) + sum(xi**2 - 10 * np.cos(2 * np.pi * xi) for xi in x)

# Bounds for each dimension
bounds = [(-5.12, 5.12), (-5.12, 5.12)]

# Differential evolution doesn't need initial point
result = differential_evolution(objective, bounds, seed=42)

print(f"Global minimum: {result.x}")
print(f"Function value: {result.fun}")
```
* Use for functions with multiple local minima
* Bounds are required (not optional)
* Stochastic algorithm - set `seed` for reproducibility
* **Status**: Current, stable

### Curve Fitting ✅ Current
```python
from scipy.optimize import curve_fit
import numpy as np

# Model function
def model(x, a, b, c):
    return a * np.exp(-b * x) + c

# Generate noisy data
x_data = np.linspace(0, 4, 50)
y_data = model(x_data, 2.5, 1.3, 0.5) + 0.2 * np.random.normal(size=len(x_data))

# Fit curve to data
params, covariance = curve_fit(model, x_data, y_data)

print(f"Fitted parameters: a={params[0]:.2f}, b={params[1]:.2f}, c={params[2]:.2f}")
print(f"Parameter uncertainties: {np.sqrt(np.diag(covariance))}")
```
* Nonlinear least squares fitting
* Returns fitted parameters and covariance matrix
* Optionally provide bounds and initial parameter guess
* **Status**: Current, stable

### Linear Algebra Operations ✅ Current
```python
from scipy import linalg
import numpy as np

# Create a matrix
A = np.array([[1, 2], [3, 4]])
b = np.array([5, 6])

# Solve linear system: A @ x = b
x = linalg.solve(A, b)

# Matrix inverse
A_inv = linalg.inv(A)

# Eigenvalues and eigenvectors
eigenvalues, eigenvectors = linalg.eig(A)

# Singular value decomposition
U, s, Vh = linalg.svd(A)

# QR decomposition
Q, R = linalg.qr(A)

# Cholesky decomposition (for positive definite matrices)
L = linalg.cholesky(A @ A.T, lower=True)

# Matrix norm
norm_value = linalg.norm(A, ord='fro')
```
* Comprehensive linear algebra routines built on LAPACK
* More features than NumPy's linalg module
* Set `check_finite=False` for performance if inputs are known valid
* **Status**: Current, stable

### Sparse Matrix Operations ✅ Current
```python
from scipy import sparse
import numpy as np

# Create sparse matrix from dense
dense = np.array([[1, 0, 0], [0, 0, 3], [4, 5, 0]])
sparse_csr = sparse.csr_array(dense)

# Create from COO format (coordinate format)
row = np.array([0, 0, 1, 2, 2])
col = np.array([0, 2, 1, 0, 1])
data = np.array([1, 2, 3, 4, 5])
sparse_coo = sparse.coo_array((data, (row, col)), shape=(3, 3))

# Convert between formats
sparse_csc = sparse_csr.tocsc()  # Compressed Sparse Column

# Sparse matrix operations
result = sparse_csr @ sparse_csr.T  # Matrix multiplication
sum_value = sparse_csr.sum()
max_value = sparse_csr.max()

# Remove explicit zeros
sparse_csr.eliminate_zeros()

# Consolidate duplicate entries
sparse_coo.sum_duplicates()

# Convert back to dense
dense_result = sparse_csr.todense()
```
* Multiple sparse formats: CSR (row), CSC (column), COO (coordinate), BSR, DIA, DOK, LIL
* CSR and CSC are efficient for arithmetic and matrix-vector products
* COO is efficient for construction and format conversion
* **Status**: Current, stable

### Statistical Analysis ✅ Current
```python
from scipy import stats
import numpy as np

# Generate random data
data1 = np.random.normal(10, 2, 100)
data2 = np.random.normal(11, 2, 100)

# T-test: are means significantly different?
t_statistic, p_value = stats.ttest_ind(data1, data2)
print(f"T-test p-value: {p_value:.4f}")

# Distribution fitting
# Use a normal distribution
mean, std = stats.norm.fit(data1)
print(f"Fitted normal: mean={mean:.2f}, std={std:.2f}")

# Probability density function
x = np.linspace(0, 20, 100)
pdf = stats.norm.pdf(x, loc=mean, scale=std)

# Cumulative distribution function
cdf = stats.norm.cdf(15, loc=mean, scale=std)

# Generate random samples from distribution
samples = stats.norm.rvs(loc=10, scale=2, size=1000)

# Correlation
correlation, p_corr = stats.pearsonr(data1, data2)
```
* Wide range of probability distributions
* Hypothesis testing (t-test, chi-square, ANOVA, etc.)
* Distribution fitting and random sampling
* **Status**: Current, stable

### Numerical Integration ✅ Current
```python
from scipy import integrate
import numpy as np

# Integrate a function
def integrand(x):
    return np.exp(-x**2)

# Single integral from 0 to infinity
result, error = integrate.quad(integrand, 0, np.inf)
print(f"Integral result: {result:.6f}, error: {error:.2e}")

# Double integral
def integrand_2d(y, x):
    return x * y**2

result_2d, error_2d = integrate.dblquad(
    integrand_2d,
    0, 2,  # x from 0 to 2
    lambda x: 0, lambda x: 1  # y from 0 to 1
)

# Solve ODE: dy/dt = -2y, y(0) = 1
def dydt(y, t):
    return -2 * y

t = np.linspace(0, 4, 100)
y0 = 1
solution = integrate.odeint(dydt, y0, t)
```
* `quad` for single integrals, `dblquad` for double, `tplquad` for triple
* `odeint` for ordinary differential equations
* Returns result and error estimate
* **Status**: Current, stable

## Configuration

### Default Optimization Options
```python
from scipy.optimize import minimize

# Default tolerances and options
result = minimize(
    objective,
    x0,
    method='BFGS',
    options={
        'maxiter': 10000,      # Maximum iterations
        'disp': False,         # Display convergence messages
        'gtol': 1e-5,          # Gradient tolerance
        'eps': 1.4901161193847656e-08  # Step size for numerical derivatives
    }
)
```

### Controlling BLAS/LAPACK Threading
```python
import threadpoolctl

# SciPy uses multi-threaded BLAS/LAPACK by default
# Control threading behavior:
with threadpoolctl.threadpool_limits(limits=4, user_api='blas'):
    result = linalg.solve(A, b)

# Or set globally
threadpoolctl.threadpool_limits(limits=1)  # Force single-threaded
```

### Sparse Matrix Format Selection
```python
# Choose format based on operation:
# - CSR: fast row slicing, matrix-vector products
# - CSC: fast column slicing, matrix-vector products  
# - COO: fast format conversion, construction
# - LIL: fast incremental construction
# - DOK: fast element access and update

sparse_lil = sparse.lil_array((1000, 1000))  # Build incrementally
sparse_lil[0, 100] = 1
sparse_lil[500, 501] = 2

sparse_csr = sparse_lil.tocsr()  # Convert for computation
```

### Optimization Method Selection
```python
# Unconstrained: BFGS (default), Nelder-Mead, CG, Newton-CG
# Bound-constrained: L-BFGS-B, TNC
# Constrained: SLSQP, trust-constr (recommended)
# Global: differential_evolution, basinhopping, dual_annealing

result = minimize(objective, x0, method='trust-constr', constraints=constraints)
```

## Pitfalls

### Wrong: Using DIA sparse format for subscripting
```python
from scipy import sparse
import numpy as np

dense = np.array([[1, 0, 0], [0, 2, 0], [0, 0, 3]])
sparse_dia = sparse.dia_array(dense)

# This raises TypeError: 'dia_array' object is not subscriptable
value = sparse_dia[1, 1]
```

### Right: Convert to CSR format for indexing
```python
from scipy import sparse
import numpy as np

dense = np.array([[1, 0, 0], [0, 2, 0], [0, 0, 3]])
sparse_dia = sparse.dia_array(dense)

# Convert to CSR format which supports indexing
sparse_csr = sparse_dia.tocsr()
value = sparse_csr[1, 1]  # Works correctly
```

### Wrong: Explicit zeros taking up storage in sparse arrays
```python
from scipy import sparse
import numpy as np

# Including explicit zeros in data
row = [0, 0, 1, 1, 2, 2]
col = [0, 3, 1, 2, 2, 3]
data = [1, 2, 4, 1, 5, 0]  # The 0 at position (2,3) is stored explicitly

sparse_csr = sparse.csr_array((data, (row, col)))
print(f"Stored elements: {sparse_csr.nnz}")  # 6 elements stored
```

### Right: Remove explicit zeros after construction
```python
from scipy import sparse
import numpy as np

row = [0, 0, 1, 1, 2, 2]
col = [0, 3, 1, 2, 2, 3]
data = [1, 2, 4, 1, 5, 0]

sparse_csr = sparse.csr_array((data, (row, col)))
sparse_csr.eliminate_zeros()  # Remove explicit zeros
print(f"Stored elements: {sparse_csr.nnz}")  # 5 elements stored
```

### Wrong: Not handling duplicate entries in COO format
```python
from scipy import sparse
import numpy as np

# Multiple values at position (1,1)
row = [0, 0, 1, 1, 1, 2]
col = [0, 3, 1, 1, 2, 2]
data = [1, 2, 1, 3, 1, 5]

sparse_coo = sparse.coo_array((data, (row, col)))
print(f"Stored elements: {sparse_coo.nnz}")  # 6 stored elements
# Duplicates are summed when converted to dense, but stored separately
```

### Right: Consolidate duplicates explicitly
```python
from scipy import sparse
import numpy as np

row = [0, 0, 1, 1, 1, 2]
col = [0, 3, 1, 1, 2, 2]
data = [1, 2, 1, 3
## References

- [homepage](https://scipy.org/)
- [documentation](https://docs.scipy.org/doc/scipy/)
- [source](https://github.com/scipy/scipy)
- [download](https://github.com/scipy/scipy/releases)
- [tracker](https://github.com/scipy/scipy/issues)
