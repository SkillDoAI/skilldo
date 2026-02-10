---
name: keras
description: Keras is a high-level neural networks API for building and training deep learning models.
version: unknown
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
from keras import Model, layers, losses, metrics, optimizers
```

## Core Patterns

The right way to use the main APIs. Show 3-5 most common patterns.

### Model Definition ✅ Current
```python
# Define a custom model by extending the Model class
from keras import Model, layers

class MyModel(Model):
    def __init__(self, hidden_dim: int, output_dim: int):
        super(MyModel, self).__init__()
        self.dense1 = layers.Dense(hidden_dim, activation='relu')
        self.dense2 = layers.Dense(output_dim)

    def call(self, inputs):
        x = self.dense1(inputs)
        return self.dense2(x)

# Instantiate the model
model = MyModel(hidden_dim=256, output_dim=16)
```
* This code defines a custom model with two dense layers.
* **Status**: Current, stable

### Model Compilation ✅ Current
```python
# Compile the model with optimizer, loss, and metrics
from keras import optimizers, losses, metrics

model.compile(optimizer=optimizers.SGD(learning_rate=0.001),
              loss=losses.MeanSquaredError(),
              metrics=[metrics.MeanSquaredError()])
```
* This code compiles the model with specified optimizer, loss function, and metrics.
* **Status**: Current, stable

### Model Fitting ✅ Current
```python
# Fit the model to training data
import numpy as np

x = np.random.random((50000, 128))  # Example input data
y = np.random.random((50000, 16))    # Example output data
batch_size = 32
epochs = 10

history = model.fit(x, y, batch_size=batch_size, epochs=epochs, validation_split=0.2)
```
* This code trains the model using the provided data.
* **Status**: Current, stable

### Input Layer ✅ Current
```python
# Create an input layer
from keras.layers import Input

input_layer = Input(shape=(128,))
```
* This code creates an input layer with a specified shape.
* **Status**: Current, stable

### Load Model ✅ Current
```python
# Load a previously saved model
from keras.models import load_model

loaded_model = load_model('my_model.keras')
```
* This code loads a Keras model from a file.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Default values for optimizers, loss functions, etc. can be specified during instantiation.
- Common customizations include adjusting learning rates, activation functions, and layer configurations.
- Environment variables can be set for configuring backend options before importing Keras.

## Pitfalls

### Wrong: Setting backend after importing keras
```python
import keras  # Incorrect order
# Backend configuration code here
```

### Right: Set backend before import
```python
import os
os.environ['Keras_Backend'] = 'tensorflow'  # Correctly set before import
import keras
```

### Wrong: Using tf.keras model without conversion
```python
model = tf.keras.Model()  # Will cause issues if not updated
```

### Right: Ensure model.save() uses the .keras format
```python
model.save('my_model.keras')  # Use the correct format
```

### Wrong: Forgetting to compile the model before fitting
```python
# model.fit(x, y)  # This will raise an error
```

### Right: Always compile the model first
```python
model.compile(optimizer='adam', loss='mean_squared_error')
model.fit(x, y)  # Now this works
```

## References

- [Home](https://keras.io/)
- [Repository](https://github.com/keras-team/keras)

## Migration from v[previous]

What changed in this version (if applicable):
- **Breaking changes**: Changed model saving format to .keras. Update model.save() calls to use .keras format.
- **Deprecated → Current mapping**: No deprecated APIs noted.
- **Before/after code examples**: Not applicable.

## API Reference

Brief reference of the most important public APIs:

- **Model()** - Base class for defining custom models.
- **SGD(learning_rate: float)** - Stochastic Gradient Descent optimizer.
- **Dense(units: int)** - Fully connected layer.
- **Input(shape)** - Input layer for the model.
- **MeanSquaredError()** - Loss function for regression tasks.
- **load_model(filepath)** - Load a model from a file.
- **fit(x, y)** - Train the model on the data.
- **compile(optimizer, loss, metrics)** - Configure the model for training.