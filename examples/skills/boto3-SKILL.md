---
name: boto3
description: AWS SDK for Python for creating service clients/resources and calling AWS APIs.
version: 1.42.44
ecosystem: python
license: Apache-2.0
---

## Imports

Show the standard import patterns. Most common first:

```python
import boto3
from botocore.exceptions import ClientError
```

## Core Patterns

**CRITICAL: Prioritize PUBLIC APIs over internal/compat modules**

- Use APIs from api_surface with `publicity_score: "high"` first
- Avoid `.compat`, `.internal`, `._private` modules unless they're the only option
- Example: Prefer `library.MainClass` over `library.compat.helper_function`

**CRITICAL: Mark deprecation status with clear indicators**

### Create a low-level service client ✅ Current

```python
import boto3

def make_s3_client():
    # Uses the standard AWS credential/region resolution chain
    # (env vars, shared config files, IAM role, etc.)
    return boto3.client("s3")

def make_ec2_client():
    return boto3.client("ec2")
```

- Creates a low-level client for direct access to AWS API operations.
- **Status**: Current, stable

### Create a high-level resource ✅ Current

```python
import boto3

def list_s3_buckets() -> list[str]:
    # Resource interface provides object-oriented access patterns.
    s3 = boto3.resource("s3")
    return [bucket.name for bucket in s3.buckets.all()]
```

- Uses the resource interface when you want higher-level, object-oriented access.
- **Status**: Current, stable

### Handle `ClientError` and branch on AWS error codes ✅ Current

```python
import logging
import boto3
from botocore.exceptions import ClientError

logger = logging.getLogger(__name__)

def get_bucket_cors_rules(bucket_name: str) -> list[dict] | None:
    s3 = boto3.client("s3")
    try:
        response = s3.get_bucket_cors(Bucket=bucket_name)
    except ClientError as e:
        code = e.response["Error"]["Code"]
        if code == "NoSuchCORSConfiguration":
            # Valid "not configured" state
            return []
        logger.error("Failed to get CORS for bucket %s: %s", bucket_name, e)
        return None
    return response["CORSRules"]
```

- Catches `botocore.exceptions.ClientError` and inspects `e.response['Error']['Code']`.
- **Status**: Current, stable

### Put S3 bucket CORS configuration ✅ Current

```python
import boto3

def put_bucket_cors(bucket_name: str) -> None:
    s3 = boto3.client("s3")

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

    s3.put_bucket_cors(
        Bucket=bucket_name,
        CORSConfiguration=cors_configuration,
    )
```

- Sets a bucket’s CORS configuration using the modeled request shape.
- **Status**: Current, stable

### Manage SES email templates and send a templated email ✅ Current

```python
import boto3
from botocore.exceptions import ClientError

def create_or_update_template(template_name: str) -> None:
    ses = boto3.client("ses")

    template = {
        "TemplateName": template_name,
        "SubjectPart": "Hello, {{name}}",
        "TextPart": "Hi {{name}}, your code is {{code}}.",
        "HtmlPart": "<p>Hi {{name}}, your code is <b>{{code}}</b>.</p>",
    }

    try:
        # Create first; if it already exists, update.
        ses.create_template(Template=template)
    except ClientError as e:
        if e.response["Error"]["Code"] in {"TemplateNameAlreadyExists", "AlreadyExists"}:
            ses.update_template(Template=template)
        else:
            raise

def send_template_email(
    template_name: str,
    source_email: str,
    to_email: str,
) -> str:
    ses = boto3.client("ses")
    response = ses.send_templated_email(
        Source=source_email,
        Destination={"ToAddresses": [to_email]},
        Template=template_name,
        TemplateData='{"name":"Alice","code":"123456"}',
    )
    return response["MessageId"]
```

- Uses SES template lifecycle APIs and `send_templated_email`.
- **Status**: Current, stable

## Configuration

Standard configuration and setup:

- **Recommended credential/region configuration**: shared config files
  - `~/.aws/credentials`
    - Example:
      - `[default]`
      - `aws_access_key_id = ...`
      - `aws_secret_access_key = ...`
  - `~/.aws/config`
    - Example:
      - `[default]`
      - `region = us-east-1`
- **In code**: prefer `boto3.client("s3")` / `boto3.resource("s3")` without hardcoded keys.
- **Local development**: use a virtual environment:
  - `python -m venv .venv`
  - activate it, then `pip install boto3`

## Pitfalls

CRITICAL: This section is MANDATORY. Show 3-5 common mistakes with specific Wrong/Right examples.

### Wrong: Hardcoding AWS credentials in code (bypasses default credential chain)

```python
import boto3

s3 = boto3.client(
    "s3",
    aws_access_key_id="AKIA...",
    aws_secret_access_key="SECRET...",
    region_name="us-east-1",
)
```

### Right: Use shared config/credentials files and default resolution chain

```python
# ~/.aws/credentials
# [default]
# aws_access_key_id = ...
# aws_secret_access_key = ...
#
# ~/.aws/config
# [default]
# region = us-east-1

import boto3

s3 = boto3.client("s3")
```

### Wrong: Treating all `ClientError` exceptions as “not configured”

```python
import boto3
from botocore.exceptions import ClientError

def get_bucket_cors(bucket_name: str) -> list[dict]:
    s3 = boto3.client("s3")
    try:
        return s3.get_bucket_cors(Bucket=bucket_name)["CORSRules"]
    except ClientError:
        # Incorrect: AccessDenied / NoSuchBucket / etc. are not "no config"
        return []
```

### Right: Branch on `e.response['Error']['Code']` for service-specific handling

```python
import logging
import boto3
from botocore.exceptions import ClientError

logger = logging.getLogger(__name__)

def get_bucket_cors(bucket_name: str) -> list[dict] | None:
    s3 = boto3.client("s3")
    try:
        response = s3.get_bucket_cors(Bucket=bucket_name)
    except ClientError as e:
        if e.response["Error"]["Code"] == "NoSuchCORSConfiguration":
            return []
        logger.error("Unexpected error getting CORS: %s", e)
        return None
    return response["CORSRules"]
```

### Wrong: Incorrect request parameter shape/casing for `put_bucket_cors`

```python
import boto3

s3 = boto3.client("s3")

cors_configuration = {
    "corsrules": [  # wrong key name
        {
            "allowedmethods": ["GET"],  # wrong casing
            "allowedorigins": ["*"],    # wrong casing
        }
    ]
}

s3.put_bucket_cors(Bucket="amzn-s3-demo-bucket", CORSConfiguration=cors_configuration)
```

### Right: Match the modeled AWS API shape exactly

```python
import boto3

s3 = boto3.client("s3")

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

s3.put_bucket_cors(
    Bucket="amzn-s3-demo-bucket",
    CORSConfiguration=cors_configuration,
)
```

### Wrong: Ignoring AWS API errors when managing EC2 Elastic IPs

```python
import boto3

def allocate_and_associate(instance_id: str) -> None:
    ec2 = boto3.client("ec2")
    alloc = ec2.allocate_address(Domain="vpc")
    # If this fails (e.g., InvalidInstanceID.NotFound), it will raise and you may leak the EIP.
    ec2.associate_address(InstanceId=instance_id, AllocationId=alloc["AllocationId"])
```

### Right: Use `ClientError` handling and cleanup on failure

```python
import boto3
from botocore.exceptions import ClientError

def allocate_and_associate(instance_id: str) -> str:
    ec2 = boto3.client("ec2")
    alloc = ec2.allocate_address(Domain="vpc")
    allocation_id = alloc["AllocationId"]

    try:
        ec2.associate_address(InstanceId=instance_id, AllocationId=allocation_id)
    except ClientError:
        # Best-effort cleanup to avoid leaking the allocated address
        ec2.release_address(AllocationId=allocation_id)
        raise

    return allocation_id
```

## References

CRITICAL: Include ALL provided URLs below (do NOT skip this section):

- Documentation: https://boto3.amazonaws.com/v1/documentation/api/latest/index.html
- Source: https://github.com/boto/boto3

## Migration from v1.42.43

What changed in this version (if applicable):

- **Breaking changes**: None indicated in the provided excerpts for v1.42.44.
- **Deprecated → Current mapping**: No API-level deprecations/migrations provided.
- **Runtime support note**: Python 3.8 support ended 2025-04-22; Python 3.9 support scheduled to end 2026-04-29. Plan runtime upgrades accordingly.

## API Reference

Brief reference of the most important public APIs:

- **boto3.client(service_name, ...)** - Create a low-level client for a service (e.g., `"s3"`, `"ec2"`, `"ses"`).
- **boto3.resource(service_name, ...)** - Create a high-level resource interface (e.g., `"s3"`).

- **EC2.Client.describe_addresses(...)** - Describe Elastic IP addresses (filters/AllocationIds/PublicIps depending on use).
- **EC2.Client.allocate_address(Domain="vpc", ...)** - Allocate an Elastic IP address (commonly `Domain="vpc"`).
- **EC2.Client.associate_address(InstanceId=..., AllocationId=..., ...)** - Associate an Elastic IP to an instance/network interface.
- **EC2.Client.release_address(AllocationId=..., ...)** - Release an Elastic IP allocation.

- **S3.Client.get_bucket_cors(Bucket=...)** - Get bucket CORS rules; may raise `ClientError` with `NoSuchCORSConfiguration`.
- **S3.Client.put_bucket_cors(Bucket=..., CORSConfiguration=...)** - Set bucket CORS rules (must match modeled shape).

- **SES.Client.create_template(Template=...)** - Create an SES template.
- **SES.Client.list_templates(...)** - List templates (pagination may apply depending on usage).
- **SES.Client.get_template(TemplateName=...)** - Fetch a template definition.
- **SES.Client.update_template(Template=...)** - Update an existing template.
- **SES.Client.delete_template(TemplateName=...)** - Delete a template.
- **SES.Client.send_templated_email(Source=..., Destination=..., Template=..., TemplateData=...)** - Send an email using a stored template.

- **botocore.exceptions.ClientError** - Exception type for AWS API errors; inspect `e.response["Error"]["Code"]`.