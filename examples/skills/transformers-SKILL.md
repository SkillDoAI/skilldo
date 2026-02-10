---

name: transformers
description: State-of-the-art NLP, vision, audio, and multimodal model inference/training utilities including pipelines, pretrained models, and tokenizers.
version: unknown
ecosystem: python
license: Apache 2.0 License
generated_with: gpt-5.2
---

## Imports

```python
import transformers
from transformers import AutoTokenizer, pipeline
from transformers.utils.metrics import attach_tracer, traced
```

## Core Patterns

### Create inference pipelines (text + image) ✅ Current
```python
from transformers import pipeline

def main() -> None:
    text_gen = pipeline(task="text-generation", model="Qwen/Qwen2.5-1.5B")
    out = text_gen("The secret to baking a really good cake is ", max_new_tokens=40)
    print(out[0]["generated_text"])

    img_cls = pipeline(task="image-classification", model="facebook/dinov2-small-imagenet1k-1-layer")
    preds = img_cls("https://huggingface.co/datasets/Narsil/image_dummy/raw/main/parrots.png")
    print(preds[:2])

if __name__ == "__main__":
    main()
```
* Prefer `transformers.pipeline(task=..., model=...)` for quick inference; it handles preprocessing/postprocessing and downloads/caches weights.

### Chat-style prompting with `text-generation` pipeline ✅ Current
```python
import torch
from transformers import pipeline

def main() -> None:
    chat = [
        {"role": "system", "content": "You are a concise assistant."},
        {"role": "user", "content": "Give me 3 ideas for a weekend trip from Paris."},
    ]

    pipe = pipeline(
        task="text-generation",
        model="meta-llama/Meta-Llama-3-8B-Instruct",
        dtype=torch.bfloat16,
        device_map="auto",
    )

    response = pipe(chat, max_new_tokens=200)
    # Many chat-capable pipelines return a list of messages in generated_text
    print(response[0]["generated_text"][-1]["content"])

if __name__ == "__main__":
    main()
```
* For chat/instruct models, pass a list of `{role, content}` messages (not just a single string).
* Use `dtype=` and `device_map="auto"` to control memory/placement for larger models.

### Load, modify, and save a tokenizer ✅ Current
```python
import tempfile
import shutil
from transformers import AutoTokenizer

def main() -> None:
    tmp_dir = tempfile.mkdtemp()
    try:
        tokenizer = AutoTokenizer.from_pretrained("bert-base-uncased")

        # Add a special token and ensure it tokenizes as a single token
        special = "[SPECIAL_TOKEN_1]"
        tokenizer.add_tokens([special], special_tokens=True)
        assert tokenizer.tokenize(special) == [special]

        # Add extra special tokens without replacing existing ones
        extra = "[SPECIAL_TOKEN_2]"
        tokenizer.add_special_tokens({"extra_special_tokens": [extra]}, replace_extra_special_tokens=False)
        assert tokenizer.tokenize(extra) == [extra]

        # Save and reload round-trip
        tokenizer.save_pretrained(tmp_dir)
        reloaded = tokenizer.__class__.from_pretrained(tmp_dir)

        text = "He is very happy, UNwanté,dunning"
        assert tokenizer.encode(text, add_special_tokens=False) == reloaded.encode(text, add_special_tokens=False)
        assert tokenizer.get_vocab() == reloaded.get_vocab()

        # Common conventions
        assert reloaded.model_input_names[0] in ["input_ids", "input_values"]

        print("Tokenizer round-trip OK:", tmp_dir)
    finally:
        shutil.rmtree(tmp_dir)

if __name__ == "__main__":
    main()
```
* `AutoTokenizer.from_pretrained()` loads a tokenizer from a model id or local directory.
* `save_pretrained()` + `from_pretrained()` should preserve vocab and tokenization behavior.

### Batch tokenize and decode with `__call__` and `BatchEncoding` ✅ Current
```python
from transformers import AutoTokenizer

def main() -> None:
    tokenizer = AutoTokenizer.from_pretrained("bert-base-uncased")

    sequences = ["Hello world!", "Transformers tokenizers batch encode."]
    encoding = tokenizer(sequences, padding=True)

    # BatchEncoding behaves like a dict; `.data` exposes the underlying mapping
    data = encoding.data
    input_ids = data["input_ids"]

    decoded = [tokenizer.decode(ids, skip_special_tokens=True) for ids in input_ids]
    for src, dst in zip(sequences, decoded):
        print("SRC:", src)
        print("DEC:", dst)
        print("---")

if __name__ == "__main__":
    main()
```
* Use `tokenizer(texts, padding=True, truncation=True, return_tensors=...)` for batch preprocessing.
* Decode with `skip_special_tokens=True` for human-readable text.

### Instrument methods/functions with `attach_tracer` and `traced` ✅ Current
```python
from __future__ import annotations

from transformers.utils.metrics import attach_tracer, traced

@attach_tracer()
class ExampleClass:
    def __init__(self, name: str) -> None:
        self.name = name

    @traced
    def process_data(self, data: str) -> str:
        return f"Processed {data} with {self.name}"

    @traced(span_name="custom_operation")
    def special_operation(self, value: int) -> int:
        return value * 2

    @traced(
        additional_attributes=[
            ("name", "object.name", lambda x: x.upper()),
            ("name", "object.fixed_value", "static_value"),
        ]
    )
    def operation_with_attributes(self) -> str:
        return "Operation completed"

@traced
def standalone_function(arg1: int, arg2: int) -> int:
    return arg1 + arg2

def main() -> None:
    ex = ExampleClass("test_object")
    print(ex.process_data("sample"))
    print(ex.special_operation(42))
    print(ex.operation_with_attributes())
    print(standalone_function(1, 2))

if __name__ == "__main__":
    main()
```
* `@attach_tracer()` is a class decorator that ensures instances have `self.tracer`.
* `@traced` works on methods and standalone functions; supports `span_name=` and `additional_attributes=`.

## Configuration

- Installation (match backend/framework):
  - `pip install "transformers[torch]"` (PyTorch required per README: Python 3.9+, PyTorch 2.4+).
- Model caching:
  - Models/tokenizers downloaded by `pipeline()` / `from_pretrained()` are cached and reused across runs (location depends on Hugging Face cache configuration).
- Device and precision for large models:
  - Prefer explicit compute configuration for pipelines: `dtype=torch.bfloat16` (or `torch.float16`) and `device_map="auto"`.
- Tokenizer behavior knobs (commonly passed to `from_pretrained()`):
  - Examples: `do_lower_case=True` (model-dependent), plus tokenizer-specific kwargs; they are persisted in `tokenizer.init_kwargs`.
- Special tokens:
  - Add tokens: `tokenizer.add_tokens([...], special_tokens=True)`
  - Add extra specials: `tokenizer.add_special_tokens({"extra_special_tokens": [...]}, replace_extra_special_tokens=False)`
  - Note: tests mention `additional_special_tokens` is deprecated in v5 and converted to `extra_special_tokens` (prefer `extra_special_tokens` for new code).

## Pitfalls

### Wrong: Shadowing the `pipeline` factory with a Pipeline instance
```python
from transformers import pipeline

pipeline = pipeline(task="text-generation", model="Qwen/Qwen2.5-1.5B")
# Now `pipeline(...)` is no longer the factory function; it's a Pipeline object.
pipeline = pipeline(task="image-classification", model="facebook/dinov2-small-imagenet1k-1-layer")
```

### Right: Use distinct variable names for pipeline instances
```python
from transformers import pipeline

text_gen = pipeline(task="text-generation", model="Qwen/Qwen2.5-1.5B")
img_cls = pipeline(task="image-classification", model="facebook/dinov2-small-imagenet1k-1-layer")
```

### Wrong: Treating a chat/instruct model as a plain string prompt
```python
from transformers import pipeline

pipe = pipeline(task="text-generation", model="meta-llama/Meta-Llama-3-8B-Instruct")
out = pipe("Hey, can you tell me any fun things to do in New York?")
print(out)
```

### Right: Provide chat history as a list of `{role, content}` messages
```python
import torch
from transformers import pipeline

chat = [
    {"role": "system", "content": "You are a sassy, wise-cracking robot as imagined by Hollywood circa 1986."},
    {"role": "user", "content": "Hey, can you tell me any fun things to do in New York?"},
]

pipe = pipeline(
    task="text-generation",
    model="meta-llama/Meta-Llama-3-8B-Instruct",
    dtype=torch.bfloat16,
    device_map="auto",
)
response = pipe(chat, max_new_tokens=256)
print(response[0]["generated_text"][-1]["content"])
```

### Wrong: Calling a multimodal pipeline without required named inputs
```python
from transformers import pipeline

vqa = pipeline(task="visual-question-answering", model="Salesforce/blip-vqa-base")
# Missing `image=`; this cannot build inputs for VQA.
print(vqa("What is in the image?"))
```

### Right: Pass the task-specific named parameters
```python
from transformers import pipeline

vqa = pipeline(task="visual-question-answering", model="Salesforce/blip-vqa-base")
print(
    vqa(
        image="https://huggingface.co/datasets/huggingface/documentation-images/resolve/main/transformers/tasks/idefics-few-shot.jpg",
        question="What is in the image?",
    )
)
```

### Wrong: Adding a special token without marking it as special
```python
from transformers import AutoTokenizer

tokenizer = AutoTokenizer.from_pretrained("bert-base-uncased")
tokenizer.add_tokens(["[SPECIAL_TOKEN_1]"])  # not marked special
print(tokenizer.tokenize("[SPECIAL_TOKEN_1]"))
```

### Right: Mark special tokens (so tokenization/handling is consistent)
```python
from transformers import AutoTokenizer

tokenizer = AutoTokenizer.from_pretrained("bert-base-uncased")
tokenizer.add_tokens(["[SPECIAL_TOKEN_1]"], special_tokens=True)
assert tokenizer.tokenize("[SPECIAL_TOKEN_1]") == ["[SPECIAL_TOKEN_1]"]
```

## References

- [Official Documentation](https://huggingface.co/docs/transformers)
- [GitHub Repository](https://github.com/huggingface/transformers)
- parrots.png dataset image:
  - <https://huggingface.co/datasets/Narsil/image_dummy/raw/main/parrots.png>
- idefics-few-shot.jpg documentation image:
  - <https://huggingface.co/datasets/huggingface/documentation-images/resolve/main/transformers/tasks/idefics-few-shot.jpg>

## Migration from v[previous]

No explicit breaking-change notes were provided in the inputs.

Known deprecation note from tests:
- `additional_special_tokens` ⚠️ Soft Deprecation (not shown in examples)
  - Deprecated since: v5 (per test note)
  - Still works: likely (tests indicate it is converted)
  - Modern alternative: `extra_special_tokens`
  - Migration guidance: replace `{"additional_special_tokens": [...]}` with `{"extra_special_tokens": [...]}` when calling `tokenizer.add_special_tokens(...)`.

## API Reference

- **transformers.pipeline(task, model, \*\*kwargs)** - Create a task-specific `Pipeline`; key kwargs include `dtype=`, `device_map=`, `revision=`.
- **transformers.Pipeline(\_\_call\_\_)** - Run inference on a single input or batch; returns task-specific structured outputs.
- **transformers.AutoTokenizer.from_pretrained(pretrained_model_name_or_path, \*\*kwargs)** - Load tokenizer from Hub id or local directory; accepts tokenizer-specific kwargs.
- **PreTrainedTokenizerBase.save_pretrained(save_directory)** - Save tokenizer files/config to a directory for later reuse.
- **PreTrainedTokenizerBase.\_\_call\_\_(texts, padding=False, truncation=False, return_tensors=None, \*\*kwargs)** - Batch tokenize; returns a `BatchEncoding`.
- **transformers.BatchEncoding.data** - Underlying dict-like mapping of encoded fields (e.g., `input_ids`, `attention_mask`).
- **PreTrainedTokenizerBase.encode(text, add_special_tokens=True, \*\*kwargs)** - Encode text to token ids.
- **PreTrainedTokenizerBase.decode(ids, skip_special_tokens=False, clean_up_tokenization_spaces=True, \*\*kwargs)** - Decode token ids to text.
- **PreTrainedTokenizerBase.tokenize(text, \*\*kwargs)** - Convert text to token strings.
- **PreTrainedTokenizerBase.get_vocab()** - Return vocabulary mapping token string → id.
- **PreTrainedTokenizerBase.add_tokens(new_tokens, special_tokens=False)** - Add tokens to tokenizer; use `special_tokens=True` for special tokens.
- **PreTrainedTokenizerBase.add_special_tokens(special_tokens_dict, replace_extra_special_tokens=True)** - Add special tokens; supports `extra_special_tokens`.
- **PreTrainedTokenizerBase.model_input_names** - List of primary model input field names (often starts with `input_ids` or `input_values`).
- **transformers.utils.metrics.attach_tracer()** - Class decorator that attaches a tracer to instances (`self.tracer`).
- **transformers.utils.metrics.traced(span_name=None, additional_attributes=None)** - Decorator for methods/functions to create tracing spans; supports custom span names and attributes.