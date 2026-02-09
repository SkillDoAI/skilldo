---
name: matplotlib
version: 3.10
ecosystem: python
# license: Unknown
generated_with: qwen3-coder:latest + gpt-5.2 (agent5)
---

---
name: matplotlib
description: A comprehensive library for creating static, animated, and interactive visualizations in Python.
version: 3.10
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
import matplotlib.pyplot as plt
import matplotlib as mpl
from matplotlib import pyplot, figure, axes
from mpl_toolkits.axes_grid1 import make_axes_locatable
```

## Core Patterns

### Plotting Basic Line Graph ✅ Current
```python
import matplotlib.pyplot as plt

x = [1, 2, 3, 4]
y = [1, 4, 9, 16]

plt.plot(x, y)
plt.xlabel('X-axis')
plt.ylabel('Y-axis')
plt.title('Basic Line Plot')
plt.show()
```
* Creates a simple line plot using pyplot
* **Status**: Current, stable

### Saving Figure with Customization ✅ Current
```python
import matplotlib.pyplot as plt

fig, ax = plt.subplots()
ax.plot([1, 2, 3], [1, 4, 9])
ax.set_xlabel('X')
ax.set_ylabel('Y')

plt.savefig('my_plot.png', dpi=300, bbox_inches='tight')
plt.show()
```
* Saves a figure with specified DPI and tight bounding box
* **Status**: Current, stable

### Using rcParams for Global Styling ✅ Current
```python
import matplotlib as mpl

mpl.rcParams['font.size'] = 12
mpl.rcParams['axes.grid'] = True

import matplotlib.pyplot as plt
plt.plot([1, 2, 3], [1, 4, 9])
plt.show()
```
* Sets global styling parameters for all subsequent plots
* **Status**: Current, stable

### Creating Subplots with Shared Axes ✅ Current
```python
import matplotlib.pyplot as plt

fig, (ax1, ax2) = plt.subplots(2, 1, sharex=True)
ax1.plot([1, 2, 3], [1, 4, 9])
ax2.plot([1, 2, 3], [1, 8, 27])
plt.show()
```
* Creates multiple subplots sharing x-axis
* **Status**: Current, stable

### Configuring Backend and Interactive Mode ✅ Current
```python
import matplotlib
matplotlib.use('Agg')  # Set backend before importing pyplot
import matplotlib.pyplot as plt

matplotlib.interactive(False)
plt.plot([1, 2, 3], [1, 4, 9])
plt.show()
```
* Sets the rendering backend and disables interactive mode
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Default values: Default backend is 'Agg', interactive mode is off
- Common customizations: rcParams, backends, logging levels
- Environment variables: MPLBACKEND, MPLCONFIGDIR
- Config file formats: ~/.matplotlib/matplotlibrc, font configuration

## Pitfalls

### Wrong: Not importing pyplot before plotting
```python
import matplotlib as mpl
mpl.plot([1, 2, 3], [1, 4, 9])  # This will fail
```

### Right: Import pyplot for plotting functions
```python
import matplotlib.pyplot as plt
plt.plot([1, 2, 3], [1, 4, 9])  # Correct way
```

### Wrong: Using rcParams without proper reference
```python
import matplotlib as mpl
mpl.rcParams['font.size'] = 12  # This may cause issues if used inconsistently
```

### Right: Use rcParams properly with context manager
```python
import matplotlib as mpl
with mpl.rc_context({'font.size': 12}):
    import matplotlib.pyplot as plt
    plt.plot([1, 2, 3], [1, 4, 9])
```

### Wrong: Ignoring backend choice when using GUI
```python
import matplotlib.pyplot as plt
plt.plot([1, 2, 3], [1, 4, 9])
plt.show()  # May not work if backend not set properly
```

### Right: Set backend explicitly before importing pyplot
```python
import matplotlib
matplotlib.use('TkAgg')
import matplotlib.pyplot as plt
plt.plot([1, 2, 3], [1, 4, 9])
plt.show()
```

## References

- [Homepage](https://matplotlib.org)
- [Download](https://matplotlib.org/stable/install/index.html)
- [Documentation](https://matplotlib.org)
- [Source Code](https://github.com/matplotlib/matplotlib)
- [Bug Tracker](https://github.com/matplotlib/matplotlib/issues)
- [Forum](https://discourse.matplotlib.org/)
- [Donate](https://numfocus.org/donate-to-matplotlib)

## Migration from v3.9

What changed in this version (if applicable):
- Breaking changes: None significant in v3.10
- Deprecated → Current mapping: No major mapping changes
- Before/after code examples: None needed for current stable

## API Reference

Brief reference of the most important public APIs:

- **pyplot.plot()** - Creates line plots with x and y data
- **pyplot.show()** - Displays the current figure
- **pyplot.savefig()** - Saves the figure to a file
- **pyplot.subplots()** - Creates a figure and set of subplots
- **pyplot.figure()** - Creates a new figure
- **rcParams** - Dictionary for global rc settings
- **use()** - Sets the matplotlib backend
- **interactive()** - Sets interactive mode
- **get_backend()** - Returns the current backend name
- **colormaps** - Dictionary of available colormaps
- **pyplot.close()** - Closes a figure window
- **pyplot.clf()** - Clears the current figure
- **pyplot.gca()** - Gets the current axes
- **pyplot.gcf()** - Gets the current figure
- **pyplot.legend()** - Adds a legend to the axes
- **pyplot.grid()** - Adds a grid to the axes
- **pyplot.title()** - Sets title of axes
- **pyplot.xlabel() / ylabel()** - Sets x, y axis labels
- **rc_context()** - Context manager for rcParams
- **get_configdir()** - Returns configuration directory path
- **get_data_path()** - Returns matplotlib data path
- **get_cachedir()** - Returns cache directory path
- **set_loglevel()** - Sets the logging level for matplotlib
- **make_axes_locatable** - Utility to create axes for plots with subplots
- **host_subplot** - Alternative to subplot for advanced use cases
- **AxesGrid** - Grid layout for plotting multiple axes
- **inset_axes** - Creates inset axes inside a main plot
- **zoomed_inset_axes** - Creates zoomed-in axes for plots
- **mark_inset** - Connects axes with a zoomed-in view
- **AnchoredSizeBar** - Adds a scale bar to plots
- **ImageGrid** - Grid layout for images
- **BboxConnectorPatch** - Creates patches connecting bounding boxes
- **get_tightbbox** - Computes tight bounding box of a figure
- **defaultParams** - Default parameter dictionary
- **rcdefaults()** - Resets rcParams to defaults
- **rc_file()** - Loads rcParams from file
- **rc_params()** - Returns current rcParams
- **rc_params_from_file()** - Loads rcParams from file
- **RcParams** - Class representing rcParams
- **MatplotlibDeprecationWarning** - Warning class for deprecations
- **ExecutableNotFoundError** - Exception raised when executable not found
- **matplotlib_fname()** - Returns path to matplotlib config file
- **__version__ / __version_info__** - Version information properties
- **__bibtex__** - BibTeX citation for matplotlib
- **color_sequences** - Dictionary of color sequences
- **bivar_colormaps** - Dictionary of bivariate colormaps
- **multivar_colormaps** - Dictionary of multivariate colormaps
- **colormaps** - Dictionary of all available colormaps
- **get_backend()** - Returns the current backend
- **is_interactive()** - Returns True if interactive mode is on
- **set_loglevel()** - Sets log level for matplotlib
- **get_configdir()** - Returns config directory path
- **get_cachedir()** - Returns cache directory path
- **get_data_path()** - Returns matplotlib data path
- **matplotlib_fname()** - Returns matplotlib config file path
- **rc_file_defaults()** - Loads default rcParams from file
- **rc_file()** - Loads rcParams from file
- **rc()** - Sets rcParams
- **rcdefaults()** - Resets rcParams to defaults
- **use()** - Sets matplotlib backend
- **interactive()** - Sets interactive mode
- **is_interactive()** - Returns if interactive mode is on
- **get_backend()** - Returns current backend
- **set_loglevel()** - Sets logging level
- **get_configdir()** - Returns config directory path
- **get_cachedir()** - Returns cache directory path
- **get_data_path()** - Returns matplotlib data path
- **matplotlib_fname()** - Returns matplotlib config file path
- **RcParams** - Class representing rcParams
- **defaultParams** - Default parameters dictionary
- **rcParams** - Current rcParams dictionary
- **rcParamsDefault** - Default rcParams dictionary
- **rcParamsOrig** - Original rcParams dictionary
- **rc_params()** - Returns current rcParams
- **rc_params_from_file()** - Loads rcParams from file
- **rc_file()** - Loads rcParams from file
- **rc_file_defaults()** - Loads default rcParams from file
- **rc()** - Sets rcParams
- **rcdefaults()** - Resets rcParams to defaults
- **rc_context()** - Context manager for rcParams
- **use()** - Sets matplotlib backend
- **interactive()** - Sets interactive mode
- **is_interactive()** - Returns if interactive mode is on
- **get_backend()** - Returns current backend
- **set_loglevel()** - Sets logging level
- **get_configdir()** - Returns config directory path
- **get_cachedir()** - Returns cache directory path
- **get_data_path()** - Returns matplotlib data path
- **matplotlib_fname()** - Returns matplotlib config file path
- **MatplotlibDeprecationWarning** - Warning class for deprecations
- **ExecutableNotFoundError** - Exception raised when executable not found
- **__version__ / __version_info__** - Version information properties
- **__bibtex__** - BibTeX citation for matplotlib
- **color_sequences** - Dictionary of color sequences
- **bivar_colormaps** - Dictionary of bivariate colormaps
- **multivar_colormaps** - Dictionary of multivariate colormaps
- **colormaps** - Dictionary of all available colormaps
- **make_axes_locatable** - Utility to create axes for plots with subplots
- **host_subplot** - Alternative to subplot for advanced use cases
- **AxesGrid** - Grid layout for plotting multiple axes
- **inset_axes** - Creates inset axes inside a main plot
- **zoomed_inset_axes** - Creates zoomed-in axes for plots
- **mark_inset** - Connects axes with a zoomed-in view
- **AnchoredSizeBar** - Adds a scale bar to plots
- **ImageGrid** - Grid layout for images
- **BboxConnectorPatch** - Creates patches connecting bounding boxes
- **get_tightbbox** - Computes tight bounding box of a figure