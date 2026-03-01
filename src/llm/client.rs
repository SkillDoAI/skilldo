use anyhow::Result;
use async_trait::async_trait;
use tracing::warn;

#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, prompt: &str) -> Result<String>;
}

/// Wraps any LlmClient with retry logic for transient network errors.
pub struct RetryClient {
    inner: Box<dyn LlmClient>,
    max_retries: usize,
    retry_delay_secs: u64,
}

impl RetryClient {
    pub fn new(inner: Box<dyn LlmClient>, max_retries: usize, retry_delay_secs: u64) -> Self {
        Self {
            inner,
            max_retries,
            retry_delay_secs,
        }
    }
}

#[async_trait]
impl LlmClient for RetryClient {
    async fn complete(&self, prompt: &str) -> Result<String> {
        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match self.inner.complete(prompt).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    let err_str = e.to_string();
                    let is_transient = err_str.contains("connection closed")
                        || err_str.contains("timed out")
                        || err_str.contains("reset by peer")
                        || err_str.contains("broken pipe")
                        || err_str.contains("429")
                        || err_str.contains("503")
                        || err_str.contains("502")
                        || err_str.contains("500 Internal");

                    if is_transient && attempt < self.max_retries {
                        warn!(
                            "Transient error (attempt {}/{}), retrying in {}s: {}",
                            attempt + 1,
                            self.max_retries + 1,
                            self.retry_delay_secs,
                            err_str.lines().next().unwrap_or(&err_str)
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(self.retry_delay_secs))
                            .await;
                        last_error = Some(e);
                        continue;
                    }

                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap())
    }
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
            // Extract agent: API Extractor (v2)
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
            // Map agent: Pattern Extractor (v2)
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
            // Learn agent: Context Extractor (v2)
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
            // Create agent: Synthesizer
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
        } else if prompt.contains("verification script generator") {
            // Review agent Phase A: introspection script
            Ok(r#"```python
# /// script
# requires-python = ">=3.10"
# dependencies = ["testpkg"]
# ///
import json
result = {"version_installed": "1.0.0", "version_expected": "1.0.0", "imports": [], "signatures": [], "dates": []}
print(json.dumps(result))
```"#
            .to_string())
        } else if prompt.contains("quality gate for a generated SKILL.md") {
            // Review agent Phase B: verdict
            Ok(r#"{"passed": true, "issues": []}"#.to_string())
        } else {
            Ok(r#"{"status": "mock"}"#.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A mock LlmClient that fails a configurable number of times with a given
    /// error message, then succeeds. Tracks total call count.
    struct FailThenSucceed {
        call_count: Arc<AtomicUsize>,
        fail_times: usize,
        error_msg: String,
    }

    impl FailThenSucceed {
        fn new(call_count: Arc<AtomicUsize>, fail_times: usize, error_msg: &str) -> Self {
            Self {
                call_count,
                fail_times,
                error_msg: error_msg.to_string(),
            }
        }
    }

    #[async_trait]
    impl LlmClient for FailThenSucceed {
        async fn complete(&self, _prompt: &str) -> Result<String> {
            let n = self.call_count.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_times {
                Err(anyhow::anyhow!("{}", self.error_msg))
            } else {
                Ok("success".to_string())
            }
        }
    }

    #[tokio::test]
    async fn test_retry_succeeds_first_try() {
        let count = Arc::new(AtomicUsize::new(0));
        let mock = FailThenSucceed::new(count.clone(), 0, "unused");
        let client = RetryClient::new(Box::new(mock), 3, 0);

        let result = client.complete("hello").await.unwrap();

        assert_eq!(result, "success");
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_retry_succeeds_after_transient_failure() {
        let count = Arc::new(AtomicUsize::new(0));
        let mock = FailThenSucceed::new(count.clone(), 1, "connection closed");
        let client = RetryClient::new(Box::new(mock), 3, 0);

        let result = client.complete("hello").await.unwrap();

        assert_eq!(result, "success");
        assert_eq!(count.load(Ordering::SeqCst), 2); // 1 failure + 1 success
    }

    #[tokio::test]
    async fn test_retry_non_transient_error_no_retry() {
        let count = Arc::new(AtomicUsize::new(0));
        let mock = FailThenSucceed::new(count.clone(), 10, "invalid API key");
        let client = RetryClient::new(Box::new(mock), 3, 0);

        let result = client.complete("hello").await;

        assert!(result.is_err());
        assert_eq!(count.load(Ordering::SeqCst), 1); // no retry for non-transient
        assert!(result.unwrap_err().to_string().contains("invalid API key"));
    }

    #[tokio::test]
    async fn test_retry_exhausted() {
        let count = Arc::new(AtomicUsize::new(0));
        let mock = FailThenSucceed::new(count.clone(), 100, "connection closed");
        let client = RetryClient::new(Box::new(mock), 2, 0);

        let result = client.complete("hello").await;

        assert!(result.is_err());
        // 1 initial attempt + 2 retries = 3 total calls
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("connection closed"));
    }

    #[tokio::test]
    async fn test_retry_recognizes_all_transient_errors() {
        let transient_errors = [
            "connection closed by server",
            "request timed out",
            "reset by peer",
            "broken pipe",
            "HTTP 429 rate limited",
            "HTTP 503 service unavailable",
            "HTTP 502 bad gateway",
            "500 Internal server error",
        ];

        for error_msg in &transient_errors {
            let count = Arc::new(AtomicUsize::new(0));
            let mock = FailThenSucceed::new(count.clone(), 1, error_msg);
            let client = RetryClient::new(Box::new(mock), 3, 0);

            let result = client.complete("hello").await;

            assert!(
                result.is_ok(),
                "Expected retry+success for transient error '{}', got: {:?}",
                error_msg,
                result
            );
            assert_eq!(
                count.load(Ordering::SeqCst),
                2,
                "Expected exactly 2 calls (1 fail + 1 success) for '{}'",
                error_msg
            );
        }
    }
}
