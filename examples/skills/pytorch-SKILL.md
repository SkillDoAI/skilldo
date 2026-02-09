---
name: torch
description: A library for tensor computation and deep learning.
version: unknown
ecosystem: python
license: BSD-3-Clause
---

## Imports

Show the standard import patterns. Most common first:
```python
import torch
from torch import Tensor
```

## Core Patterns

The right way to use the main APIs. Show 3-5 most common patterns.

### Create Tensor ✅ Current
```python
import torch
from torch import Tensor

def main():
    # Create a tensor from a list of data
    tensor = Tensor([1.0, 2.0, 3.0])

    print(tensor)

if __name__ == "__main__":
    main()
```
* Creates a tensor from a list of data.
* **Status**: Current, stable

### Generate Random Tensor ✅ Current
```python
import torch

def main():
    # Generate a random tensor of shape (2, 3)
    random_tensor = torch.randn((2, 3))

    print(random_tensor)

if __name__ == "__main__":
    main()
```
* Generates a tensor with random values from a standard normal distribution.
* **Status**: Current, stable

### Save Tensor ✅ Current
```python
import torch

def main():
    tensor = torch.Tensor([1.0, 2.0, 3.0])
    # Save the tensor to a file
    torch.save(tensor, 'tensor.pt')

if __name__ == "__main__":
    main()
```
* Saves a tensor to a specified file.
* **Status**: Current, stable

### Load Tensor ✅ Current
```python
import torch

def main():
    # Load tensor from file
    loaded_tensor = torch.load('tensor.pt')
    print(loaded_tensor)

if __name__ == "__main__":
    main()
```
* Loads a tensor from a specified file.
* **Status**: Current, stable

### Set Seed for Randomness ✅ Current
```python
import torch

def main():
    # Set the seed for reproducibility
    torch.manual_seed(42)
    random_tensor1 = torch.randn(3)
    random_tensor2 = torch.randn(3)

    print(random_tensor1)
    print(random_tensor2)

if __name__ == "__main__":
    main()
```
* Sets the manual seed for generating random numbers.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Use `torch.manual_seed(seed: int)` to set a random seed for reproducibility.
- Tensors can be saved to and loaded from files using `torch.save()` and `torch.load()` respectively.

## Pitfalls

### Wrong: Mutable Default Arguments
```python
def create_tensor(data=[], dtype=None):
    return Tensor(data, dtype=dtype)
```

### Right: Correct Approach
```python
def create_tensor(data=None, dtype=None):
    if data is None:
        data = []
    return Tensor(data, dtype=dtype)
```
* Mutable default arguments retain changes across function calls, leading to unexpected behavior.

### Wrong: Not Setting the Seed
```python
import torch

def generate_random_tensors():
    return torch.randn((2, 3))

tensor1 = generate_random_tensors()
tensor2 = generate_random_tensors()
```

### Right: Setting the Seed
```python
import torch

def generate_random_tensors():
    torch.manual_seed(42)
    return torch.randn((2, 3))

tensor1 = generate_random_tensors()
tensor2 = generate_random_tensors()
```
* Not setting a seed can lead to non-reproducible results.

### Wrong: Using Deprecated Features
```python
tensor = torch.Tensor([1, 2, 3], dtype=torch.float32)
```

### Right: Using Current Features
```python
tensor = torch.tensor([1, 2, 3], dtype=torch.float32)
```
* Always use the current recommended API.

## References

- [Homepage](https://pytorch.org)
- [Repository](https://github.com/pytorch/pytorch)
- [Documentation](https://pytorch.org/docs)
- [Issue Tracker](https://github.com/pytorch/pytorch/issues)
- [Forum](https://discuss.pytorch.org)

## Migration from v[previous]

What changed in this version (if applicable):
- Breaking changes: Ensure to migrate to the new tensor creation API where applicable.
- Deprecated → Current mapping:
    - `torch.Tensor()` should use `torch.tensor()` instead for creating tensors.

## API Reference

Brief reference of the most important public APIs:

- **Tensor(data: Any)** - Creates a tensor from a given data.
- **torch.randn(size: Union[int, Tuple[int, ...]])** - Generates a tensor with random values.
- **torch.save(obj: Any, f: Union[str, os.PathLike, io.IOBase])** - Saves an object to a file.
- **torch.load(f: Union[str, os.PathLike, io.IOBase])** - Loads an object from a file.
- **torch.manual_seed(seed: int) -> None** - Sets the seed for generating random numbers.
- **torch.is_tensor(obj: Any) -> bool** - Checks if the object is a tensor.