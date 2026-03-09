---
name: boto3
description: AWS SDK for Python for creating service clients/resources and calling AWS APIs.
license: Apache-2.0
metadata:
  version: "1.42.63"
  ecosystem: python
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---

## Imports

```python
import boto3

# ClientError lives in botocore.exceptions; import it safely for all supported versions.
try:
    from botocore.exceptions import ClientError
except ImportError:  # pragma: no cover
    import botocore.exceptions as _exc
    ClientError = _exc.ClientError  # type: ignore

# TransferConfig provides fine‑grained control over multipart uploads/downloads.
# In some environments the boto3.s3 submodule is not eagerly loaded, which can cause
# an AttributeError when importing TransferConfig directly. The following block
# ensures the submodule is imported first and falls back to a minimal stub if needed.
try:
    import importlib
    importlib.import_module("boto3.s3.transfer")
    from boto3.s3.transfer import TransferConfig
except Exception:  # pragma: no cover
    # Define a very small placeholder that mimics the real TransferConfig's signature.
    class TransferConfig:  # type: ignore
        def __init__(self, *args, **kwargs):
            pass
```

## Core Patterns

### Create a low‑level service client ✅ Current
```python
import boto3

def make_s3_client(region_name: str = "us-east-1"):
    # Prefer credentials/region from ~/.aws/{credentials,config};
    # pass region_name explicitly when you need deterministic behavior.
    # boto3.client accepts *args, **kwargs – the first positional argument is the service name.
    return boto3.client("s3", region_name=region_name)

if __name__ == "__main__":
    s3 = make_s3_client()
    # This call is safe to run; it only constructs the client.
    print(f"Created client: {s3.meta.service_model.service_name}")
```
* Use `boto3.client("service_name", region_name=…)` for explicit API operations (e.g., `get_bucket_cors`, `put_bucket_cors`, SES/EC2 operations).

### Create a high‑level service resource ✅ Current
```python
import boto3

def make_s3_resource(region_name: str = "us-east-1"):
    # boto3.resource also uses *args, **kwargs – the first positional argument is the service name.
    return boto3.resource("s3", region_name=region_name)

if __name__ == "__main__":
    s3 = make_s3_resource()
    # Resource objects provide an OO interface; operations require AWS access when called.
    print(f"Created resource: {s3.meta.service_name}")
```
* Use `boto3.resource("service_name", …)` when you want object‑oriented access patterns.

### Handle AWS errors with `ClientError` and error codes ✅ Current
```python
import logging

import boto3
from botocore.exceptions import ClientError

logging.basicConfig(level=logging.INFO)

def get_bucket_cors_rules(bucket_name: str, region_name: str = "us-east-1") -> list[dict]:
    s3 = boto3.client("s3", region_name=region_name)
    try:
        return s3.get_bucket_cors(Bucket=bucket_name)["CORSRules"]
    except ClientError as e:
        code = e.response.get("Error", {}).get("Code")
        if code == "NoSuchCORSConfiguration":
            return []
        raise  # re‑raise unexpected failures (permissions, missing bucket, etc.)

if __name__ == "__main__":
    # Example usage (will make a network call if executed with a real bucket name)
    bucket = "amzn-s3-demo-bucket"
    try:
        rules = get_bucket_cors_rules(bucket)
        logging.info("CORS rules: %s", rules)
    except ClientError as e:
        logging.error("AWS error: %s", e)
```
* Catch `botocore.exceptions.ClientError` (not generic `Exception`) and branch on `e.response['Error']['Code']` for expected conditions.

### Put S3 bucket CORS using the documented request shape ✅ Current
```python
import boto3

def set_bucket_cors(bucket_name: str, region_name: str = "us-east-1") -> None:
    s3 = boto3.client("s3", region_name=region_name)

    cors_configuration: dict = {
        "CORSRules": [
            {
                "AllowedHeaders": ["Authorization"],
                "AllowedMethods": ["GET", "PUT"],
                "AllowedOrigins": ["*"],
                "ExposeHeaders": ["ETag", "x-amz-request-id"],
                "MaxAgeSeconds": 3000,
            }
        ]
    }

    s3.put_bucket_cors(Bucket=bucket_name, CORSConfiguration=cors_configuration)

if __name__ == "__main__":
    # Requires an existing bucket and permissions when executed.
    set_bucket_cors("amzn-s3-demo-bucket")
```
* The top‑level key must be `CORSRules` inside `CORSConfiguration=…`.

### Manage SES templates and send templated email ✅ Current
```python
import boto3

def ensure_template(template_name: str, region_name: str = "us-east-1") -> None:
    ses = boto3.client("ses", region_name=region_name)

    valid_html = "<html><body><p>Hello</p></body></html>"
    template: dict = {
        "TemplateName": template_name,
        "SubjectPart": "SUBJECT_LINE",
        "TextPart": "TEXT_CONTENT",
        "HtmlPart": valid_html,
    }

    # Create or update depending on existence
    existing = ses.list_templates().get("TemplatesMetadata", [])
    if any(t.get("Name") == template_name for t in existing):
        ses.update_template(Template=template)
    else:
        ses.create_template(Template=template)

def send_email(template_name: str, to_address: str, from_address: str, region_name: str = "us-east-1") -> None:
    ses = boto3.client("ses", region_name=region_name)

    ses.send_templated_email(
        Source=from_address,
        Destination={"ToAddresses": [to_address]},
        Template=template_name,
        TemplateData='{"name":"World"}',
    )

if __name__ == "__main__":
    # Requires verified identities in SES and correct permissions when executed.
    ensure_template("TEMPLATE_NAME")
    # send_email("TEMPLATE_NAME", "recipient@example.com", "sender@example.com")
```
* SES does not validate HTML; validate/format HTML yourself before calling `create_template`/`update_template`.
* Template operations: `create_template`, `list_templates`, `get_template`, `update_template`, `delete_template`, `send_templated_email`.

### Upload and download files to/from S3 ✅ Current
```python
import boto3
from boto3.s3.transfer import TransferConfig

def upload_file_to_s3(file_path: str, bucket: str, key: str, region_name: str = "us-east-1") -> None:
    """Upload a file to S3 using the high‑level upload_file method."""
    s3 = boto3.client("s3", region_name=region_name)

    # Configure transfer options
    config = TransferConfig(
        multipart_threshold=8 * 1024 * 1024,  # 8 MiB
        max_concurrency=10,
        use_threads=True,
    )

    s3.upload_file(
        Filename=file_path,
        Bucket=bucket,
        Key=key,
        Config=config,
    )

def download_file_from_s3(bucket: str, key: str, file_path: str, region_name: str = "us-east-1") -> None:
    """Download a file from S3 using the high‑level download_file method."""
    s3 = boto3.client("s3", region_name=region_name)

    config = TransferConfig(
        multipart_threshold=8 * 1024 * 1024,
        max_concurrency=10,
        use_threads=True,
    )

    s3.download_file(
        Bucket=bucket,
        Key=key,
        Filename=file_path,
        Config=config,
    )

if __name__ == "__main__":
    # Example usage (requires a valid bucket and permissions)
    # upload_file_to_s3("local_file.txt", "my-bucket", "uploads/file.txt")
    # download_file_from_s3("my-bucket", "uploads/file.txt", "downloaded_file.txt")
    pass
```
* Use `upload_file()` and `download_file()` for efficient transfers with automatic multipart handling.
* `TransferConfig` lets you tune multipart thresholds, concurrency, and threading.

### Use S3 resources for object‑oriented operations ✅ Current
```python
import boto3

def upload_with_resource(file_path: str, bucket_name: str, key: str, region_name: str = "us-east-1") -> None:
    """Upload using the S3 Bucket resource."""
    s3 = boto3.resource("s3", region_name=region_name)
    bucket = s3.Bucket(bucket_name)

    bucket.upload_file(Filename=file_path, Key=key)

def load_bucket_metadata(bucket_name: str, region_name: str = "us-east-1") -> dict:
    """Load bucket metadata using the resource interface."""
    s3 = boto3.resource("s3", region_name=region_name)
    bucket = s3.Bucket(bucket_name)

    # Calling load() fetches bucket metadata
    bucket.load()

    return {
        "name": bucket.name,
        "creation_date": bucket.creation_date,
    }

if __name__ == "__main__":
    # Example usage (requires a valid bucket)
    # upload_with_resource("file.txt", "my-bucket", "uploads/file.txt")
    # metadata = load_bucket_metadata("my-bucket")
    pass
```
* S3 resources provide object‑oriented interfaces: `Bucket.upload_file()`, `Object.download_file()`, etc.
* Call `load()` on bucket/object resources to fetch metadata from AWS.

### Use Session for custom credential and region configuration ✅ Current
```python
import boto3

def create_session_with_profile(profile_name: str) -> boto3.Session:
    """Create a session using a named profile from ~/.aws/config."""
    return boto3.Session(profile_name=profile_name)

def create_client_from_session(profile_name: str, service_name: str, region_name: str):
    """Create a client using custom session configuration."""
    session = boto3.Session(
        profile_name=profile_name,
        region_name=region_name,
    )
    return session.client(service_name)

if __name__ == "__main__":
    # Use a specific profile and region
    s3 = create_client_from_session("dev-profile", "s3", "us-west-2")
    # s3.list_buckets()
```
* Use `boto3.Session()` to manage custom credential profiles and regions.
* Create clients and resources from sessions to isolate configuration.

## Configuration

- Preferred configuration is via shared config files (avoid hardcoding credentials):
  - `~/.aws/credentials` (profiles, access keys)
  - `~/.aws/config` (default region/output, named profiles)
- Common environment variables (standard AWS SDK behavior):
  - `AWS_PROFILE` (select profile)
  - `AWS_REGION` / `AWS_DEFAULT_REGION` (default region)
  - `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` (temporary credentials)
- When region must be deterministic (CI, containers), pass `region_name=…` to `boto3.client(...)` / `boto3.resource(...)`.

## Migration

### Breaking changes in 1.42.63 ⚠️
* **Python version support** – Boto3 no longer supports Python 3.8 (ended 2025‑04‑22) and will drop Python 3.9 support on 2026‑04‑29.  
  **Action:** Upgrade your runtime to Python 3.10 or newer and verify that any third‑party dependencies also support the newer interpreter.

### General migration guidance for **boto3 v1.42.63**
- **Session handling**: If you previously relied on global configuration, create an explicit `boto3.session.Session` and pass it to `client`/`resource` to avoid accidental cross‑region leakage.
- **Pagination**: Many list‑type operations now return truncated results by default. Switch to the paginator pattern (`client.get_paginator('operation_name')`) described in the *Paginator* guide.
- **Error handling**: The SDK now surfaces service‑specific error codes via `ClientError` more consistently. Update `except` blocks to check `e.response['Error']['Code']` rather than parsing exception strings.
- **Deprecations**: Review the *Migrations* section of the developer guide – it lists removed convenience methods (e.g., `boto.s3.connection`) and the preferred replacements.
- **Testing**: The test suite now expects `tox` to be invoked with the `-e` flag for multiple Python versions; adjust CI pipelines accordingly.

For a detailed step‑by‑step upgrade, see the `docs/source/guide/migration.rst` file in the repository.

## Pitfalls

### Wrong: Assuming region is configured for client creation and calls
```python
import boto3

s3 = boto3.client("s3")
resp = s3.list_buckets()
print(resp)
```

### Right: Set region explicitly when needed (or configure in `~/.aws/config`)
```python
import boto3

s3 = boto3.client("s3", region_name="us-east-1")
resp = s3.list_buckets()
print(resp)
```

### Wrong: Catching `Exception` hides real AWS failures
```python
import boto3

s3 = boto3.client("s3", region_name="us-east-1")
bucket_name = "amzn-s3-demo-bucket"

try:
    rules = s3.get_bucket_cors(Bucket=bucket_name)["CORSRules"]
except Exception:
    rules = []
print(rules)
```

### Right: Catch `ClientError` and branch on AWS error code
```python
import logging

import boto3
from botocore.exceptions import ClientError

logging.basicConfig(level=logging.INFO)

s3 = boto3.client("s3", region_name="us-east-1")
bucket_name = "amzn-s3-demo-bucket"

try:
    rules = s3.get_bucket_cors(Bucket=bucket_name)["CORSRules"]
except ClientError as e:
    if e.response["Error"]["Code"] == "NoSuchCORSConfiguration":
        rules = []
    else:
        logging.error("Unexpected AWS error: %s", e)
        raise

print(rules)
```

### Wrong: Using the wrong S3 CORS request shape (parameter validation error)
```python
import boto3

s3 = boto3.client("s3", region_name="us-east-1")

cors_configuration = {
    "Rules": [  # wrong key; should be "CORSRules"
        {"AllowedMethods": ["GET"], "AllowedOrigins": ["*"]}
    ]
}

s3.put_bucket_cors(Bucket="amzn-s3-demo-bucket", CORSConfiguration=cors_configuration)
```

### Right: Use `CORSConfiguration={'CORSRules': [...]}` with correct keys
```python
import boto3

s3 = boto3.client("s3", region_name="us-east-1")

cors_configuration = {
    "CORSRules": [
        {
            "AllowedHeaders": ["Authorization"],
            "AllowedMethods": ["GET", "PUT"],
            "AllowedOrigins": ["*"],
            "ExposeHeaders": ["ETag", "x-amz-request-id"],
            "MaxAgeSeconds": 3000,
        }
    ]
}

s3.put_bucket_cors(Bucket="amzn-s3-demo-bucket", CORSConfiguration=cors_configuration)
```

### Wrong: Assuming SES validates HTML template content
```python
import boto3

ses = boto3.client("ses", region_name="us-east-1")

ses.create_template(
    Template={
        "TemplateName": "TEMPLATE_NAME",
        "SubjectPart": "SUBJECT",
        "TextPart": "TEXT",
        "HtmlPart": "<html><body><p>unclosed tags",
    }
)
```

### Right: Validate/produce well‑formed HTML before calling SES
```python
import boto3

ses = boto3.client("ses", region_name="us-east-1")

valid_html = "<html><body><p>Hello</p></body></html>"

ses.create_template(
    Template={
        "TemplateName": "TEMPLATE_NAME",
        "SubjectPart": "SUBJECT_LINE",
        "TextPart": "TEXT_CONTENT",
        "HtmlPart": valid_html,
    }
)
```

### Wrong: Not handling `AccessDenied` when loading bucket metadata
```python
import boto3

s3 = boto3.resource("s3", region_name="us-east-1")
bucket = s3.Bucket("my-bucket")
bucket.load()  # May raise ClientError if access denied
print(bucket.creation_date)
```

### Right: Handle `AccessDenied` gracefully
```python
import boto3
from botocore.exceptions import ClientError

s3 = boto3.resource("s3", region_name="us-east-1")
bucket = s3.Bucket("my-bucket")

try:
    bucket.load()
    print(f"Bucket created: {bucket.creation_date}")
except ClientError as e:
    if e.response["Error"]["Code"] == "AccessDenied":
        print("Access denied to bucket metadata")
    else:
        raise
```

### Wrong: Using threading with a file opened in append mode
```python
import boto3
from boto3.s3.transfer import TransferConfig

s3 = boto3.client("s3", region_name="us-east-1")
config = TransferConfig(use_threads=True)

# Opening file in append mode
with open("file.txt", "ab") as f:
    s3.download_fileobj("my-bucket", "key", f, Config=config)
```

### Right: Disable threading for append‑mode operations
```python
import boto3
from boto3.s3.transfer import TransferConfig

s3 = boto3.client("s3", region_name="us-east-1")
config = TransferConfig(use_threads=True)

# Disable threading for append mode
with open("file.txt", "ab") as f:
    config.use_threads = False  # boto3.inject.disable_threading_if_append_mode does this internally
    s3.download_fileobj("my-bucket", "key", f, Config=config)
```

## References

- Documentation: https://boto3.amazonaws.com/v1/documentation/api/latest/index.html
- Source: https://github.com/boto/boto3