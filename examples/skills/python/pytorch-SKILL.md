---

name: torch
description: A comprehensive Python library for tensor computation, automatic differentiation, and deep learning, offering GPU acceleration and a rich ecosystem of neural‑network utilities.
license: BSD-3-Clause
metadata:
  version: "latest"
  ecosystem: python
  generated-by: skilldo/gpt-oss-120b + review:gpt-oss-120b
---

## Imports

```python
import torch
import torch.nn as nn
from torch.utils.data import DataLoader, TensorDataset
```

## Core Patterns

### Tensor creation + basic ops ✅ Current
```python
import torch

def main() -> None:
    x = torch.randn(2, 3, dtype=torch.float32)
    y = torch.ones(2, 3, dtype=torch.float32)

    z = x + y
    w = torch.matmul(z, z.T)  # (2,3) @ (3,2) -> (2,2)

    print("x:", x)
    print("w shape:", w.shape)
    print("w:", w)

if __name__ == "__main__":
    main()
```
* Use `torch.tensor`, `torch.zeros/ones`, `torch.randn`, and ops like `+`, `*`, `torch.matmul` for eager tensor computation.

### Autograd: gradients with `backward()` ✅ Current
```python
import torch

def main() -> None:
    x = torch.randn(5, requires_grad=True)
    y = (x * x).sum()  # scalar
    y.backward()

    assert x.grad is not None
    print("x:", x)
    print("grad:", x.grad)

if __name__ == "__main__":
    main()
```
* Set `requires_grad=True` on leaf tensors; call `.backward()` on a scalar loss to populate `.grad`.

### Training loop with `torch.nn.Module` + optimizer ✅ Current
```python
import torch
import torch.nn as nn
from torch.utils.data import DataLoader, TensorDataset

def main() -> None:
    torch.manual_seed(0)

    # Synthetic regression: y = 2x + 1 + noise
    x = torch.randn(256, 1)
    y = 2.0 * x + 1.0 + 0.1 * torch.randn_like(x)

    loader = DataLoader(TensorDataset(x, y), batch_size=32, shuffle=True)

    model = nn.Sequential(nn.Linear(1, 16), nn.ReLU(), nn.Linear(16, 1))
    optimizer = torch.optim.SGD(model.parameters(), lr=0.1)
    loss_fn = nn.MSELoss()

    model.train()
    for _epoch in range(3):
        for xb, yb in loader:
            pred = model(xb)
            loss = loss_fn(pred, yb)

            optimizer.zero_grad(set_to_none=True)
            loss.backward()
            optimizer.step()

    model.eval()
    with torch.no_grad():
        test_x = torch.tensor([[0.0], [1.0]])
        print("pred:", model(test_x).squeeze(-1))

if __name__ == "__main__":
    main()
```
* Standard pattern: `model.train()`, forward pass, compute loss, `optimizer.zero_grad()`, `loss.backward()`, `optimizer.step()`.

### Save/load model and optimizer state ✅ Current
```python
import torch
import torch.nn as nn

def main() -> None:
    model = nn.Linear(4, 2)
    optimizer = torch.optim.AdamW(model.parameters(), lr=1e-3)

    # "Train" one step
    x = torch.randn(8, 4)
    y = model(x).sum()
    optimizer.zero_grad(set_to_none=True)
    y.backward()
    optimizer.step()

    checkpoint_path = "/tmp/torch_checkpoint.pt"
    torch.save(
        {"model": model.state_dict(), "optimizer": optimizer.state_dict()},
        checkpoint_path,
    )

    # Restore into fresh instances
    model2 = nn.Linear(4, 2)
    optimizer2 = torch.optim.AdamW(model2.parameters(), lr=1e-3)
    # NOTE: Do NOT use `weights_only=True` here because optimizer state dict contains
    # non‑tensor entries (e.g., step counters). Loading with `weights_only=True` would
    # drop those entries and corrupt the optimizer state.
    ckpt = torch.load(checkpoint_path, map_location="cpu")
    model2.load_state_dict(ckpt["model"])
    optimizer2.load_state_dict(ckpt["optimizer"])

    print("restored ok:", True)

if __name__ == "__main__":
    main()
```
* Use `state_dict()` + `torch.save` for portability; `torch.load(..., map_location=..., weights_only=True)` is useful for loading *only* tensor weights, but when restoring optimizer state you must load the full checkpoint (or set `weights_only=False`) so that integer counters and other metadata are preserved.

### TorchScript export with `torch.jit.trace` ✅ Current
```python
import torch
import torch.nn as nn

def main() -> None:
    model = nn.Sequential(nn.Linear(3, 4), nn.ReLU(), nn.Linear(4, 2))
    model.eval()

    example = torch.randn(1, 3)
    scripted = torch.jit.trace(model, example)

    out_eager = model(example)
    out_script = scripted(example)
    print("max diff:", (out_eager - out_script).abs().max().item())

if __name__ == "__main__":
    main()
```
* `torch.jit.trace` captures tensor ops executed for given examples; prefer `model.eval()` for stable tracing.

### Compile a model with `torch.compile` (PyTorch 2.x) ✅ Current
```python
import torch
import torch.nn as nn

def main() -> None:
    torch.manual_seed(0)
    model = nn.Sequential(nn.Linear(4, 16), nn.ReLU(), nn.Linear(16, 2))

    # Compile for optimized execution (default eager mode)
    compiled_model = torch.compile(model, backend="inductor", mode="default")

    x = torch.randn(8, 4)
    out = compiled_model(x)
    print("output shape:", out.shape)

if __name__ == "__main__":
    main()
```
* `torch.compile()` optimizes models via TorchDynamo + backends like Inductor for faster execution.
* Use `mode="reduce-overhead"` for minimal overhead or `mode="max-autotune"` for aggressive tuning.
* Pass `fullgraph=True` to require a single graph (raises an error if graph breaks occur).
* Pass `dynamic=True` to enable dynamic shape support for variable‑length inputs.

### Automatic mixed precision with `torch.autocast` ✅ Current
```python
import torch
import torch.nn as nn

def main() -> None:
    torch.manual_seed(0)
    model = nn.Linear(10, 5).cuda()
    optimizer = torch.optim.SGD(model.parameters(), lr=0.01)
    scaler = torch.cuda.amp.GradScaler()  # correct GradScaler location and usage

    x = torch.randn(16, 10, device="cuda")
    y = torch.randn(16, 5, device="cuda")

    for _ in range(3):
        optimizer.zero_grad(set_to_none=True)

        with torch.autocast(device_type="cuda", dtype=torch.float16):
            pred = model(x)
            loss = ((pred - y) ** 2).mean()

        scaler.scale(loss).backward()
        scaler.step(optimizer)
        scaler.update()

    print("training complete")

if __name__ == "__main__":
    main()
```
* Use `torch.autocast` context manager for automatic mixed precision; pair with `torch.cuda.amp.GradScaler` to handle gradient scaling.
* Supports `device_type="cuda"` or `device_type="cpu"` with appropriate dtypes.

### Vectorized operations with `torch.vmap` ✅ Current
```python
import torch

def main() -> None:
    def compute(x: torch.Tensor, y: torch.Tensor) -> torch.Tensor:
        return (x * y).sum()

    # Batch of inputs
    x_batch = torch.randn(32, 10)
    y_batch = torch.randn(32, 10)

    # Vectorize over first dimension
    batched_compute = torch.vmap(compute)
    result = batched_compute(x_batch, y_batch)

    print("result shape:", result.shape)  # (32,)

if __name__ == "__main__":
    main()
```
* `torch.vmap` automatically vectorizes a function over batch dimensions, avoiding manual loops.
* Alias for `torch.func.vmap`; both are available.

### Export a model with `torch.export` ✅ Current
```python
import torch
import torch.nn as nn

def main() -> None:
    torch.manual_seed(0)
    model = nn.Sequential(nn.Linear(4, 8), nn.ReLU(), nn.Linear(8, 2))
    model.eval()

    # Export with static shapes
    args = (torch.randn(2, 4),)
    exported = torch.export.export(model, args)

    # Run the exported program
    result = exported.module()(*args)
    print("exported output shape:", result.shape)

if __name__ == "__main__":
    main()
```
* `torch.export.export` produces a fully‑captured, serializable `ExportedProgram` for deployment.
* Use `dynamic_shapes` to specify dynamic batch or sequence dimensions.

### Functional transforms with `torch.func.vmap` (new) ✅ Current
```python
import torch
import torch.nn.functional as F

def main() -> None:
    def relu_batch(x):
        return F.relu(x)

    # Original batch tensor
    batch = torch.randn(8, 5)

    # Apply vmap via torch.func namespace
    batched_relu = torch.func.vmap(relu_batch)
    out = batched_relu(batch)

    print("out shape:", out.shape)  # (8, 5)

if __name__ == "__main__":
    main()
```
* The new `torch.func` namespace provides functional transforms; `torch.func.vmap` is equivalent to the top‑level `torch.vmap`.

### Functional gradient with `torch.func.grad` (new) ✅ Current
```python
import torch
import torch.nn as nn
import torch.func as ft

def main() -> None:
    # Simple scalar function
    def loss_fn(w: torch.Tensor, x: torch.Tensor, y: torch.Tensor) -> torch.Tensor:
        pred = x @ w
        return ((pred - y) ** 2).mean()  # scalar loss

    x = torch.randn(10, 5)
    y = torch.randn(10, 1)
    w = torch.randn(5, 1, requires_grad=False)

    # grad returns a callable that computes gradient w.r.t. the first argument (w)
    grad_w = ft.grad(loss_fn)

    grad = grad_w(w, x, y)
    print("grad shape:", grad.shape)  # (5,1)

if __name__ == "__main__":
    main()
```
* `torch.func.grad` (or `torch.grad` alias) creates a gradient‑computing function without building an autograd graph each call.
* Useful for meta‑learning, hyper‑parameter optimization, or custom training loops.

## Configuration

* **Device selection**
  * CPU by default. Use `device = torch.device("cuda")` if `torch.cuda.is_available()`.
  * Move tensors/modules with `.to(device)`.

* **Reproducibility**
  * Set seeds with `torch.manual_seed(seed: int)` – returns a `torch._C.Generator` that can be stored if needed.
  * For CUDA determinism you may need additional settings depending on ops and backend.
  * Use `torch.use_deterministic_algorithms(True)` to enforce deterministic operations where available.
  * Many GPU kernels can be nondeterministic; expect occasional differences unless you constrain algorithms.

* **Multiprocessing / DataLoader**
  * `DataLoader(..., num_workers>0)` uses subprocesses; in containers increase shared memory (`--ipc=host` or `--shm-size`).

* **Default device and dtype**
  * Set default device: `torch.set_default_device("cuda")` or `torch.set_default_device("cpu")`.
  * Set default dtype: `torch.set_default_dtype(torch.float32)`.
  * ⚠️ `torch.set_default_tensor_type(...)` is **deprecated** since 2.1; migrate to `set_default_dtype` + `set_default_device`.

* **Float32 matmul precision**
  * Control TF32 / BF16 usage: `torch.set_float32_matmul_precision('highest' | 'high' | 'medium')`.
  * `'high'` or `'medium'` can significantly speed up matmul on Ampere+ GPUs via TF32.

* **Source build environment variables (common)**
  * Disable backends: `USE_CUDA=0`, `USE_ROCM=0`, `USE_XPU=0`
  * ROCm non‑default location: set `ROCM_PATH=/path/to/rocm`
  * Choose CUDA toolkit by `PATH` ordering for `nvcc` (e.g. `/usr/local/cuda-12.8/bin`)

## Migration

### Breaking changes since 1.13.0 → 2.0.0
* **`torch.compile` introduced** – the default JIT compilation path changed. Functions previously marked with `@torch.jit.script` may now be compiled twice if also passed through `torch.compile`.
  * **Migration:** Do not wrap already‑scripted functions with `torch.compile`. Either use the new `torch.compile` API for the whole training loop **or** keep the old `@torch.jit.script` annotations, but avoid mixing both on the same callable.

### Deprecations
* `torch.set_default_tensor_type` – soft‑deprecated since 2.1. Use `torch.set_default_dtype` and `torch.set_default_device` instead.

### Upgrade checklist
1. Replace any `torch.set_default_tensor_type` calls with the new pair of defaults.
2. Review code that uses both `@torch.jit.script` and `torch.compile`; remove one of the two mechanisms.
3. Ensure environment variables for accelerator back‑ends are `USE_CUDA`, `USE_ROCM`, `USE_XPU`.
4. Verify Docker containers allocate sufficient shared memory (`--ipc=host` or `--shm-size`).
5. Update Python requirement to ≥ 3.10 if still on an older interpreter.

## Pitfalls

### Wrong: assuming DataLoader multiprocessing "just works" in Docker (shared memory too small)
```python
import torch
from torch.utils.data import DataLoader, TensorDataset

def main() -> None:
    ds = TensorDataset(torch.randn(10_000, 64), torch.randn(10_000, 1))
    # In many Docker setups, this can crash/hang due to small /dev/shm
    loader = DataLoader(ds, batch_size=128, num_workers=4, persistent_workers=True)

    for xb, yb in loader:
        _ = xb.mean() + yb.mean()
        break

if __name__ == "__main__":
    main()
```

### Right: increase container shared memory (or reduce workers)
```python
# Run container with more shared memory:
# docker run --rm -it --ipc=host pytorch/pytorch:latest
# or:
# docker run --rm -it --shm-size=8g pytorch/pytorch:latest

import torch
from torch.utils.data import DataLoader, TensorDataset

def main() -> None:
    ds = TensorDataset(torch.randn(10_000, 64), torch.randn(10_000, 1))
    loader = DataLoader(ds, batch_size=128, num_workers=2)

    for xb, yb in loader:
        _ = xb.mean() + yb.mean()
        break

if __name__ == "__main__":
    main()
```

### Wrong: forgetting `optimizer.zero_grad()` (gradients accumulate)
```python
import torch
import torch.nn as nn

def main() -> None:
    model = nn.Linear(3, 1)
    opt = torch.optim.SGD(model.parameters(), lr=0.1)

    x = torch.randn(4, 3)
    for _ in range(2):
        loss = model(x).sum()
        loss.backward()  # grads accumulate across iterations
        opt.step()

    print("done")

if __name__ == "__main__":
    main()
```

### Right: clear gradients each step (optionally `set_to_none=True`)
```python
import torch
import torch.nn as nn

def main() -> None:
    model = nn.Linear(3, 1)
    opt = torch.optim.SGD(model.parameters(), lr=0.1)

    x = torch.randn(4, 3)
    for _ in range(2):
        opt.zero_grad(set_to_none=True)
        loss = model(x).sum()
        loss.backward()
        opt.step()

    print("done")

if __name__ == "__main__":
    main()
```

### Wrong: calling `.numpy()` on a tensor that requires grad
```python
import torch

def main() -> None:
    x = torch.randn(3, requires_grad=True)
    y = (x * 2).sum()
    y.backward()

    # Raises an error because x requires grad
    arr = x.numpy()
    print(arr)

if __name__ == "__main__":
    main()
```

### Right: detach (and move to CPU if needed) before converting to NumPy
```python
import torch

def main() -> None:
    x = torch.randn(3, requires_grad=True)
    y = (x * 2).sum()
    y.backward()

    arr = x.detach().cpu().numpy()
    print(arr)

if __name__ == "__main__":
    main()
```

### Wrong: saving the whole module object instead of `state_dict()`
```python
import torch
import torch.nn as nn

def main() -> None:
    model = nn.Linear(2, 2)
    # Pickle‑based; can be brittle across code changes
    torch.save(model, "/tmp/model_entire.pt")

if __name__ == "__main__":
    main()
```

### Right: save/load `state_dict()` for forward‑compatible checkpoints
```python
import torch
import torch.nn as nn

def main() -> None:
    path = "/tmp/model_state.pt"
    model = nn.Linear(2, 2)
    torch.save(model.state_dict(), path)

    model2 = nn.Linear(2, 2)
    state = torch.load(path, map_location="cpu", weights_only=True)
    model2.load_state_dict(state)

if __name__ == "__main__":
    main()
```

### Wrong: loading untrusted checkpoints without `weights_only=True`
```python
import torch

def main() -> None:
    # Insecure: allows arbitrary code execution from malicious pickles
    checkpoint = torch.load("untrusted_model.pt", weights_only=False)
    print(checkpoint)

if __name__ == "__main__":
    main()
```

### Right: use `weights_only=True` when loading checkpoints
```python
import torch

def main() -> None:
    # Secure: only loads tensor data, not arbitrary Python objects
    # `weights_only=True` is now the default in recent PyTorch versions
    checkpoint = torch.load("model.pt", weights_only=True)
    print(checkpoint)

if __name__ == "__main__":
    main()
```

## References

- [Homepage](https://pytorch.org)
- [Repository](https://github.com/pytorch/pytorch)
- [Documentation](https://pytorch.org/docs)
- [Issue Tracker](https://github.com/pytorch/pytorch/issues)
- [Forum](https://discuss.pytorch.org)

## API Reference

- **torch.tensor(data, dtype=None, device=None, requires_grad=False)** – Create a tensor from Python data.
- **torch.randn(*sizes, dtype=None, device=None, requires_grad=False)** – Random normal tensor.
- **torch.rand(*sizes, dtype=None, device=None, requires_grad=False)** – Random uniform `[0, 1)` tensor.
- **torch.zeros(*sizes, dtype=None, device=None, requires_grad=False)** – All‑zeros tensor.
- **torch.ones(*sizes, dtype=None, device=None, requires_grad=False)** – All‑ones tensor.
- **torch.matmul(input, other)** – Matrix product (supports broadcasting).
- **torch.stack(tensors, dim=0)** – Concatenate tensors along a new dimension.
- **torch.split(tensor, split_size_or_sections, dim=0)** – Split tensor into chunks.
- **torch.chunk(input, chunks, dim=0)** – Split tensor into a specific number of chunks.
- **torch.unravel_index(indices, shape)** – Convert flat indices to coordinate tuples.
- **torch.manual_seed(seed) -> torch._C.Generator** – Set seeds with a random seed; returns a `Generator` that can be stored if needed.
- **torch.seed()** – Set random seed to a random number; returns that seed.
- **torch.initial_seed()** – Returns the initial seed for generating random numbers.
- **torch.get_rng_state(device="cpu")** – Get RNG state as a tensor.
- **torch.set_rng_state(new_state, device="cpu")** – Set RNG state from a tensor.
- **torch.typename(obj)** – Return the type name of a tensor or module as a string.
- **torch.is_tensor(obj)** – Returns `True` if `obj` is a PyTorch tensor.
- **torch.is_storage(obj)** – Returns `True` if `obj` is a storage object.
- **Tensor.backward(gradient=None)** – Compute gradients for the autograd graph.
- **torch.no_grad()** – Context manager disabling gradient tracking.
- **torch.enable_grad()** – Context manager enabling gradient tracking (default).
- **torch.inference_mode(mode=True)** – Context manager for inference with reduced overhead.
- **torch.autocast(device_type, dtype=None, enabled=True, cache_enabled=None)** – Automatic mixed‑precision context.
- **torch.compile(func, backend=None, mode="default", fullgraph=False, dynamic=False, options=None, disable=False)** – Compile a model for optimized execution (PyTorch 2.x).
- **torch.vmap(*args, **kwargs)** – Vectorizing map for automatic batching. Alias for `torch.func.vmap`.
- **torch.cond(pred, true_fn, false_fn, operands)** – Conditional control‑flow operator.
- **torch.export.export(mod, args, kwargs=None, *, dynamic_shapes=None, strict=False, preserve_module_call_signature=())** – Export models to a serializable `ExportedProgram`. `dynamic_shapes` replaces the older `constraints` parameter.
- **torch.save(obj, f, pickle_protocol=2, _use_new_zipfile_serialization=True)** – Serialize an object (commonly a `state_dict()`).
- **torch.load(f, map_location=None, *, weights_only=True, **pickle_load_args)** – Deserialize an object; `weights_only=True` is now the default.
- **torch.use_deterministic_algorithms(mode, *, warn_only=False)** – Enable/disable deterministic algorithms.
- **torch.are_deterministic_algorithms_enabled()** – Check if deterministic algorithms are enabled.
- **torch.is_deterministic_algorithms_warn_only_enabled()** – Check warn‑only mode for deterministic algorithms.
- **torch.set_deterministic_debug_mode(debug_mode)** – Set deterministic debug mode.
- **torch.get_deterministic_debug_mode()** – Get current deterministic debug mode.
- **torch.set_default_device(device)** – Set default device for tensor creation.
- **torch.get_default_device()** – Get default device.
- **torch.set_default_dtype(d)** – Set default floating‑point dtype.
- **torch.set_default_tensor_type(t) ⚠️** – Deprecated since 2.1; use `set_default_dtype` + `set_default_device`.
- **torch.set_float32_matmul_precision(precision)** – Set precision for float32 matmul (`'highest' | 'high' | 'medium'`).
- **torch.get_float32_matmul_precision()** – Get current float32 matmul precision.
- **torch.set_warn_always(b)** – Enable/disable always‑warn mode for warnings.
- **torch.is_warn_always_enabled()** – Check if always‑warn mode is enabled.
- **torch.set_printoptions(...)** – Set print options for tensors.
- **torch.cuda.amp.GradScaler(init_scale=65536.0, growth_factor=2.0, backoff_factor=0.5, growth_interval=2000, enabled=True)** – Gradient scaler for automatic mixed precision.
- **torch.nn.Module** – Base class for all neural‑network modules.
- **torch.nn.Linear(in_features, out_features, bias=True)** – Fully‑connected layer.
- **torch.nn.Sequential(*args)** – Sequential container of modules.
- **torch.nn.functional.relu(input)** – Rectified linear unit activation.
- **torch.nn.MSELoss()** – Mean squared error loss.
- **torch.optim.SGD(params, lr, momentum=0, weight_decay=0)** – SGD optimizer.
- **torch.optim.AdamW(params, lr, betas=(0.9, 0.999), weight_decay=0.01)** – AdamW optimizer.
- **torch.utils.data.DataLoader(dataset, batch_size=1, shuffle=False, num_workers=0)** – Batching + multiprocessing input pipeline.
- **torch.utils.data.TensorDataset(*tensors)** – Dataset wrapping tensors.
- **torch.jit.trace(func, example_inputs)** – Trace a module/function to TorchScript.
- **torch.UntypedStorage(*args, device=None)** – Raw untyped storage backing tensors.
- **torch.TypedStorage(*args, **kwargs) ⚠️** – Deprecated since 2.0; use `torch.UntypedStorage` or `tensor.untyped_storage()`.
- **torch.SymInt(a)** – Symbolic integer for shape reasoning.
- **torch.SymFloat(a)** – Symbolic float for shape reasoning.
- **torch.SymBool(a)** – Symbolic boolean for guards.
- **torch.sym_sum(args)** – Symbolic sum over a sequence.
- **torch.sym_max(a, b)** – Symbolic maximum.
- **torch.sym_min(a, b)** – Symbolic minimum.
- **torch.sym_not(a)** – Symbolic boolean negation.
- **torch.sym_ite(b, t, f)** – Symbolic if‑then‑else.
- **torch.sym_sqrt(a)** – Symbolic square root.
- **torch.sym_cos(a)** – Symbolic cosine.
- **torch.sym_sin(a)** – Symbolic sine.
- **torch.sym_tan(a)** – Symbolic tangent.
- **torch.sym_log2(a)** – Symbolic log base 2.
- **torch.sym_exp(a)** – Symbolic exponential.
- **torch.functorch** – functional transforms and utilities:
  - `grad(func, ...)` – Functional gradient.
  - `jacrev(func, ...)` – Jacobian via reverse‑mode AD.
  - `jacfwd(func, ...)` – Jacobian via forward‑mode AD.
  - `vmap(func, ...)` – Functional vectorized map.
  - `jvp(func, ...)` – Jacobian‑vector product.
  - `vjp(func, ...)` – Vector‑Jacobian product.
  - `make_functional(module)` – Convert `nn.Module` to functional form.
  - `make_functional_with_buffers(module)` – Same as above, preserving buffers.
  - `FunctionalModule(module)` – Functional wrapper class.
  - `FunctionalModuleWithBuffers(module)` – Functional wrapper preserving buffers.
  - `make_fx(func, ...)` – Produce a functional program representation.