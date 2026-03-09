---
name: matplotlib
description: Python library for creating static, animated, and interactive visualizations.
version: 3.10.8
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import matplotlib
from matplotlib import rc_params, set_loglevel, MatplotlibDeprecationWarning
from matplotlib import get_cachedir, get_configdir, get_data_path, matplotlib_fname
```

## Core Patterns

### Inspect version and environment ✅ Current
```python
import matplotlib
from matplotlib import get_cachedir, get_configdir, get_data_path, matplotlib_fname

def main() -> None:
    print("matplotlib:", matplotlib.__version__)
    print("version_info:", matplotlib.__version_info__)
    print("configdir:", get_configdir())
    print("cachedir:", get_cachedir())
    print("data_path:", get_data_path())
    print("matplotlibrc:", matplotlib_fname())

if __name__ == "__main__":
    main()
```
* Use this to debug runtime environment issues (config, cache, bundled data, and the active matplotlibrc).

### Read and query rcParams ✅ Current
```python
from __future__ import annotations

import matplotlib
from matplotlib import rc_params

def main() -> None:
    params: matplotlib.RcParams = rc_params()

    # Read a value.
    backend = params.get("backend", None)
    # NOTE: backend may be an object (not necessarily a str) depending on environment/backend setup.
    print("backend:", backend)

    # Find all params matching a pattern.
    font_params: matplotlib.RcParams = params.find_all("font")
    print("num font-related rcParams:", len(font_params))

    # Copy for safe experimentation.
    params_copy: matplotlib.RcParams = params.copy()
    params_copy["figure.dpi"] = 150  # validated assignment via RcParams.__setitem__
    print("figure.dpi (copy):", params_copy["figure.dpi"])
    print("figure.dpi (original):", params["figure.dpi"])

if __name__ == "__main__":
    main()
```
* Use `matplotlib.rc_params()` to obtain a validated `RcParams` mapping, then query, filter, and copy it safely.

### Update rcParams with validated vs raw setters ✅ Current
```python
from __future__ import annotations

import matplotlib
from matplotlib import rc_params

def main() -> None:
    params: matplotlib.RcParams = rc_params()

    # Validated set (recommended): enforces types/allowed values.
    params["_internal.classic_mode"] = False if "_internal.classic_mode" in params else params.get("_internal.classic_mode", False)

    # Public-but-underscored helpers (documented as public in Matplotlib):
    # - _set: validated set
    # - _get: get with Matplotlib's internal semantics
    # - _update_raw: bypass some validation (use carefully)
    if "figure.dpi" in params:
        params._set("figure.dpi", 120)
        dpi = params._get("figure.dpi")
        print("figure.dpi via _get:", dpi)

    # Raw update is for advanced cases; keep values correct to avoid later failures.
    params._update_raw({"savefig.dpi": "figure"})
    print("savefig.dpi:", params["savefig.dpi"])

if __name__ == "__main__":
    main()
```
* Prefer `params[key] = value` / `RcParams.__setitem__` (validated). Use `_update_raw` only when you fully control inputs.

### Control Matplotlib logging ✅ Current
```python
import matplotlib

def main() -> None:
    matplotlib.set_loglevel("warning")
    # Common levels: "debug", "info", "warning", "error", "critical"
    print("log level set; version:", matplotlib.__version__)

if __name__ == "__main__":
    main()
```
* Use `matplotlib.set_loglevel()` to reduce noisy logs in production or increase verbosity while debugging.

### Handle Matplotlib deprecations with explicit warning class ✅ Current
```python
from __future__ import annotations

import warnings
from matplotlib import MatplotlibDeprecationWarning

def main() -> None:
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter("always", MatplotlibDeprecationWarning)

        # Example deprecation warning emission (for testing/demo purposes).
        warnings.warn("example deprecation", MatplotlibDeprecationWarning)

        print("captured deprecations:", len(caught))

if __name__ == "__main__":
    main()
```
* Use `MatplotlibDeprecationWarning` in warning filters/tests to handle staged deprecations explicitly.

### Handle missing external executables ✅ Current
```python
from __future__ import annotations

import shutil
import matplotlib

def require_executable(name: str) -> str:
    path = shutil.which(name)
    if path is None:
        raise matplotlib.ExecutableNotFoundError(f"Required executable not found on PATH: {name}")
    return path

def main() -> None:
    # Example: check for a tool your workflow needs.
    try:
        exe = require_executable("latex")
        print("Found latex at:", exe)
    except matplotlib.ExecutableNotFoundError as e:
        print("Cannot proceed:", e)

if __name__ == "__main__":
    main()
```
* Raise `matplotlib.ExecutableNotFoundError` (a `FileNotFoundError`) when your Matplotlib-adjacent workflow depends on external tools.

## Configuration

- **rcParams**: Central configuration mapping (validated keys/values). Retrieve with `matplotlib.rc_params()`.
- **Config directory**: `matplotlib.get_configdir()` (location of user config such as `matplotlibrc`).
- **Cache directory**: `matplotlib.get_cachedir()` (font cache and other cached artifacts).
- **Data path**: `matplotlib.get_data_path()` (bundled data shipped with Matplotlib).
- **Active config file**: `matplotlib.matplotlib_fname()` (path to the matplotlibrc in use).
- **Logging**: `matplotlib.set_loglevel(level: str)` to control Matplotlib’s internal logging verbosity.
- **API stability note**: In Matplotlib, visual output is treated as part of the public API; changes that alter appearance can be considered breaking.

## Pitfalls

### Wrong: Assuming `matplotlib.__version__` is a function
```python
import matplotlib

def main() -> None:
    # TypeError: 'str' object is not callable
    print(matplotlib.__version__())

if __name__ == "__main__":
    main()
```

### Right: Treat `__version__` as a string attribute
```python
import matplotlib

def main() -> None:
    print(matplotlib.__version__)
    print(matplotlib.__version_info__)

if __name__ == "__main__":
    main()
```

### Wrong: Mutating global rcParams when you only meant to experiment
```python
from matplotlib import rc_params

def main() -> None:
    params = rc_params()
    params["figure.dpi"] = 10  # affects subsequent figures in this process
    print("figure.dpi now:", params["figure.dpi"])

if __name__ == "__main__":
    main()
```

### Right: Work on a copy of rcParams for local experimentation
```python
from matplotlib import rc_params

def main() -> None:
    params = rc_params()
    local = params.copy()
    local["figure.dpi"] = 10
    print("local figure.dpi:", local["figure.dpi"])
    print("global figure.dpi:", params["figure.dpi"])

if __name__ == "__main__":
    main()
```

### Wrong: Using `_update_raw` with invalid values (can break later)
```python
from matplotlib import rc_params

def main() -> None:
    params = rc_params()
    # This may bypass normal validation and cause errors later when rendering/saving.
    params._update_raw({"figure.dpi": "not-a-number"})
    print("figure.dpi:", params["figure.dpi"])

if __name__ == "__main__":
    main()
```

### Right: Prefer validated assignment (or `_set`) for rcParams
```python
from matplotlib import rc_params

def main() -> None:
    params = rc_params()
    params["figure.dpi"] = 200  # validated
    print("figure.dpi:", params["figure.dpi"])

if __name__ == "__main__":
    main()
```

### Wrong: Catching the wrong exception type for missing executables
```python
import shutil
import matplotlib

def main() -> None:
    try:
        path = shutil.which("latex")
        if path is None:
            raise FileNotFoundError("latex not found")
    except matplotlib.ExecutableNotFoundError:
        # This block will not run because FileNotFoundError was raised instead.
        print("Handle missing executable")

if __name__ == "__main__":
    main()
```

### Right: Raise/catch `matplotlib.ExecutableNotFoundError` consistently
```python
import shutil
import matplotlib

def main() -> None:
    try:
        path = shutil.which("latex")
        if path is None:
            raise matplotlib.ExecutableNotFoundError("latex not found on PATH")
        print("latex:", path)
    except matplotlib.ExecutableNotFoundError as e:
        print("Handle missing executable:", e)

if __name__ == "__main__":
    main()
```

## References

- [Homepage](https://matplotlib.org)
- [Download](https://matplotlib.org/stable/install/index.html)
- [Documentation](https://matplotlib.org)
- [Source Code](https://github.com/matplotlib/matplotlib)
- [Bug Tracker](https://github.com/matplotlib/matplotlib/issues)
- [Forum](https://discourse.matplotlib.org/)
- [Donate](https://numfocus.org/donate-to-matplotlib)

## Migration from v[previous]

Matplotlib continues to use a staged deprecation lifecycle and strict API-stability policy.

- ⚠️ **Visual-output compatibility**: figure appearance is treated as API; style/colormap/aesthetic changes are considered breaking.
- ⚠️ **Deprecation workflow**: APIs are typically warned first (`MatplotlibDeprecationWarning`), then removed in later releases.
- Keep code and type hints aligned with deprecation transitions (renamed/deleted/keyword-only params).
- Review runtime warnings and release/API change notes before upgrading; pin versions when exact visual reproducibility is required.

## API Reference

- **matplotlib.__version__** - Version string (computed lazily via module `__getattr__`).
- **matplotlib.__version_info__** - Structured version info object.
- **matplotlib.__bibtex__** - BibTeX citation string.
- **matplotlib.set_loglevel(level)** - Set Matplotlib’s logging level.
- **matplotlib.get_configdir()** - Return the configuration directory path.
- **matplotlib.get_cachedir()** - Return the cache directory path.
- **matplotlib.get_data_path()** - Return the path to Matplotlib’s bundled data.
- **matplotlib.matplotlib_fname()** - Return the path to the active matplotlibrc file.
- **matplotlib.ExecutableNotFoundError** - Exception for missing external executables (subclass of `FileNotFoundError`).
- **matplotlib.MatplotlibDeprecationWarning** ⚠️ - Warning category used for Matplotlib deprecations.
- **matplotlib.RcParams** - Validated mapping for runtime configuration (rcParams).
- **matplotlib.rc_params(fail_on_error: bool = False)** - Load and return rcParams as an `RcParams` instance.
- **matplotlib.rc_params_from_file(fname, fail_on_error: bool = False, use_default_template: bool = True)** - Load rcParams from a file.
- **matplotlib.rcParamsDefault** - Property: default rcParams.
- **matplotlib.rcParams** - Property: the global rcParams, assignable.
- **matplotlib.rcParamsOrig** - Property: original rcParams at import.
- **matplotlib.defaultParams** - Property: default rcParams (legacy/alias).
- **matplotlib.rc(group, **kwargs)** - Set the current rc params for a group.
- **matplotlib.rcdefaults()** - Restore rcParams to their default settings.
- **matplotlib.rc_file_defaults()** - Restore rcParams from the default matplotlibrc file.
- **matplotlib.rc_file(fname, *, use_default_template: bool = True)** - Update rcParams from a specified file.
- **matplotlib.rc_context(rc=None, fname=None)** - Context manager to temporarily set rcParams.
- **matplotlib.use(backend, *, force: bool = True)** - Select backend; must be called before importing pyplot.
- **matplotlib.get_backend(*, auto_select: bool = True)** - Get the current backend name (or `None`).
- **matplotlib.interactive(b)** - Set interactive mode on or off.
- **matplotlib.is_interactive()** - Return whether interactive mode is on.
- **matplotlib.RcParams.find_all(pattern)** - Return a filtered `RcParams` matching a pattern.
- **matplotlib.RcParams.copy()** - Return a copy of the `RcParams`.
- **matplotlib.RcParams._set(key, val)** - Public (documented) helper to set a parameter with Matplotlib semantics.
- **matplotlib.RcParams._get(key)** - Public (documented) helper to get a parameter with Matplotlib semantics.
- **matplotlib.RcParams._update_raw(other_params)** - Update parameters with raw values (advanced use).

## Security

This SKILL.md teaches only safe, standard usage of the Matplotlib library. It does **not** instruct agents to access, modify, or transmit files or data outside the user's project directory. All examples are limited to configuration, debugging, and visualization. No destructive or privileged actions are included or permitted.

## Current Library State (from source analysis)

### API Surface
```json
{
  "library_category": "plotting_library",
  "apis": [
    {
      "name": "matplotlib.set_loglevel",
      "type": "function",
      "signature": "set_loglevel(level)",
      "signature_truncated": false,
      "return_type": "None",
      "module": "matplotlib.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "decorators": [],
      "deprecation": {
        "is_deprecated": false
      },
      "type_hints": {}
    },
    {
      "name": "matplotlib.ExecutableNotFoundError",
      "type": "class",
      "signature": "ExecutableNotFoundError()",
      "signature_truncated": false,
      "return_type": "ExecutableNotFoundError",
      "module": "matplotlib.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "bases": ["FileNotFoundError"],
      "decorators": [],
      "deprecation": {
        "is_deprecated": false
      }
    },
    {
      "name": "matplotlib.get_configdir",
      "type": "function",
      "signature": "get_configdir()",
      "signature_truncated": false,
      "return_type": "str",
      "module": "matplotlib.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "decorators": [
        {
          "name": "_logged_cached",
          "params": {
            "fmt": "CONFIGDIR=%s"
          }
        }
      ],
      "deprecation": {
        "is_deprecated": false
      }
    },
    {
      "name": "matplotlib.get_cachedir",
      "type": "function",
      "signature": "get_cachedir()",
      "signature_truncated": false,
      "return_type": "str",
      "module": "matplotlib.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "decorators": [
        {
          "name": "_logged_cached",
          "params": {
            "fmt": "CACHEDIR=%s"
          }
        }
      ],
      "deprecation": {
        "is_deprecated": false
      }
    },
    {
      "name": "matplotlib.get_data_path",
      "type": "function",
      "signature": "get_data_path()",
      "signature_truncated": false,
      "return_type": "str",
      "module": "matplotlib.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "decorators": [
        {
          "name": "_logged_cached",
          "params": {
            "fmt": "matplotlib data path: %s"
          }
        }
      ],
      "deprecation": {
        "is_deprecated": false
      }
    },
    {
      "name": "matplotlib.matplotlib_fname",
      "type": "function",
      "signature": "matplotlib_fname()",
      "signature_truncated": false,
      "return_type": "str",
      "module": "matplotlib.__init__",
      "publicity_score": "high",
      "module_type": "public",
      "decorators": [],
      "deprecation": {
        "is_deprecated": false
      }
    }
  ]
}
```

### Usage Patterns
- Prefer staged deprecation handling with `MatplotlibDeprecationWarning` ⚠️ for APIs in transition.
- For `mpl_toolkits.axes_grid1`, tested patterns include `ImageGrid`, `AxesGrid`, `Grid`, inset helpers, host/parasite axes, and divider sizing utilities.
- Use pytest-based warning and error assertions when validating behavior and deprecations.

### Documentation & Changelog
- Matplotlib policy treats visual output as part of public API.
- Deprecations are staged: warning first, removal later.
- Keep new internal helpers private (leading underscore) unless intentionally public API.
- Keep runtime behavior, documentation version directives, and type hints consistent during deprecation windows.