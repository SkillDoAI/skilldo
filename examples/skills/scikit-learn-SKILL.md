---

name: scikit-learn
description: Machine learning library for classical supervised/unsupervised models, preprocessing, and model evaluation using NumPy/SciPy.
version: unknown
ecosystem: python
license: BSD-3-Clause
generated_with: gpt-5.2
---

## Imports

```python
import sklearn
from sklearn import datasets, metrics
from sklearn.model_selection import train_test_split, GridSearchCV
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler
from sklearn.linear_model import LogisticRegression
from sklearn.neighbors import NearestNeighbors
```

## Core Patterns

### Check installation + environment details ✅ Current
```python
import sklearn

if __name__ == "__main__":
    print("scikit-learn version:", sklearn.__version__)
    sklearn.show_versions()
```
* Use `sklearn.show_versions()` when debugging environment issues (BLAS/OpenMP, NumPy/SciPy versions, compiler info).

### Train/test split + fit/predict + metrics ✅ Current
```python
from sklearn import datasets, metrics
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)

    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    clf = LogisticRegression(max_iter=2000, solver="lbfgs")
    clf.fit(X_train, y_train)

    y_pred = clf.predict(X_test)
    print("accuracy:", metrics.accuracy_score(y_test, y_pred))
    print(metrics.classification_report(y_test, y_pred))
```
* Standard estimator workflow: `fit()` on train, `predict()` on test, evaluate with `sklearn.metrics`.

### Pipeline with preprocessing + model ✅ Current
```python
from sklearn import datasets, metrics
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)
    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    pipe = Pipeline(
        steps=[
            ("scaler", StandardScaler()),
            ("clf", LogisticRegression(max_iter=2000)),
        ]
    )

    pipe.fit(X_train, y_train)
    y_pred = pipe.predict(X_test)
    print("accuracy:", metrics.accuracy_score(y_test, y_pred))
```
* Prefer `Pipeline` to prevent train/test leakage (fit transformers only on training data).

### Hyperparameter search with GridSearchCV ✅ Current
```python
from sklearn import datasets, metrics
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import GridSearchCV, train_test_split
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)
    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    pipe = Pipeline(
        steps=[
            ("scaler", StandardScaler()),
            ("clf", LogisticRegression(max_iter=2000)),
        ]
    )

    param_grid = {
        "clf__C": [0.1, 1.0, 10.0],
        "clf__penalty": ["l2"],
        "clf__solver": ["lbfgs"],
    }

    search = GridSearchCV(
        estimator=pipe,
        param_grid=param_grid,
        scoring="accuracy",
        cv=5,
        n_jobs=-1,
    )
    search.fit(X_train, y_train)

    print("best params:", search.best_params_)
    y_pred = search.predict(X_test)
    print("test accuracy:", metrics.accuracy_score(y_test, y_pred))
```
* Use `step__param` names to tune pipeline components.
* Use `n_jobs=-1` to parallelize where supported.

### Nearest-neighbor index + query ✅ Current
```python
import numpy as np
from sklearn.neighbors import NearestNeighbors

if __name__ == "__main__":
    rng = np.random.RandomState(0)
    X = rng.randn(10, 3)
    query = rng.randn(2, 3)

    nn = NearestNeighbors(n_neighbors=3, metric="euclidean")
    nn.fit(X)

    distances, indices = nn.kneighbors(query, return_distance=True)
    print("indices:\n", indices)
    print("distances:\n", distances)
```
* `NearestNeighbors` provides unsupervised neighbor search via `fit()` + `kneighbors()`.

## Configuration

- **Randomness / reproducibility**
  - Many APIs accept `random_state=...` (e.g., `train_test_split`, many estimators). Prefer setting it in tests and benchmarks.
  - Test reproducibility can be controlled via the environment variable **`SKLEARN_SEED`** (used by scikit-learn’s test suite).
- **Installation verification**
  - Prefer `python -m pip ...` to avoid mismatched interpreter vs pip.
  - Useful commands:
    - `python -m pip show scikit-learn`
    - `python -m pip freeze`
    - `python -c "import sklearn; sklearn.show_versions()"`
- **Optional dependencies**
  - Plotting helpers (functions named `plot_...` and classes ending with `...Display`) require `matplotlib`.
- **Running tests**
  - Run from outside the source tree using `pytest sklearn` and ensure pytest meets the minimum required version (per scikit-learn’s developer docs).

## Pitfalls

### Wrong: Mixing OS package manager installs with pip installs (Linux)
```python
# This is a shell sequence, not Python; shown here because it causes Python import/runtime issues.
# sudo apt-get install python3-sklearn
# pip install -U scikit-learn
import sklearn

print(sklearn.__version__)
```

### Right: Use an isolated environment (venv) and verify with sklearn.show_versions()
```python
# Shell sequence:
# python3 -m venv sklearn-env
# source sklearn-env/bin/activate
# python -m pip install -U scikit-learn
# python -c "import sklearn; sklearn.show_versions()"

import sklearn

if __name__ == "__main__":
    sklearn.show_versions()
```

### Wrong: Using `pip` and `python` from different environments/interpreters
```python
# Shell sequence that can target different interpreters:
# pip install -U scikit-learn
# python -c "import sklearn; sklearn.show_versions()"

import sklearn
print(sklearn.__version__)
```

### Right: Always use `python -m pip` with the same interpreter you run
```python
# Shell sequence:
# python -m pip install -U scikit-learn
# python -m pip show scikit-learn
# python -c "import sklearn; sklearn.show_versions()"

import sklearn

if __name__ == "__main__":
    sklearn.show_versions()
```

### Wrong: Plotting API usage without Matplotlib installed
```python
# If matplotlib is not installed, importing plotting helpers can fail at import time.
from sklearn.inspection import PartialDependenceDisplay  # requires matplotlib

print(PartialDependenceDisplay)
```

### Right: Install matplotlib before using plot_* or *Display APIs
```python
# Shell:
# python -m pip install -U scikit-learn matplotlib

from sklearn.inspection import PartialDependenceDisplay

if __name__ == "__main__":
    print("PartialDependenceDisplay import OK:", PartialDependenceDisplay)
```

### Wrong: Data leakage by scaling before train/test split
```python
from sklearn import datasets, metrics
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import StandardScaler

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)

    scaler = StandardScaler()
    X_scaled = scaler.fit_transform(X)  # leakage: uses all data statistics

    X_train, X_test, y_train, y_test = train_test_split(
        X_scaled, y, test_size=0.25, random_state=0, stratify=y
    )

    clf = LogisticRegression(max_iter=2000)
    clf.fit(X_train, y_train)
    print(metrics.accuracy_score(y_test, clf.predict(X_test)))
```

### Right: Put preprocessing inside a Pipeline
```python
from sklearn import datasets, metrics
from sklearn.linear_model import LogisticRegression
from sklearn.model_selection import train_test_split
from sklearn.pipeline import Pipeline
from sklearn.preprocessing import StandardScaler

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)

    X_train, X_test, y_train, y_test = train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    pipe = Pipeline(
        steps=[("scaler", StandardScaler()), ("clf", LogisticRegression(max_iter=2000))]
    )
    pipe.fit(X_train, y_train)
    print(metrics.accuracy_score(y_test, pipe.predict(X_test)))
```

## References

- [homepage](https://scikit-learn.org)
- [source](https://github.com/scikit-learn/scikit-learn)
- [download](https://pypi.org/project/scikit-learn/#files)
- [tracker](https://github.com/scikit-learn/scikit-learn/issues)
- [release notes](https://scikit-learn.org/stable/whats_new)

## Migration from v[previous]

- **Python minimum version change (1.6 → 1.7)**:
  - Change: scikit-learn 1.7 requires **Python >= 3.10** (1.6 supported Python 3.9—3.13).
  - Action: upgrade Python first, then upgrade scikit-learn; or pin scikit-learn to `<=1.6.*` if you must stay on Python 3.9.

```python
# Example: guard in tooling/scripts that manage environments (runtime check)
import sys

if __name__ == "__main__":
    if sys.version_info < (3, 10):
        raise RuntimeError("scikit-learn >= 1.7 requires Python >= 3.10")
    print("Python version OK:", sys.version)
```

## API Reference

- **sklearn.show_versions()** - Print build/runtime dependency versions; use for debugging environment issues.
- **sklearn.datasets.load_breast_cancer(return_X_y=True)** - Load a built-in dataset; `return_X_y` returns `(X, y)`.
- **sklearn.model_selection.train_test_split()** - Split arrays into random train/test subsets; key params: `test_size`, `random_state`, `stratify`.
- **sklearn.pipeline.Pipeline(steps=...)** - Chain transformers and an estimator; prevents leakage; params: `steps`.
- **sklearn.preprocessing.StandardScaler()** - Standardize features by removing mean/scaling to unit variance.
- **sklearn.linear_model.LogisticRegression()** - Linear classifier; key params: `C`, `penalty`, `solver`, `max_iter`, `random_state`.
- **sklearn.model_selection.GridSearchCV(estimator, param_grid, cv=..., scoring=...)** - Exhaustive parameter search with CV; key params: `param_grid`, `cv`, `n_jobs`, `scoring`.
- **sklearn.metrics.accuracy_score(y_true, y_pred)** - Classification accuracy metric.
- **sklearn.metrics.classification_report(y_true, y_pred)** - Text summary of precision/recall/F1.
- **sklearn.neighbors.NearestNeighbors(n_neighbors=..., metric=...)** - Unsupervised neighbor search model.
- **NearestNeighbors.fit(X)** - Build neighbor index from training data.
- **NearestNeighbors.kneighbors(X, n_neighbors=..., return_distance=True)** - Query nearest neighbors; returns `(distances, indices)`.
- **pytest (run as: `pytest sklearn`)** - Test invocation convention used by scikit-learn’s test suite.
- **sklearnex.neighbors.NearestNeighbors** - Intel extension drop-in; requires `scikit-learn-intelex` and explicit enablement/patching per its docs.