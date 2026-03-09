---

name: keras
description: Multi-backend deep learning library for building, training, running inference, and saving neural network models in Python.
license: MIT
metadata:
  version: "3.13.2"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import tensorflow as tf  # Requires tensorflow to be installed
import keras
from keras import Model, layers
```

> **Note:** The imports above require `tensorflow` to be installed (`pip install tensorflow`). If TensorFlow is not installed, `import keras` will fail when `KERAS_BACKEND="tensorflow"` because Keras requires at least one supported backend to be installed and importable. In environments where `tensorflow` is not installed, all `import keras` statements will raise `ModuleNotFoundError`. Install at least one supported backend before importing `keras`.

## Core Patterns

### Run inference with `Model.predict()` ✅ Current
```python
# /// script
# dependencies = ["keras==3.13.2", "tensorflow==2.18.0", "numpy==2.2.5"]
# ///
import os

os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
from keras import layers
import numpy as np

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
* Always set `KERAS_BACKEND` to `"tensorflow"` before importing `keras`; `"tensorflow"` is the most widely available backend and must always be used in these examples.
* Requires `tensorflow` to be installed; set `KERAS_BACKEND="tensorflow"` before importing `keras`.
* Never use `KERAS_BACKEND="numpy"` in these examples — the numpy backend requires `scipy` and is not a general-purpose backend.

### Save a model with `Model.save()` to the native `.keras` format ✅ Current
```python
# /// script
# dependencies = ["keras==3.13.2", "tensorflow==2.18.0"]
# ///
import os
import tempfile

os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
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
* Always set `KERAS_BACKEND` to `"tensorflow"` before importing `keras`; `"tensorflow"` is the required backend for these examples.
* Requires `tensorflow` to be installed; set `KERAS_BACKEND="tensorflow"` before importing `keras`.
* Never use `KERAS_BACKEND="numpy"` in these examples — the numpy backend requires `scipy` and is not a general-purpose backend.

### Configure backend via environment before import ✅ Current
```python
# /// script
# dependencies = ["keras==3.13.2", "tensorflow==2.18.0", "numpy==2.2.5"]
# ///
import os

# Must set KERAS_BACKEND to "tensorflow" before importing keras.
# Never use "numpy" backend — it requires scipy and is not general-purpose.
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
import numpy as np

model = keras.models.Sequential([
    keras.layers.Input(shape=(4,)),
    keras.layers.Dense(2, activation="relu")
])

x = np.random.rand(3, 4).astype(np.float32)
y = model(x)

print("Keras imported with backend:", keras.config.backend())
```
* Backend selection is a pre-import configuration step; do not attempt to switch backends after `import keras`.
* Valid backend values (depending on installation): `"tensorflow"`, `"jax"`, `"torch"`, `"openvino"` (inference-only).
* Always use `"tensorflow"` as the backend in these examples — it is the most widely available backend and requires no additional packages beyond `tensorflow`.
* Requires `tensorflow` to be installed; set `KERAS_BACKEND="tensorflow"` before importing `keras`. The critical requirement is setting the env var before any `keras` import; explicitly importing `tensorflow` before `keras` is a common convention but not strictly required.
* Never use `KERAS_BACKEND="numpy"` in these examples — the numpy backend requires `scipy` and is not a general-purpose backend.

### OpenVINO backend for inference-only `Model.predict()` ✅ Current
```python
import os

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

### Build and train a model with `Model.fit()` ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
from keras import layers
import numpy as np

model = keras.Sequential([
    layers.Input(shape=(8,)),
    layers.Dense(16, activation="relu"),
    layers.Dense(1),
])

model.compile(optimizer="adam", loss="mse")

x = np.random.rand(64, 8).astype("float32")
y = np.random.rand(64, 1).astype("float32")

history = model.fit(x, y, epochs=3, batch_size=16, verbose=0)
print("Final loss:", history.history["loss"][-1])
```

### Subclassing `Model` for custom training logic ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
from keras import layers
import numpy as np


class MyModel(keras.Model):
    def __init__(self, **kwargs):
        super().__init__(**kwargs)
        self.dense1 = layers.Dense(16, activation="relu")
        self.dense2 = layers.Dense(1)

    def call(self, x):
        x = self.dense1(x)
        return self.dense2(x)

    def train_step(self, data):
        x, y = data
        with tf.GradientTape() as tape:
            y_pred = self(x, training=True)
            loss = self.compute_loss(y=y, y_pred=y_pred)
        gradients = tape.gradient(loss, self.trainable_variables)
        self.optimizer.apply_gradients(zip(gradients, self.trainable_variables))
        return {"loss": loss}


model = MyModel()
model.compile(optimizer="adam", loss="mse")
x = np.random.rand(32, 8).astype("float32")
y = np.random.rand(32, 1).astype("float32")
model.fit(x, y, epochs=2, verbose=0)
```

### Custom layer with `build()` and `get_config()` ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
import numpy as np


@keras.saving.register_keras_serializable(package="MyPackage")
class LinearLayer(keras.Layer):
    def __init__(self, units, **kwargs):
        super().__init__(**kwargs)
        self.units = units

    def build(self, input_shape):
        self.kernel = self.add_weight(
            shape=(input_shape[-1], self.units),
            initializer="glorot_uniform",
            name="kernel",
        )
        self.bias = self.add_weight(
            shape=(self.units,),
            initializer="zeros",
            name="bias",
        )
        super().build(input_shape)

    def call(self, x):
        return keras.ops.matmul(x, self.kernel) + self.bias

    def get_config(self):
        config = super().get_config()
        config.update({"units": self.units})
        return config


layer = LinearLayer(units=4)
x = np.random.rand(3, 8).astype("float32")
out = layer(x)
print(out.shape)
```
* Define weights in `build()` (not `__init__`) so that input shapes are known.
* Always implement `get_config()` for serialization; accept and forward `**kwargs` to `super().__init__()`.
* Use `@keras.saving.register_keras_serializable()` for custom classes used in saved models.

### Backend-agnostic ops with `keras.ops` ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
import numpy as np


@keras.saving.register_keras_serializable(package="MyPackage")
class NormLayer(keras.Layer):
    """Layer that L2-normalizes its input — works on any backend."""

    def call(self, x):
        # Use keras.ops instead of tf.*, torch.*, or jax.* for portability
        norm = keras.ops.sqrt(keras.ops.sum(keras.ops.power(x, 2), axis=-1, keepdims=True))
        return x / (norm + 1e-8)

    def get_config(self):
        return super().get_config()


layer = NormLayer()
x = np.random.rand(4, 6).astype("float32")
out = layer(x)
print(out.shape)
```

### Mixed-precision training ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
from keras import layers
import numpy as np

keras.mixed_precision.set_global_policy("mixed_float16")

model = keras.Sequential([
    layers.Input(shape=(8,)),
    layers.Dense(16, activation="relu"),
    layers.Dense(1, dtype="float32"),  # Keep output in float32 for numerical stability
])

model.compile(optimizer="adam", loss="mse")
x = np.random.rand(32, 8).astype("float32")
y = np.random.rand(32, 1).astype("float32")
model.fit(x, y, epochs=2, verbose=0)
print("Compute dtype:", model.layers[1].compute_dtype)
```

### Reproducibility with `keras.utils.set_random_seed()` ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
import numpy as np

keras.utils.set_random_seed(42)  # Seeds Python, NumPy, and the backend RNG

model = keras.Sequential([
    keras.layers.Input(shape=(4,)),
    keras.layers.Dense(2),
])
x = np.random.rand(3, 4).astype("float32")
print(model(x))
```

### Using pretrained application models ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
import numpy as np

# Load MobileNetV2 with ImageNet weights (downloads on first call)
base_model = keras.applications.MobileNetV2(
    include_top=False,
    weights=None,  # Use weights='imagenet' in practice; None avoids download in examples
    input_shape=(224, 224, 3),
    pooling="avg",
)

x = np.random.rand(2, 224, 224, 3).astype("float32")
features = base_model(x, training=False)
print(features.shape)
```

### Saving and loading models ✅ Current
```python
import os
import tempfile
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
from keras import layers

model = keras.Sequential([
    layers.Input(shape=(4,)),
    layers.Dense(8, activation="relu"),
    layers.Dense(1),
])

path = os.path.join(tempfile.gettempdir(), "model.keras")
model.save(path)

loaded = keras.saving.load_model(path)
print(loaded.summary())
```

### Callbacks with `Model.fit()` ✅ Current
```python
import os
import tempfile
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
from keras import layers
import numpy as np

model = keras.Sequential([
    layers.Input(shape=(8,)),
    layers.Dense(16, activation="relu"),
    layers.Dense(1),
])
model.compile(optimizer="adam", loss="mse")

checkpoint_path = os.path.join(tempfile.gettempdir(), "best_model.keras")
callbacks = [
    keras.callbacks.EarlyStopping(monitor="loss", patience=2),
    keras.callbacks.ModelCheckpoint(filepath=checkpoint_path, save_best_only=True),
]

x = np.random.rand(64, 8).astype("float32")
y = np.random.rand(64, 1).astype("float32")
model.fit(x, y, epochs=10, callbacks=callbacks, verbose=0)
```

### Custom `PyDataset` for data loading ✅ Current
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
import numpy as np


class MyDataset(keras.utils.PyDataset):
    def __init__(self, x, y, batch_size, **kwargs):
        super().__init__(**kwargs)
        self.x = x
        self.y = y
        self.batch_size = batch_size

    def __len__(self):
        return int(np.ceil(len(self.x) / self.batch_size))

    def __getitem__(self, idx):
        start = idx * self.batch_size
        end = start + self.batch_size
        return self.x[start:end], self.y[start:end]


x = np.random.rand(100, 4).astype("float32")
y = np.random.rand(100, 1).astype("float32")
dataset = MyDataset(x, y, batch_size=16)

model = keras.Sequential([
    keras.layers.Input(shape=(4,)),
    keras.layers.Dense(1),
])
model.compile(optimizer="adam", loss="mse")
model.fit(dataset, epochs=2, verbose=0)
```
* Use `keras.utils.PyDataset` (not the deprecated `keras.utils.Sequence`) for Python-based data generators.

### Quantization ✅ Current
```python
import os
import tempfile
os.environ["KERAS_BACKEND"] = "tensorflow"

import tensorflow as tf
import keras
from keras import layers
import numpy as np

model = keras.Sequential([
    layers.Input(shape=(8,)),
    layers.Dense(16, activation="relu"),
    layers.Dense(1),
])
model.compile(optimizer="adam", loss="mse")

x = np.random.rand(32, 8).astype("float32")
y = np.random.rand(32, 1).astype("float32")
model.fit(x, y, epochs=1, verbose=0)

# Quantize to int8 after training
model.quantize("int8")

path = os.path.join(tempfile.gettempdir(), "quantized_model.keras")
model.save(path)
print("Quantized model saved to:", path)
```

## Configuration

- **Backend selection (required for multi-backend)**:
  - Set `KERAS_BACKEND` **before** importing `keras`. This is the critical requirement; setting the env var before any `keras` import is what determines the backend.
    - Valid values (per installation): `tensorflow`, `jax`, `torch`, `openvino` (inference-only).
  - Alternatively configure via `~/.keras/keras.json` **before** import.
- **Backend immutability**:
  - Backend cannot be changed reliably after `keras` is imported; restart the process/kernel to switch.
- **Installation convention**:
  - Install Keras 3 from PyPI as `keras`.
  - Keras 2 remains separately available as `tf-keras`.
  - Install at least one backend package alongside `keras`: `tensorflow`, `jax`, `torch` (and optionally `openvino` for inference-only). Keras cannot be imported without a supported backend installed.
- **GPU environments**:
  - Prefer separate environments per backend to avoid CUDA version mismatches; use backend-provided CUDA requirements files when applicable.
- **Flash attention**:
  - Control via `keras.config.enable_flash_attention()` / `keras.config.disable_flash_attention()`.

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

### Wrong: Custom layer without `get_config()` (serialization fails)
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import keras

class MyLayer(keras.Layer):
    def __init__(self, units):
        super().__init__()
        self.units = units

    def call(self, x):
        return x

# No get_config() — model.save() will fail or lose config
```

### Right: Implement `get_config()` and accept `**kwargs`
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import keras


@keras.saving.register_keras_serializable(package="MyPackage")
class MyLayer(keras.Layer):
    def __init__(self, units, **kwargs):
        super().__init__(**kwargs)
        self.units = units

    def call(self, x):
        return x

    def get_config(self):
        config = super().get_config()
        config.update({"units": self.units})
        return config
```

### Wrong: Using backend-specific ops in custom layers
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import tensorflow as tf
import keras

class MyLayer(keras.Layer):
    def call(self, x):
        return tf.nn.relu(x)  # TensorFlow-specific; breaks other backends
```

### Right: Use `keras.ops` for backend-agnostic operations
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import keras

class MyLayer(keras.Layer):
    def call(self, x):
        return keras.ops.nn.relu(x)  # Works on all backends
```

### Wrong: Seeding only Python/NumPy (backend RNG not seeded)
```python
import random
import numpy as np

random.seed(42)
np.random.seed(42)
# Does not seed the deep learning backend's RNG
```

### Right: Use `keras.utils.set_random_seed()` for full reproducibility
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import keras

keras.utils.set_random_seed(42)  # Seeds Python, NumPy, and the backend RNG
```

### Wrong: Using deprecated `keras.utils.Sequence` in Keras 3
```python
import keras

class MyDataset(keras.utils.Sequence):  # Deprecated in Keras 3
    ...
```

### Right: Use `keras.utils.PyDataset`
```python
import keras

class MyDataset(keras.utils.PyDataset):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)

    def __len__(self):
        return num_batches

    def __getitem__(self, idx):
        return x_batch, y_batch
```

### Wrong: Manual float16 casting instead of using mixed-precision policy
```python
import keras

class MyLayer(keras.Layer):
    def call(self, x):
        x = keras.ops.cast(x, "float16")  # Manual casting bypasses policy system
        return x
```

### Right: Use `keras.mixed_precision.set_global_policy()`
```python
import os
os.environ["KERAS_BACKEND"] = "tensorflow"
import keras

keras.mixed_precision.set_global_policy("mixed_float16")
# Keras automatically handles casting; keep output layers in float32
```

### Wrong: Creating weights in `__init__` (input shape unknown)
```python
import keras

class MyLayer(keras.Layer):
    def __init__(self, units, **kwargs):
        super().__init__(**kwargs)
        # Cannot create weight here — input shape is unknown
        self.kernel = self.add_weight(shape=(None, units))
```

### Right: Create weights in `build()`
```python
import keras

class MyLayer(keras.Layer):
    def __init__(self, units, **kwargs):
        super().__init__(**kwargs)
        self.units = units

    def build(self, input_shape):
        self.kernel = self.add_weight(
            shape=(input_shape[-1], self.units),
            name="kernel",
        )
        super().build(input_shape)

    def get_config(self):
        config = super().get_config()
        config.update({"units": self.units})
        return config
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
  - **Now**: use `model.save("path/model.keras")` for the native Keras 3 format.

- **`keras.utils.Sequence` deprecated** ⚠️
  - **Before**: subclass `keras.utils.Sequence` for data generators.
  - **Now**: subclass `keras.utils.PyDataset` instead.

- **Custom ops must use `keras.ops`**
  - **Before**: backend-specific ops (e.g., `tf.nn.relu`) in custom layers.
  - **Now**: use `keras.ops.nn.relu` and other `keras.ops` for backend portability.

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

## Migration

**Breaking changes from Keras 2.x to Keras 3.x:**

- Default model save format is now `.keras` instead of legacy HDF5 (`.h5`).
  ⚠️ Update `model.save()` calls to use the `.keras` extension and format.
- Configuring backend must be done **before** importing keras; backend cannot be changed after import.
  ⚠️ Move all backend configuration (`KERAS_BACKEND` env var, config file) before any `keras` import statements.
- `keras.utils.Sequence` is deprecated in Keras 3. ⚠️ Use `keras.utils.PyDataset` instead.

Keras 3 is intended as a drop-in replacement for `tf.keras` when using the TensorFlow backend. For custom components, refactor to backend-agnostic implementations using `keras.ops`. Model saving should use the new `.keras` format. Configure backend before importing keras.

## API Reference

> **Note:** The signatures below are based on Keras 3.13.2 source and documentation. They could not be verified by introspection in the reviewed environment because `tensorflow` is not installed and `import keras` fails with `ModuleNotFoundError: No module named 'tensorflow'`. Install a supported backend (e.g., `pip install tensorflow`) before using Keras.

- **keras** — Top-level package for Keras 3 (multi-backend); backend configured pre-import. Requires a supported backend (`tensorflow`, `jax`, `torch`, or `openvino`) to be installed before import.
- **keras.Model** — `Model(*args, **kwargs)`
  - Base class for models; exposes inference and saving APIs.
- **keras.Sequential** — `Sequential(layers=None, trainable=True, name=None)`
  - Linear stack model constructor.
- **keras.Layer** — `Layer(activity_regularizer=None, trainable=True, dtype=None, autocast=True, name=None)`
  - Base class for layers.
- **keras.Input** — `Input(shape=None, batch_size=None, dtype=None, sparse=False, batch_shape=None, name=None, tensor=None)`
  - Defines input shape for models; returns a `KerasTensor`.
- **keras.InputSpec** — `InputSpec(dtype=None, shape=None, ndim=None, max_ndim=None, min_ndim=None, axes=None, allow_last_axis_squeeze=False, name=None, optional=False)`
  - Used in layer input validation.
- **keras.Variable** — `Variable(initializer, shape=None, dtype=None, trainable=True, autocast=True, aggregation='mean', name=None)`
  - Backend variable abstraction.
- **keras.KerasTensor** — `KerasTensor(shape, dtype='float32', sparse=False, name=None, record_history=True)`
  - Symbolic tensor used internally and for model construction.
- **keras.Function** — `Function(inputs, outputs, name=None)`
- **keras.Operation** — `Operation(dtype=None, name=None)`
- **keras.Loss** — `Loss(name=None, reduction='sum_over_batch_size', dtype=None)`
- **keras.Metric** — `Metric(dtype=None, name=None)`
- **keras.Optimizer** — `Optimizer(learning_rate, weight_decay=None, clipnorm=None, clipvalue=None, global_clipnorm=None, use_ema=False, ema_momentum=0.99, ema_overwrite_frequency=None, loss_scale_factor=None, gradient_accumulation_steps=None, name='optimizer', **kwargs)`
- **keras.Initializer** — `Initializer()`
- **keras.Regularizer** — `Regularizer()`
- **keras.Quantizer** — `Quantizer(name=None)`
- **keras.DTypePolicy** — `DTypePolicy(name, source_name=None)`
- **keras.FloatDTypePolicy** — `FloatDTypePolicy(name, source_name=None)`
- **keras.StatelessScope** — `StatelessScope(state_mapping=None, collect_losses=False, initialize_variables=True)`
- **keras.SymbolicScope** — `SymbolicScope()`
- **keras.RematScope** — `RematScope()`
- **keras.remat(f)** — Rematerialization (gradient checkpointing) utility.
- **keras.device(device_name)** — Device context manager.
- **keras.name_scope(name, **kwargs)** — Name scope context manager.
- **keras.version()** — Returns Keras version string.
- **keras.Model.predict(x, verbose=...)** — Runs inference; key params: input `x`, verbosity.
- **keras.Model.save(filepath)** — Saves the model; prefer `*.keras` for native format.
- **keras.Model.compile(...)** — Configures training (backend-dependent); not supported for inference-only backends like OpenVINO.
- **keras.Model.fit(...)** — Training loop (when supported by backend).
- **keras.Model.evaluate(...)** — Evaluates the model on given data.
- **keras.Model.quantize(mode)** — Quantizes model weights post-training (e.g., `'int8'`).
- **keras.saving.load_model(filepath)** — Loads a saved model.
- **keras.saving.register_keras_serializable(package)** — Decorator to register custom classes for serialization; required for custom layers and models to be correctly saved and loaded.