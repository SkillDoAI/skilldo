use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
}

pub struct MockLlmClient;

impl Default for MockLlmClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockLlmClient {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        // Return different mock responses based on which agent is calling
        // Agent detection for v2 prompts
        if prompt.contains("Extract the complete public API surface") {
            // Agent 1: API Extractor (v2)
            Ok(r#"{
  "apis": [
    {
      "name": "FastAPI",
      "type": "class",
      "signature": "FastAPI(debug: bool = False, routes: List[BaseRoute] = None, ...)",
      "return_type": "FastAPI",
      "module": "fastapi.applications",
      "deprecated": false
    },
    {
      "name": "APIRouter",
      "type": "class",
      "signature": "APIRouter(prefix: str = '', tags: List[str] = None, ...)",
      "return_type": "APIRouter",
      "module": "fastapi.routing",
      "deprecated": false
    },
    {
      "name": "Request",
      "type": "class",
      "module": "fastapi",
      "deprecated": false
    },
    {
      "name": "Response",
      "type": "class",
      "module": "fastapi",
      "deprecated": false
    },
    {
      "name": "HTTPException",
      "type": "class",
      "signature": "HTTPException(status_code: int, detail: Any = None, headers: dict = None)",
      "module": "fastapi",
      "deprecated": false
    },
    {
      "name": "Depends",
      "type": "function",
      "signature": "Depends(dependency: Callable = None, use_cache: bool = True)",
      "return_type": "Any",
      "module": "fastapi",
      "deprecated": false
    },
    {
      "name": "Path",
      "type": "function",
      "signature": "Path(..., description: str = None, gt: float = None, ...)",
      "module": "fastapi",
      "deprecated": false
    },
    {
      "name": "Query",
      "type": "function",
      "signature": "Query(default = ..., description: str = None, ...)",
      "module": "fastapi",
      "deprecated": false
    },
    {
      "name": "Body",
      "type": "function",
      "signature": "Body(default = ..., embed: bool = False, ...)",
      "module": "fastapi",
      "deprecated": false
    }
  ]
}"#
            .to_string())
        } else if prompt.contains("Extract correct usage patterns from the tests") {
            // Agent 2: Pattern Extractor (v2)
            Ok(r#"{
  "patterns": [
    {
      "api": "FastAPI",
      "pattern": "Basic app creation",
      "setup": "from fastapi import FastAPI\napp = FastAPI()",
      "usage": "app.get('/')\\ndef root(): return {'message': 'Hello'}",
      "assertion": "client.get('/').json() == {'message': 'Hello'}"
    },
    {
      "api": "Path parameters",
      "pattern": "Path parameter with type",
      "setup": "@app.get('/items/{item_id}')",
      "usage": "def read_item(item_id: int): return {'item_id': item_id}",
      "config": "Type hints are validated automatically"
    },
    {
      "api": "Depends",
      "pattern": "Dependency injection",
      "setup": "def common_params(q: str = None): return q",
      "usage": "@app.get('/items/')\\ndef read(commons = Depends(common_params)): ...",
      "config": "Dependencies are cached by default"
    },
    {
      "api": "Pydantic models",
      "pattern": "Request body validation",
      "setup": "class Item(BaseModel):\\n    name: str\\n    price: float",
      "usage": "@app.post('/items/')\\ndef create(item: Item): return item",
      "config": "Automatic JSON parsing and validation"
    }
  ]
}"#
            .to_string())
        } else if prompt
            .contains("Extract conventions, best practices, pitfalls, and migration notes")
        {
            // Agent 3: Context Extractor (v2)
            Ok(r#"{
  "conventions": [
    "Use async def for endpoints when performing I/O operations",
    "Type hints are required for automatic validation and docs",
    "Path operations are declared with decorators like @app.get()",
    "Pydantic models for request/response schemas"
  ],
  "pitfalls": [
    {
      "mistake": "Forgetting 'await' with async dependencies",
      "consequence": "RuntimeWarning or incorrect behavior",
      "solution": "Always await async dependencies"
    },
    {
      "mistake": "Not using Depends() for shared logic",
      "consequence": "Code duplication, harder testing",
      "solution": "Extract common code into dependencies"
    },
    {
      "mistake": "Mutable default arguments in path operations",
      "consequence": "Shared state across requests",
      "solution": "Use Depends() or None defaults"
    }
  ],
  "breaking_changes": [],
  "migration_notes": []
}"#
            .to_string())
        } else if prompt.contains("creating an agent rules file")
            || prompt.contains("Here is the current SKILL.md")
        {
            // Agent 4: Synthesizer
            Ok(r#"---
name: fastapi
description: Modern, fast web framework for building APIs with Python
version: unknown
ecosystem: python
---

## Imports

```python
from fastapi import FastAPI, APIRouter, Request, Response, HTTPException
from fastapi import Depends, Path, Query, Body, Header, Cookie
from fastapi.responses import JSONResponse, HTMLResponse
from pydantic import BaseModel
```

## Core Patterns

### Basic Application

```python
from fastapi import FastAPI

app = FastAPI()

@app.get("/")
def read_root():
    return {"message": "Hello World"}
```

### Path Parameters

```python
@app.get("/items/{item_id}")
def read_item(item_id: int):
    return {"item_id": item_id}
```

### Query Parameters

```python
@app.get("/items/")
def read_items(skip: int = 0, limit: int = 10):
    return {"skip": skip, "limit": limit}
```

### Request Body with Pydantic

```python
from pydantic import BaseModel

class Item(BaseModel):
    name: str
    price: float
    is_offer: bool = False

@app.post("/items/")
def create_item(item: Item):
    return item
```

### Dependency Injection

```python
from fastapi import Depends

def common_params(q: str = None, skip: int = 0):
    return {"q": q, "skip": skip}

@app.get("/items/")
def read_items(commons: dict = Depends(common_params)):
    return commons
```

## Configuration

```python
app = FastAPI(
    title="My API",
    description="API description",
    version="1.0.0",
    docs_url="/docs",  # Swagger UI
    redoc_url="/redoc"  # ReDoc
)
```

## Pitfalls

### Wrong: Forgetting await with async

```python
@app.get("/users/")
async def get_users(db = Depends(get_db)):
    users = db.query()  # Missing await!
    return users
```

### Right: Use await

```python
@app.get("/users/")
async def get_users(db = Depends(get_db)):
    users = await db.query()
    return users
```

### Wrong: Mutable defaults

```python
@app.post("/items/")
def create(tags: list = []):  # Shared across requests!
    return tags
```

### Right: Use None or Depends

```python
@app.post("/items/")
def create(tags: list = None):
    tags = tags or []
    return tags
```

## API Reference

- **FastAPI()** - Main application class
- **APIRouter()** - Modular route groups
- **Depends()** - Dependency injection
- **Path()** - Path parameter with validation
- **Query()** - Query parameter with validation
- **Body()** - Request body parameter
- **HTTPException()** - Raise HTTP errors
- **Request** - Access raw request
- **Response** - Custom response
"#
            .to_string())
        } else if prompt.contains("reviewing a generated SKILL.md") {
            // Agent 5: Reviewer
            Ok(r#"{"status": "pass"}"#.to_string())
        } else {
            Ok(r#"{"status": "mock"}"#.to_string())
        }
    }
}
