---
name: pydantic
description: A data validation and settings management library for Python.
version: 2.12.5
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
from pydantic import BaseModel, Field, ValidationError, computed_field, field_validator
```

## Core Patterns

### Create a Model ✅ Current
```python
from pydantic import BaseModel

class User(BaseModel):
    id: int
    name: str
```
* Defines a data model with `id` and `name` attributes.
* **Status**: Current, stable

### Field with Validation ✅ Current
```python
from pydantic import BaseModel, Field

class Product(BaseModel):
    name: str
    price: float = Field(gt=0)  # price must be greater than 0
```
* Uses `Field` to enforce validation rules on attributes.
* **Status**: Current, stable

### Field Validator ✅ Current
```python
from pydantic import BaseModel, field_validator

class Model(BaseModel):
    a: str

    @field_validator('a')
    def check_a(cls, value):
        if value != 'a':
            raise ValueError('a must be "a"')
        return value
```
* Validates the field `a` to ensure it meets criteria.
* **Status**: Current, stable

### Computed Field ✅ Current
```python
from pydantic import BaseModel, computed_field

class Rectangle(BaseModel):
    width: float
    length: float

    @computed_field
    def area(self) -> float:
        return self.width * self.length
```
* Defines a computed property `area` based on other attributes.
* **Status**: Current, stable

### Model Dumping ✅ Current
```python
from pydantic import BaseModel

class Model(BaseModel):
    x: int
    y: int

m = Model(x=1, y=2)
print(m.model_dump())  # Output: {'x': 1, 'y': 2}
```
* Serializes the model instance to a dictionary format.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Use `Field` for default values and validations.
- Example:
```python
from pydantic import BaseModel, Field

class ConfiguredModel(BaseModel):
    name: str = Field(default="Default Name", title="The name of the user")
```

## Pitfalls

### Wrong: Mutable Defaults
```python
class User(BaseModel):
    friends: list[int] = []  # This will share state across instances
```

### Right: Use Field for Mutable Defaults
```python
from pydantic import BaseModel, Field

class User(BaseModel):
    friends: list[int] = Field(default_factory=list)  # Unique for each instance
```

### Wrong: Missing Await on Async Call
```python
async def fetch_data():
    response = httpx.get('https://api.example.com')  # Missing await
```

### Right: Properly Await Async Calls
```python
import httpx

async def fetch_data():
    async with httpx.AsyncClient() as client:
        response = await client.get('https://api.example.com')  # Correctly awaited
```

### Wrong: Using json_encoders in Model Config
```python
class Model(BaseModel):
    class Config:
        json_encoders = {datetime: lambda v: v.isoformat()}  # Deprecated
```

### Right: Use @field_serializer for Custom Serialization
```python
from pydantic import BaseModel, field_serializer

class Model(BaseModel):
    value: datetime

    @field_serializer('value')
    def serialize_value(cls, v):
        return v.isoformat()  # Use the new serialization method
```

## References

- [Homepage](https://github.com/pydantic/pydantic)
- [Documentation](https://docs.pydantic.dev)
- [Funding](https://github.com/sponsors/samuelcolvin)
- [Source](https://github.com/pydantic/pydantic)
- [Changelog](https://docs.pydantic.dev/latest/changelog/)

## Migration from v1.10.0

What changed in this version:
- **Breaking changes**: API signature changes for BaseModel methods. Update method calls to use new names like `model_dump()` instead of `dict()`.
- **Deprecated**: Usage of `dict()`, `json()`, and `parse_obj()` will emit warnings. Prefer `model_dump()` and `model_validate()`.
- **Removed**: The `from_orm` method has been removed; use `model_validate()` with `from_attributes=True` instead.

## API Reference

- **BaseModel()** - Base class for creating data models with validation.
- **Field()** - Function to define fields with metadata and validation.
- **field_validator()** - Decorator to validate fields in models.
- **computed_field()** - Decorator to define computed properties.
- **model_dump()** - Method to serialize model instances to dictionaries.