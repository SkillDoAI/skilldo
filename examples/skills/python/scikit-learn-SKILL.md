---

name: scikit-learn
description: Machine learning library for classical supervised/unsupervised models, preprocessing, and model evaluation using NumPy/SciPy.
license: BSD-3-Clause
metadata:
  version: "1.8.0"
  ecosystem: python
  generated-by: skilldo/gpt-oss-120b + review:gpt-oss-120b
---

## Imports

```python
import sklearn
from sklearn import datasets, metrics
import sklearn.model_selection as model_selection
import sklearn.pipeline as pipeline
import sklearn.preprocessing as preprocessing
import sklearn.linear_model as linear_model
import sklearn.neighbors as neighbors
import sklearn.compose as compose
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
* The output starts with “System:” in recent scikit‑learn releases.

### Train/test split + fit/predict + metrics ✅ Current
```python
from sklearn import datasets, metrics
import sklearn.model_selection as model_selection
import sklearn.linear_model as linear_model

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)

    X_train, X_test, y_train, y_test = model_selection.train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    clf = linear_model.LogisticRegression(max_iter=2000, solver="lbfgs")
    clf.fit(X_train, y_train)

    y_pred = clf.predict(X_test)
    print("accuracy:", metrics.accuracy_score(y_test, y_pred))
    print(metrics.classification_report(y_test, y_pred))
```
* Standard estimator workflow: `fit()` on train, `predict()` on test, evaluate with `sklearn.metrics`.

### Pipeline with preprocessing + model ✅ Current
```python
from sklearn import datasets, metrics
import sklearn.pipeline as pipeline
import sklearn.preprocessing as preprocessing
import sklearn.linear_model as linear_model
import sklearn.model_selection as model_selection

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)
    X_train, X_test, y_train, y_test = model_selection.train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    pipe = pipeline.Pipeline(
        steps=[
            ("scaler", preprocessing.StandardScaler()),
            ("clf", linear_model.LogisticRegression(max_iter=2000)),
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
import sklearn.linear_model as linear_model
import sklearn.model_selection as model_selection
import sklearn.pipeline as pipeline
import sklearn.preprocessing as preprocessing

if __name__ == "__main__":
    X, y = datasets.load_breast_cancer(return_X_y=True)
    X_train, X_test, y_train, y_test = model_selection.train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    pipe = pipeline.Pipeline(
        steps=[
            ("scaler", preprocessing.StandardScaler()),
            ("clf", linear_model.LogisticRegression(max_iter=2000)),
        ]
    )

    param_grid = {
        "clf__C": [0.1, 1.0, 10.0],
        "clf__penalty": ["l2"],
        "clf__solver": ["lbfgs"],
    }

    search = model_selection.GridSearchCV(
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
import sklearn.neighbors as neighbors

if __name__ == "__main__":
    rng = np.random.RandomState(0)
    X = rng.randn(10, 3)
    query = rng.randn(2, 3)

    nn = neighbors.NearestNeighbors(n_neighbors=3, metric="euclidean")
    nn.fit(X)

    distances, indices = nn.kneighbors(query, return_distance=True)
    print("indices:\n", indices)
    print("distances:\n", distances)
```
* `NearestNeighbors` provides unsupervised neighbor search via `fit()` + `kneighbors()`.

### ColumnTransformer + heterogeneous preprocessing ✅ Current
```python
import numpy as np
import sklearn.compose as compose
import sklearn.preprocessing as preprocessing
import sklearn.pipeline as pipeline
import sklearn.linear_model as linear_model
import sklearn.model_selection as model_selection
from sklearn import datasets, metrics

if __name__ == "__main__":
    # Load a dataset with both numeric and categorical features
    X, y = datasets.load_breast_cancer(return_X_y=True)
    # For illustration, create a fake categorical column
    cat = (X[:, 0] > X[:, 0].mean()).astype(int).reshape(-1, 1)
    X_num = X[:, 1:]                # numeric part
    X = np.hstack([X_num, cat])     # combine

    # Column indices: last column is categorical
    preprocessor = compose.ColumnTransformer(
        transformers=[
            ("num", preprocessing.StandardScaler(), slice(0, -1)),
            ("cat", preprocessing.OneHotEncoder(drop="if_binary"), -1),
        ]
    )

    pipe = pipeline.Pipeline(
        steps=[
            ("preprocess", preprocessor),
            ("clf", linear_model.LogisticRegression(max_iter=2000)),
        ]
    )

    X_train, X_test, y_train, y_test = model_selection.train_test_split(
        X, y, test_size=0.25, random_state=0, stratify=y
    )

    pipe.fit(X_train, y_train)
    y_pred = pipe.predict(X_test)
    print("accuracy:", metrics.accuracy_score(y_test, y_pred))
```
* `ColumnTransformer` lets you apply different preprocessing pipelines to subsets of columns (e.g., scaling numeric features and one‑hot‑encoding categoricals) before feeding them to a model.

## Configuration

- **Randomness / reproducibility**
  - Many APIs accept `random_state=...` (e.g., `train_test_split`, many estimators). Prefer setting it in tests and benchmarks.
  - Test reproducibility can be controlled via the environment variable **`SKLEARN_SEED`** (used by scikit‑learn’s test suite).
- **Installation verification**
  - Prefer `python -m pip ...` to avoid mismatched interpreter vs pip.
  - Useful commands:
    - `python -m pip show scikit-learn`
    - `python -m pip freeze`
    - `python -c "import sklearn; sklearn.show_versions()"`
- **Optional dependencies**
  - Plotting helpers (functions named `plot_...` and classes ending with `...Display`) require `matplotlib`.
- **Running tests**
  - Run from outside the source tree using `pytest sklearn` and ensure pytest meets the minimum required version (per scikit‑learn’s developer docs).

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

### Migration from previous release

- **Python version**: scikit-learn 1.5+ requires **Python ≥ 3.9**. Environments on older interpreters will raise an import‑time error.
- **Hard‑deprecated APIs**: `sklearn.decomposition.FactorAnalysis` is slated for removal in 1.6; replace it with `PCA(whiten=True)` or another suitable transformer. ⚠️
- **Soft‑deprecated API**: `sklearn.utils.estimator_html_repr` has moved to a private submodule; continue to use it but import from `sklearn.utils._estimator_html_repr._estimator_html_repr`. ⚠️
- **General upgrade guide** (see the full “Migration notes” section below):
  1. Verify Python version (≥ 3.9).
  2. Re‑install in a clean virtual environment.
  3. Run `python -c "import sklearn; sklearn.show_versions()"` before and after the upgrade.
  4. Re‑install `scikit-learn-intelex` if you use Intel‑optimized solvers.
  5. Review deprecation warnings and migrate accordingly.

```python
# Example: Python version guard (useful when upgrading from ≤1.4)
import sys

if __name__ == "__main__":
    if sys.version_info < (3, 9):
        raise RuntimeError("scikit-learn >= 1.5 requires Python >= 3.9")
    print("Python version OK:", sys.version)
```

### API Reference

- **sklearn.__version__** – The installed scikit-learn version (string, e.g., `'1.8.0'`).
- **sklearn.show_versions()** – Print build/runtime dependency versions; use for debugging environment issues.
- **sklearn.clone(estimator, *, safe=True)** – Clone a scikit-learn estimator; returns a new, unfitted estimator with the same parameters.
- **sklearn.get_config()** – Get current global scikit-learn configuration as a dict.
- **sklearn.set_config(**kwargs)** – Set global scikit-learn configuration (keyword‑only config entries).
- **sklearn.config_context(**kwargs)** – Context manager for temporarily setting scikit-learn config.
- **sklearn.datasets.load_breast_cancer(return_X_y=True)** – Load a built‑in dataset; `return_X_y` returns `(X, y)`.
- **sklearn.model_selection.train_test_split()** – Split arrays into random train/test subsets; key params: `test_size`, `random_state`, `stratify`.
- **sklearn.pipeline.Pipeline(steps=...)** – Chain transformers and an estimator; prevents leakage; params: `steps`.
- **sklearn.preprocessing.StandardScaler()** – Standardize features by removing mean and scaling to unit variance.
- **sklearn.linear_model.LogisticRegression()** – Linear classifier; key params: `C`, `solver`, `max_iter`, `random_state`, etc.
- **sklearn.model_selection.GridSearchCV(estimator, param_grid, cv=..., scoring=..., n_jobs=...)** – Exhaustive parameter search with CV; key params: `param_grid`, `cv`, `n_jobs`, `scoring`.
- **sklearn.metrics.accuracy_score(y_true, y_pred)** – Classification accuracy metric.
- **sklearn.metrics.classification_report(y_true, y_pred)** – Text summary of precision/recall/F1.
- **sklearn.neighbors.NearestNeighbors(n_neighbors=..., metric=...)** – Unsupervised neighbor search model.
- **sklearn.neighbors.NearestNeighbors.fit(X)** – Build neighbor index from training data.
- **sklearn.neighbors.NearestNeighbors.kneighbors(X, n_neighbors=..., return_distance=True)** – Query nearest neighbors; returns `(distances, indices)`.
- **sklearn.compose.ColumnTransformer(transformers, remainder='drop')** – Apply different preprocessing pipelines to column subsets.
- **pytest (run as: `pytest sklearn`)** – Test invocation convention used by scikit-learn’s test suite.
- **sklearnex.neighbors.NearestNeighbors** – Intel‑optimized drop‑in; requires `scikit-learn-intelex` and explicit import from `sklearnex`.
- **sklearn.decomposition.FactorAnalysis** – ⚠️ Hard‑deprecated; scheduled for removal in 1.6. Use `PCA(whiten=True)` instead.
- **sklearn.utils.estimator_html_repr** – ⚠️ Soft‑deprecated; moved to `sklearn.utils._estimator_html_repr._estimator_html_repr`.

### Migration notes

#### General upgrade guide
1. **Check your Python version** – consult the warning box in `doc/install.rst` for the minimum supported version of each scikit‑learn release. Upgrade the interpreter if needed.
2. **Validate environment** – run `python -c "import sklearn; sklearn.show_versions()"` before and after the upgrade to ensure NumPy, SciPy, joblib, and threadpoolctl versions match the requirements.
3. **Re‑install in a clean virtual environment** – this avoids leftover compiled extensions from older releases.
4. **If you rely on Intel‑optimized solvers** – after upgrading, reinstall `scikit-learn-intelex` and import from the `sklearnex` namespace as shown in the examples.
5. **Review deprecation warnings** – the release notes list functions/classes that have been removed or renamed. Replace them with the suggested alternatives.
6. **Plotting utilities** – ensure Matplotlib ≥ 3.6.1 is installed; otherwise `plot_*` functions will raise `ImportError`.
7. **Hard‑deprecated APIs** – `FactorAnalysis` will be removed in 1.6; migrate to `PCA` with whitening or another suitable transformer.

#### Breaking changes
- **Python version bump**: Versions prior to 1.5 supported Python 3.8; from 1.5 onward Python ≥ 3.9 is required.
- **Dependency minimums**: NumPy ≥ 1.24.1, SciPy ≥ 1.10.0 from 1.5. Ensure these are upgraded before installing scikit‑learn 1.5+.
- **Intel‑optimized imports**: Estimators that were previously accessed via `sklearn` now require `sklearnex` (e.g., `from sklearnex.neighbors import NearestNeighbors`).

### Documentation & Changelog
```json
{
  "documented_apis": [
    "sklearn.show_versions",
    "sklearnex.neighbors.NearestNeighbors",
    "sklearnex.neighbors",
    "sklearn"
  ],
  "conventions": [
    "Prefer installing scikit‑learn in an isolated environment (virtualenv or conda) to avoid dependency conflicts.",
    "Use the latest stable release via ``pip install -U scikit‑learn`` or ``conda install -c conda-forge scikit-learn`` unless a specific older version is required.",
    "Verify the installation with ``python -c \"import sklearn; sklearn.show_versions()\"`` which prints the versions of scikit‑learn and its core dependencies.",
    "When using the Intel‑optimized package (scikit‑learn‑intelex), import estimators from ``sklearnex`` (e.g. ``from sklearnex.neighbors import NearestNeighbors``) and let the library patch the standard scikit‑learn classes automatically.",
    "Plotting utilities follow a naming convention: functions start with ``plot_`` and visual‑display classes end with ``Display`` (e.g. ``plot_roc_curve``, ``RocCurveDisplay``).",
    "Always install binary wheels for NumPy and SciPy (the default on PyPI) to avoid expensive recompilation on platforms like Raspberry Pi.",
    "Keep the Python runtime within the supported range for the scikit‑learn version you are using (see the version‑support warning box)."
  ],
  "pitfalls": [
    {
      "category": "Environment Management",
      "wrong": "pip install scikit-learn\npython -c \"import sklearn; sklearn.show_versions()\"  # run without a virtual environment",
      "why": "Installing into the global environment can clash with other packages (e.g., different NumPy/SciPy versions) and makes reproducibility difficult.",
      "right": "python -m venv sklearn-env\nsource sklearn-env/bin/activate  # or .\\sklearn-env\\Scripts\\activate on Windows\npip install -U scikit-learn\npython -c \"import sklearn; sklearn.show_versions()\""
    },
    {
      "category": "Binary Wheels",
      "wrong": "pip install --no-binary :all: scikit-learn  # forces source build",
      "why": "Compiling NumPy/SciPy from source is slow, may fail on many platforms, and can produce ABI incompatibilities.",
      "right": "pip install -U scikit-learn   # let pip fetch pre‑built wheels; only build from source if you explicitly need a custom build."
    },
    {
      "category": "Python Version Compatibility",
      "wrong": "pip install scikit-learn==0.20   # on Python 3.11",
      "why": "scikit‑learn 0.20 only supports Python 2.7 and 3.4; installing it on newer Python versions leads to import errors.",
      "right": "Check the supported Python range (see the warning box) and install a version that matches your interpreter, e.g. ``pip install scikit-learn==1.5`` for Python 3.9+."
    },
    {
      "category": "Intel‑optimized package misuse",
      "wrong": "from sklearn.neighbors import NearestNeighbors  # expecting Intel‑accelerated version",
      "why": "The Intel‑optimized solvers are only activated when importing from ``sklearnex``; importing from the standard namespace falls back to the regular implementation.",
      "right": "from sklearnex.neighbors import NearestNeighbors   # automatically patches the standard estimator"
    },
    {
      "category": "Activation of virtual environment",
      "wrong": "pip install -U scikit-learn\n# open a new terminal and run python without re‑activating the environment",
      "why": "The new shell does not have the virtual‑environment's ``PATH`` modifications, so the global Python (and possibly an older scikit‑learn) is used.",
      "right": "Always activate the environment in each new terminal session before running Python commands."
    }
  ],
  "breaking_changes": [
    {
      "version_from": "0.20",
      "version_to": "1.5",
      "change": "Python version support was raised; 0.20 was the last version to support Python 2.7 and 3.4, while 1.5 requires Python ≥ 3.9. Import paths for Intel‑optimized estimators changed from ``sklearn`` to ``sklearnex``.",
      "migration": "Upgrade your Python interpreter to at least the minimum version required by the target scikit‑learn release. Replace any ``from sklearn...`` imports of Intel‑optimized estimators with ``from sklearnex...``. Verify the installation with ``sklearn.show_versions()``."
    },
    {
      "version_from": "1.4",
      "version_to": "1.5",
      "change": "Dependency minimum versions were bumped (e.g., NumPy ≥ 1.24.1, SciPy ≥ 1.10.0). Some internal APIs were renamed to follow the ``plot_`` / ``*Display`` convention more strictly.",
      "migration": "Update NumPy, SciPy, and ... (truncated for brevity)"
    }
  ],
  "migration_notes": "Refer to the official *What’s New* page (https://scikit-learn.org/dev/whats_new.html) for a detailed migration guide. In short:\n\n1. **Check Python compatibility** – ensure your interpreter satisfies the version matrix shown in the warning box of the installation guide.\n2. **Upgrade core dependencies** – run ``pip install -U \"numpy>=1.24.1\" \"scipy>=1.10.0\"`` before upgrading scikit‑learn.\n3. **Run ``sklearn.show_versions()`` after the upgrade to confirm that all versions are consistent.\n4. **If you rely on Intel‑optimized solvers**, change imports to the ``sklearnex`` namespace and install ``scikit‑learn-intelex`` via ``pip install scikit‑learn-intelex`` or ``conda install scikit‑learn-intelex``.\n5. **Update plotting code** – use functions prefixed with ``plot_`` and the corresponding ``*Display`` classes; older function names may have been removed.\n6. **Re‑run the test suite** (``pytest sklearn``) in an isolated environment to catch any subtle behavior changes.\n\nFollowing these steps will smooth the transition from older releases to the current 1.5 series."
}
```
