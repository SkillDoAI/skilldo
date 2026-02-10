---

name: boto3
description: AWS SDK for Python for creating service clients/resources and calling AWS APIs.
version: 1.42.44
ecosystem: python
license: Apache-2.0
generated_with: gpt-5.2
---

## Imports

```python
import boto3
from botocore.exceptions import ClientError
```

## Core Patterns

### Create a low-level service client ✅ Current
```python
import boto3

def make_s3_client(region_name: str = "us-east-1"):
    # Prefer credentials/region from ~/.aws/{credentials,config};
    # pass region_name explicitly when you need deterministic behavior.
    return boto3.client("s3", region_name=region_name)

if __name__ == "__main__":
    s3 = make_s3_client()
    # This call is safe to run; it only constructs the client.
    print(f"Created client: {s3.meta.service_model.service_name}")
```
* Use `boto3.client(service_name, region_name=...)` for explicit API operations (e.g., `get_bucket_cors`, `put_bucket_cors`, SES/EC2 operations).

### Create a high-level service resource ✅ Current
```python
import boto3

def make_s3_resource(region_name: str = "us-east-1"):
    return boto3.resource("s3", region_name=region_name)

if __name__ == "__main__":
    s3 = make_s3_resource()
    # Resource objects provide an OO interface; operations require AWS access when called.
    print(f"Created resource: {s3.meta.service_name}")
```
* Use `boto3.resource(service_name, ...)` when you want object-oriented access patterns.

### Handle AWS errors with ClientError and error codes ✅ Current
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
        raise  # re-raise unexpected failures (permissions, missing bucket, etc.)

if __name__ == "__main__":
    # Example usage (will make a network call if executed with real bucket name)
    bucket = "amzn-s3-demo-bucket"
    try:
        rules = get_bucket_cors_rules(bucket)
        logging.info("CORS rules: %s", rules)
    except ClientError as e:
        logging.error("AWS error: %s", e)
```
* Catch `botocore.exceptions.ClientError` (not `Exception`) and branch on `e.response['Error']['Code']` for expected conditions.

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
* The top-level key must be `CORSRules` inside `CORSConfiguration=...`.

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

## Configuration

- Preferred configuration is via shared config files (avoid hardcoding credentials):
  - `~/.aws/credentials` (profiles, access keys)
  - `~/.aws/config` (default region/output, named profiles)
- Common environment variables (standard AWS SDK behavior):
  - `AWS_PROFILE` (select profile)
  - `AWS_REGION` / `AWS_DEFAULT_REGION` (default region)
  - `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN` (temporary creds)
- When region must be deterministic (CI, containers), pass `region_name=...` to `boto3.client(...)` / `boto3.resource(...)`.

## Pitfalls

### Wrong: Assuming region is configured for client creation and calls
```python
import boto3

s3 = boto3.client("s3")
resp = s3.list_buckets()
print(resp)
```

### Right: Set region explicitly when needed (or configure in ~/.aws/config)
```python
import boto3

s3 = boto3.client("s3", region_name="us-east-1")
resp = s3.list_buckets()
print(resp)
```

### Wrong: Catching Exception hides real AWS failures
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

### Right: Catch ClientError and branch on AWS error code
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

### Right: Validate/produce well-formed HTML before calling SES
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

## References

- Documentation: https://boto3.amazonaws.com/v1/documentation/api/latest/index.html
- Source: https://github.com/boto/boto3

## Migration from v[previous]

- No breaking changes were present in the provided documentation/changelog excerpt for v1.42.44.
- Runtime support policy note from provided excerpt:
  - Python 3.8 support ended on 2025-04-22 (upgrade off 3.8).
  - Python 3.9 support ends on 2026-04-29 (plan upgrade before that date).

## API Reference

- **boto3.client(service_name, region_name=...)** - Create a low-level service client for explicit AWS API operations.
- **boto3.resource(service_name, region_name=...)** - Create a high-level resource interface (OO-style) for supported services.
- **EC2.Client.describe_addresses()** - Describe Elastic IP addresses allocated to the account (supports filters/queries via parameters).
- **EC2.Client.allocate_address()** - Allocate a new Elastic IP address.
- **EC2.Client.associate_address()** - Associate an Elastic IP address with an instance or network interface.
- **EC2.Client.release_address()** - Release an Elastic IP address back to AWS.
- **S3.Client.get_bucket_cors(Bucket=...)** - Fetch a bucket’s CORS rules; may raise `ClientError` with `NoSuchCORSConfiguration`.
- **S3.Client.put_bucket_cors(Bucket=..., CORSConfiguration=...)** - Set a bucket’s CORS rules (must use `CORSRules` key).
- **SES.Client.create_template(Template=...)** - Create an email template (SES does not validate HTML).
- **SES.Client.list_templates()** - List template metadata (names).
- **SES.Client.get_template(TemplateName=...)** - Retrieve a template’s full content.
- **SES.Client.update_template(Template=...)** - Update an existing template.
- **SES.Client.delete_template(TemplateName=...)** - Delete a template.
- **SES.Client.send_templated_email(Source=..., Destination=..., Template=..., TemplateData=...)** - Send an email using a stored template.
- **botocore.exceptions.ClientError** - Exception type for AWS service API errors; inspect `e.response['Error']['Code']`.