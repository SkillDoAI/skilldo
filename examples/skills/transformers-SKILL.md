---
name: transformers
version: unknown
ecosystem: python
license: Apache 2.0 License"
generated_with: claude-sonnet-4-5-20250929
---

```markdown
---
name: transformers
version: unknown
ecosystem: python
license: Apache 2.0 License
generated_with: qwen3-coder:latest + gpt-5.2 (agent5)
---

# transformers

**Hugging Face Transformers** is a library providing state-of-the-art pretrained models for Natural Language Processing (NLP), Computer Vision, Audio, and Multimodal tasks. It offers thousands of pretrained models with a simple, unified API for PyTorch, TensorFlow, and JAX.

## Installation

```bash
# Stable release with PyTorch support
pip install 'transformers[torch]'

# Or with TensorFlow
pip install 'transformers[tf]'

# Or with JAX
pip install 'transformers[flax]'

# From source (latest, potentially unstable)
git clone https://github.com/huggingface/transformers
cd transformers
pip install -e '.[torch]'
```

**Best Practice:** Use a virtual environment (venv or uv) to isolate dependencies.

## Imports

```python
# Core imports for model loading
from transformers import (
    AutoTokenizer,
    AutoModel,
    AutoModelForCausalLM,
    AutoModelForSequenceClassification,
    AutoProcessor,
    AutoModelForVision2Seq,
)

# Pipeline API
from transformers import pipeline

# Training utilities
from transformers import Trainer, TrainingArguments

# Generation utilities
from transformers import (
    GenerationConfig,
    TextIteratorStreamer,
    DynamicCache,
    StaticCache,
)

# Quantization
from transformers import BitsAndBytesConfig, HfQuantizer
from transformers.quantizers import register_quantization_config, register_quantizer
from transformers.utils.quantization_config import QuantizationConfigMixin

# Token management
from transformers import AddedToken

# Common dependencies
import torch
from PIL import Image
from datasets import load_dataset
```

## Core Patterns

### Pipeline API for Common Tasks

The Pipeline API provides task-specific interfaces for quick prototyping:

```python
from transformers import pipeline

# Text generation
generator = pipeline("text-generation", model="gpt2")
result = generator("Hello, I am", max_length=50)

# Chat interface
chatbot = pipeline("text-generation", model="meta-llama/Llama-3.2-3B-Instruct")
messages = [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "What is the capital of France?"}
]
outputs = chatbot(messages, max_new_tokens=128)
print(outputs[0]["generated_text"][-1]["content"])

# Sentiment analysis
classifier = pipeline("text-classification", model="distilbert-base-uncased-finetuned-sst-2-english")
result = classifier("I love this product!")

# Named Entity Recognition
ner = pipeline("token-classification", model="dbloss/bert-base-NER")
result = ner("My name is Sarah and I live in London")

# Question Answering
qa = pipeline("question-answering", model="distilbert-base-cased-distilled-squad")
result = qa(question="What is my name?", context="My name is Clara and I live in Berkeley")

# Summarization
summarizer = pipeline("summarization", model="facebook/bart-large-cnn")
result = summarizer("Long article text here...", max_length=130, min_length=30)

# Translation
translator = pipeline("translation_en_to_de", model="t5-base")
result = translator("Hello, how are you?")

# Automatic Speech Recognition
asr = pipeline("automatic-speech-recognition", model="openai/whisper-base")
result = asr("audio_file.mp3")

# Image Classification
classifier = pipeline("image-classification", model="google/vit-base-patch16-224")
result = classifier("path/to/image.jpg")

# Image Segmentation
segmenter = pipeline("image-segmentation", model="facebook/detr-resnet-50-panoptic")
result = segmenter("path/to/image.jpg")

# Object Detection
detector = pipeline("object-detection", model="facebook/detr-resnet-50")
result = detector("path/to/image.jpg")

# Zero-shot Classification
classifier = pipeline("zero-shot-classification", model="facebook/bart-large-mnli")
result = classifier("This is a course about Python", candidate_labels=["education", "politics", "business"])
```

### Loading Models with Auto Classes

```python
from transformers import AutoTokenizer, AutoModel, AutoModelForCausalLM
import torch

# Load tokenizer and model automatically
tokenizer = AutoTokenizer.from_pretrained("bert-base-uncased")
model = AutoModel.from_pretrained("bert-base-uncased")

# For causal language models (GPT-style)
model = AutoModelForCausalLM.from_pretrained(
    "meta-llama/Llama-3.2-1B",
    torch_dtype=torch.bfloat16,
    device_map="auto"  # Automatically distribute across GPUs
)

# Tokenize and run inference
inputs = tokenizer("Hello, how are you?", return_tensors="pt")
outputs = model(**inputs)
```

### Manual Tokenization and Inference

```python
from transformers import AutoTokenizer, AutoModelForCausalLM
import torch

tokenizer = AutoTokenizer.from_pretrained("gpt2")
model = AutoModelForCausalLM.from_pretrained("gpt2")

# Tokenize input
inputs = tokenizer("The future of AI is", return_tensors="pt")

# Generate text
with torch.no_grad():
    outputs = model.generate(
        inputs.input_ids,
        max_new_tokens=50,
        temperature=0.7,
        do_sample=True
    )

# Decode output
generated_text = tokenizer.decode(outputs[0], skip_special_tokens=True)
print(generated_text)
```

### Text Generation with Streaming

```python
from transformers import AutoTokenizer, AutoModelForCausalLM, TextIteratorStreamer
from threading import Thread

tokenizer = AutoTokenizer.from_pretrained("gpt2")
model = AutoModelForCausalLM.from_pretrained("gpt2")

# Setup streamer
streamer = TextIteratorStreamer(tokenizer, skip_prompt=True)

# Prepare inputs
inputs = tokenizer("Once upon a time", return_tensors="pt")

# Generate in background thread
generation_kwargs = dict(inputs, streamer=streamer, max_new_tokens=100)
thread = Thread(target=model.generate, kwargs=generation_kwargs)
thread.start()

# Stream output
for new_text in streamer:
    print(new_text, end="", flush=True)

thread.join()
```

### Training with Trainer API

```python
from transformers import (
    AutoTokenizer,
    AutoModelForSequenceClassification,
    Trainer,
    TrainingArguments
)
from datasets import load_dataset

# Load dataset
dataset = load_dataset("imdb")

# Load tokenizer and model
tokenizer = AutoTokenizer.from_pretrained("bert-base-uncased")
model = AutoModelForSequenceClassification.from_pretrained(
    "bert-base-uncased",
    num_labels=2
)

# Tokenize dataset
def tokenize_function(examples):
    return tokenizer(examples["text"], padding="max_length", truncation=True)

tokenized_datasets = dataset.map(tokenize_function, batched=True)

# Setup training arguments
training_args = TrainingArguments(
    output_dir="./results",
    num_train_epochs=3,
    per_device_train_batch_size=8,
    per_device_eval_batch_size=8,
    learning_rate=2e-5,
    evaluation_strategy="epoch",
    save_strategy="epoch",
    logging_dir="./logs",
    logging_steps=100,
    load_best_model_at_end=True,
)

# Create Trainer
trainer = Trainer(
    model=model,
    args=training_args,
    train_dataset=tokenized_datasets["train"],
    eval_dataset=tokenized_datasets["test"],
    tokenizer=tokenizer,
)

# Train
trainer.train()

# Evaluate
results = trainer.evaluate()
print(results)

# Make predictions
predictions = trainer.predict(tokenized_datasets["test"])
```

### Multi-modal Processing (Vision + Text)

```python
from transformers import AutoProcessor, AutoModelForVision2Seq
from PIL import Image
import torch

# Load processor and model
processor = AutoProcessor.from_pretrained("Salesforce/blip-image-captioning-base")
model = AutoModelForVision2Seq.from_pretrained("Salesforce/blip-image-captioning-base")

# Load image
image = Image.open("path/to/image.jpg")

# Process inputs
inputs = processor(images=image, return_tensors="pt")

# Generate caption
outputs = model.generate(**inputs, max_new_tokens=50)
caption = processor.decode(outputs[0], skip_special_tokens=True)
print(caption)
```

### Quantized Models (4-bit/8-bit)

```python
from transformers import AutoModelForCausalLM, AutoTokenizer, BitsAndBytesConfig
import torch

# Configure 4-bit quantization
quantization_config = BitsAndBytesConfig(
    load_in_4bit=True,
    bnb_4bit_compute_dtype=torch.bfloat16,
    bnb_4bit_quant_type="nf4",
    bnb_4bit_use_double_quant=True,
)

# Load quantized model
model = AutoModelForCausalLM.from_pretrained(
    "meta-llama/Llama-3.2-1B",
    quantization_config=quantization_config,
    device_map="auto"
)

tokenizer = AutoTokenizer.from_pretrained("meta-llama/Llama-3.2-1B")

# Use model normally
inputs = tokenizer("Hello, world!", return_tensors="pt").to(model.device)
outputs = model.generate(**inputs, max_new_tokens=50)
```

### Custom Quantization (Advanced)

```python
from transformers import (
    AutoModelForCausalLM,
    AutoTokenizer,
    HfQuantizer
)
from transformers.quantizers import register_quantization_config, register_quantizer
from transformers.utils.quantization_config import QuantizationConfigMixin
import torch

# Define custom quantization config
@register_quantization_config("custom")
class CustomConfig(QuantizationConfigMixin):
    def __init__(self, bits=8, **kwargs):
        self.quant_method = "custom"
        self.bits = bits

# Define custom quantizer
@register_quantizer("custom")
class CustomQuantizer(HfQuantizer):
    def __init__(self, quantization_config, **kwargs):
        super().__init__(quantization_config, **kwargs)
        self.quantization_config = quantization_config

    def _process_model_before_weight_loading(self, model, **kwargs):
        # Custom logic before loading weights
        return True

    def _process_model_after_weight_loading(self, model, **kwargs):
        # Custom logic after loading weights
        return True

# Use custom quantization
model = AutoModelForCausalLM.from_pretrained(
    "facebook/opt-350m",
    quantization_config=CustomConfig(bits=8),
    torch_dtype="auto"
)
```

### Using Generation Configs

```python
from transformers import AutoModelForCausalLM, AutoTokenizer, GenerationConfig
import torch

model = AutoModelForCausalLM.from_pretrained("gpt2")
tokenizer = AutoTokenizer.from_pretrained("gpt2")

# Create custom generation config
generation_config = GenerationConfig(
    max_new_tokens=100,
    do_sample=True,
    temperature=0.7,
    top_p=0.9,
    top_k=50,
    repetition_penalty=1.2,
    no_repeat_ngram_size=3,
)

# Generate with config
inputs = tokenizer("The AI revolution", return_tensors="pt")
outputs = model.generate(**inputs, generation_config=generation_config)
print(tokenizer.decode(outputs[0]))
```

### Cache Management for Generation

```python
from transformers import AutoModelForCausalLM, AutoTokenizer, DynamicCache, StaticCache
import torch

model = AutoModelForCausalLM.from_pretrained("gpt2")
tokenizer = AutoTokenizer.from_pretrained("gpt2")

# Use dynamic cache (default)
cache = DynamicCache()
inputs = tokenizer("Hello", return_tensors="pt")
outputs = model.generate(**inputs, past_key_values=cache, max_new_tokens=50)

# Use static cache for compilation
cache = StaticCache()
outputs = model.generate(**inputs, past_key_values=cache, max_new_tokens=50)
```

### Distributed Training Setup

```python
import os
import torch
import torch.distributed as dist
from transformers import Trainer, TrainingArguments

# Get environment variables set by torch.distributed.launch
LOCAL_RANK = int(os.environ.get("LOCAL_RANK", 0))
WORLD_SIZE = int(os.environ.get("WORLD_SIZE", 1))
WORLD_RANK = int(os.environ.get("RANK", 0))

# Initialize process group
def init_distributed():
    dist.init_process_group(
        backend="nccl",  # or "gloo" for CPU
        rank=WORLD_RANK,
        world_size=WORLD_SIZE
    )
    torch.cuda.set_device(LOCAL_RANK)

# Use in training
if WORLD_SIZE > 1:
    init_distributed()

training_args = TrainingArguments(
    output_dir="./results",
    local_rank=LOCAL_RANK,  # Trainer handles distribution automatically
    # ... other args
)
```

### Distributed Communication Example

```python
import torch
import torch.distributed as dist

# Assumes distributed environment is initialized
device = torch.device(f"cuda:{LOCAL_RANK}")
tensor = torch.zeros(1).to(device)

# Rank 0 sends to all other ranks
if WORLD_RANK == 0:
    for rank_recv in range(1, WORLD_SIZE):
        dist.send(tensor=tensor, dst=rank_recv)
        print(f"Rank 0 sent data to Rank {rank_recv}")
else:
    dist.recv(tensor=tensor, src=0)
    print(f"Rank {WORLD_RANK} received data from Rank 0")
```

### Adding Custom Tokens

```python
from transformers import AutoTokenizer, AddedToken

tokenizer = AutoTokenizer.from_pretrained("gpt2")

# Add simple token
tokenizer.add_tokens(["[SPECIAL]"])

# Add token with specific behavior
special_token = AddedToken(
    "[CUSTOM]",
    single_word=False,
    lstrip=False,
    rstrip=False,
    normalized=True,
    special=True
)
tokenizer.add_tokens([special_token])

# Add special tokens
tokenizer.add_special_tokens({
    "additional_special_tokens": ["[USER]", "[ASSISTANT]"]
})

# Resize model embeddings to match new vocabulary
model.resize_token_embeddings(len(tokenizer))
```

### Batch Encoding and Attention Masks

```python
from transformers import AutoTokenizer

tokenizer = AutoTokenizer.from_pretrained("bert-base-uncased")

# Single sequence
encoded = tokenizer("Hello, world!")
print(encoded)
# {'input_ids': [...], 'token_type_ids': [...], 'attention_mask': [...]}

# Batch encoding with padding
texts = ["Short text", "This is a much longer text that needs padding"]
encoded = tokenizer(
    texts,
    padding=True,  # Pad to longest sequence in batch
    truncation=True,
    max_length=512,
    return_tensors