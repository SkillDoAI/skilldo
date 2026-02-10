---

name: keras
description: Multi-backend deep learning library for building, training, running inference, and saving neural network models in Python.
version: unknown
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import keras
from keras import Model, layers
```

## Core Patterns

### Run inference with `Model.predict()` ✅ Current
```python
import os

# Backend must be set before importing keras.
os.environ["KERAS_BACKEND"] = "tensorflow"

import numpy as np
import keras
from keras import layers

model = keras.Sequential(
    [
        layers.Input(shape=(4,)),
        layers.Dense(8, activation="relu"),
        layers.Dense(3, activation="softmax"),
    ]
)

x = np.random.RandomState(0).randn(5, 4).astype("float32")
preds = model.predict(x, verbose=0)
print(preds.shape)
```
* Use `keras.Model.predict(x)` for forward-pass inference on NumPy arrays (and other backend-compatible inputs).
* Works with any supported backend; for OpenVINO, inference is the intended workflow.

### Save a model with `Model.save()` to the native `.keras` format ✅ Current
```python
import os
import tempfile

os.environ["KERAS_BACKEND"] = "tensorflow"

import keras
from keras import layers

model = keras.Sequential(
    [
        layers.Input(shape=(4,)),
        layers.Dense(8, activation="relu"),
        layers.Dense(1),
    ]
)

path = os.path.join(tempfile.gettempdir(), "example_model.keras")
model.save(path)
print("Saved to:", path)
```
* Prefer saving to a filename ending in `.keras` for the up-to-date Keras 3 native format (not legacy/ambiguous formats).

### Configure backend via environment before import ✅ Current
```python
import os

# Choose exactly one backend per process before importing keras.
# Valid values (depending on installed backend packages): "tensorflow", "jax", "torch", "openvino".
#
# This example uses "torch" because it doesn't require TensorFlow to be installed.
os.environ["KERAS_BACKEND"] = "torch"

import keras

print("Keras imported with backend:", os.environ["KERAS_BACKEND"])
```
* Backend selection is a pre-import configuration step; do not attempt to switch backends after `import keras`.

### OpenVINO backend for inference-only `Model.predict()` ✅ Current
```python
import os

# OpenVINO backend is intended for inference-only usage.
os.environ["KERAS_BACKEND"] = "openvino"

import numpy as np
import keras
from keras import layers

model = keras.Sequential(
    [
        layers.Input(shape=(4,)),
        layers.Dense(8, activation="relu"),
        layers.Dense(2),
    ]
)

x = np.random.RandomState(0).randn(3, 4).astype("float32")
y = model.predict(x, verbose=0)
print(y)
```
* Use OpenVINO backend to run predictions; do not use it for training workflows.

## Configuration

- **Backend selection (required for multi-backend)**:
  - Set `KERAS_BACKEND` **before** importing `keras`.
    - Valid values (per installation): `tensorflow`, `jax`, `torch`, `openvino` (inference-only).
  - Alternatively configure via `~/.keras/keras.json` **before** import.
- **Backend immutability**:
  - Backend cannot be changed reliably after `keras` is imported; restart the process/kernel to switch.
- **Installation convention**:
  - Install Keras 3 from PyPI as `keras`.
  - Keras 2 remains separately available as `tf-keras`.
  - Install at least one backend package alongside `keras`: `tensorflow`, `jax`, `torch` (and optionally `openvino` for inference-only).
- **GPU environments**:
  - Prefer separate environments per backend to avoid CUDA version mismatches; use backend-provided CUDA requirements files when applicable.

## Pitfalls

### Wrong: Setting `KERAS_BACKEND` after importing `keras`
```python
import keras
import os

os.environ["KERAS_BACKEND"] = "jax"  # too late; has no reliable effect
```

### Right: Set `KERAS_BACKEND` before importing `keras`
```python
import os

os.environ["KERAS_BACKEND"] = "jax"

import keras
```

### Wrong: Trying to train on the OpenVINO backend (inference-only)
```python
import os
os.environ["KERAS_BACKEND"] = "openvino"

import numpy as np
import keras
from keras import layers

model = keras.Sequential([layers.Input(shape=(4,)), layers.Dense(1)])
x = np.random.RandomState(0).randn(8, 4).astype("float32")
y = np.random.RandomState(1).randn(8, 1).astype("float32")

# Inference-only backend: training workflows like fit() are not supported.
model.fit(x, y, epochs=1)
```

### Right: Use OpenVINO backend for `Model.predict()` only
```python
import os
os.environ["KERAS_BACKEND"] = "openvino"

import numpy as np
import keras
from keras import layers

model = keras.Sequential([layers.Input(shape=(4,)), layers.Dense(1)])
x = np.random.RandomState(0).randn(8, 4).astype("float32")

preds = model.predict(x, verbose=0)
print(preds.shape)
```

### Wrong: Saving without an explicit `.keras` extension (ambiguous/legacy)
```python
import os
import tempfile

os.environ["KERAS_BACKEND"] = "tensorflow"

import keras
from keras import layers

model = keras.Sequential([layers.Input(shape=(4,)), layers.Dense(1)])

# May not use the up-to-date native Keras format.
model.save(os.path.join(tempfile.gettempdir(), "model"))
```

### Right: Save using the native `.keras` format
```python
import os
import tempfile

os.environ["KERAS_BACKEND"] = "tensorflow"

import keras
from keras import layers

model = keras.Sequential([layers.Input(shape=(4,)), layers.Dense(1)])

model.save(os.path.join(tempfile.gettempdir(), "model.keras"))
```

### Wrong: Expecting backend changes to apply within the same process
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import keras

# Attempting to switch after import leads to inconsistent behavior.
os.environ["KERAS_BACKEND"] = "torch"
```

### Right: Restart the process/kernel to change backend
```python
# Process A:
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import keras

# To switch to torch, start a new Python process/kernel:
# Process B:
# import os
# os.environ["KERAS_BACKEND"] = "torch"
# import keras
```

## References

- [Home](https://keras.io/)
- [Repository](https://github.com/keras-team/keras)

## Migration from v2 (tf.keras / tf-keras)

- **Packaging changed**
  - **Before (Keras 2)**: typically `tf.keras` (bundled with TensorFlow) or `tf-keras` on PyPI.
  - **Now (Keras 3)**: install/import `keras` from PyPI; Keras 2 remains available as `tf-keras`.

- **Backend selection is explicit and pre-import**
  - **Before**: backend implicitly TensorFlow via `tf.keras`.
  - **Now**: choose backend via `KERAS_BACKEND` env var or `~/.keras/keras.json` before importing `keras`.

- **Prefer the native `.keras` saving format**
  - **Before**: often SavedModel / H5 patterns.
  - **Now**: use `model.save("path/model.keras")` for the native Keras 3 format (especially when migrating).

Example (before/after):

```python
# Before (TensorFlow + tf.keras)
import tensorflow as tf

model = tf.keras.Sequential([tf.keras.layers.Input(shape=(4,)), tf.keras.layers.Dense(1)])
model.save("model_path")  # legacy/TF-specific defaults
```

```python
# After (Keras 3)
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import keras
from keras import layers

model = keras.Sequential([layers.Input(shape=(4,)), layers.Dense(1)])
model.save("model_path.keras")
```

## API Reference

- **keras** - Top-level package for Keras 3 (multi-backend); backend configured pre-import.
- **keras.Model** - Base class for models; exposes inference and saving APIs.
- **keras.Model.predict(x, verbose=...)** - Runs inference; key params: input `x`, verbosity.
- **keras.Model.save(filepath)** - Saves the model; prefer `*.keras` for native format.
- **keras.Model.compile(...)** - Configures training (backend-dependent); not supported for inference-only backends like OpenVINO.
- **keras.Model.fit(...)** - Training loop (when supported by backend).
- **keras.Sequential(layers=...)** - Linear stack model constructor.
- **keras.layers.Input(shape=...)** - Defines input shape for models.
- **keras.layers.Dense(units, activation=...)** - Fully-connected layer.
- **keras.layers.Dropout(rate)** - Regularization layer (training-time behavior).
- **keras.layers.Conv2D(filters, kernel_size, activation=...)** - 2D convolution layer (vision).
- **keras.layers.Flatten()** - Flattens spatial inputs to vectors.
- **keras.layers.Embedding(input_dim, output_dim)** - Token embedding layer (NLP).
- **keras.optimizers.Adam(learning_rate=...)** - Common optimizer for training.
- **keras.losses.CategoricalCrossentropy(from_logits=...)** - Common classification loss.