---
name: sqlalchemy
description: SQLAlchemy is a SQL toolkit and Object-Relational Mapping (ORM) system for Python.
version: 2.0
ecosystem: python
license: MIT
---

## Imports

Show the standard import patterns. Most common first:
```python
from sqlalchemy import create_engine, select
from sqlalchemy.orm import Session, relationship, mapped_column
```

## Core Patterns

### Create Engine ✅ Current
```python
from sqlalchemy import create_engine

# Create a new SQLite engine
engine = create_engine('sqlite://')
```
* This code initializes a new SQLAlchemy engine connected to a SQLite database.
* **Status**: Current, stable

### Define Relationships ✅ Current
```python
from sqlalchemy.orm import relationship

# Define relationship in a mapped class
class Order(Base):
    order_items: Mapped[list[OrderItem]] = relationship(cascade='all, delete-orphan', backref='order')
```
* This code establishes a one-to-many relationship between Order and OrderItem, allowing cascading operations.
* **Status**: Current, stable

### Mapped Column Definition ✅ Current
```python
from sqlalchemy.orm import mapped_column

# Define a mapped column in a class
class Order(Base):
    order_id: Mapped[int] = mapped_column(primary_key=True)
```
* This code defines a mapped column for the primary key in an ORM model.
* **Status**: Current, stable

### Session Management ✅ Current
```python
from sqlalchemy.orm import Session

# Using a session to interact with the database
with Session(engine) as session:
    # Add and commit an order
    session.add(order)
    session.commit()
```
* This pattern demonstrates how to manage a session context to ensure proper resource handling.
* **Status**: Current, stable

### Querying with Select ✅ Current
```python
from sqlalchemy import select

# Query to find an order by customer name
order = session.scalars(select(Order).filter_by(customer_name='john smith')).one()
```
* This code executes a query to retrieve a specific order based on the customer's name.
* **Status**: Current, stable

## Pitfalls

### Wrong: Mutable Default Arguments
```python
def add_order(order_list=[]):
    order_list.append(order)
```

### Right: Avoid Mutable Defaults
```python
def add_order(order_list=None):
    if order_list is None:
        order_list = []
    order_list.append(order)
```
* Mutable default arguments can lead to unexpected behavior as they persist across calls.

### Wrong: Session Management Outside Context
```python
session = Session()
return session.query(Order).all()
```

### Right: Use Context Manager
```python
with Session() as session:
    return session.query(Order).all()
```
* Session should be managed within a context manager to avoid leaks.

### Wrong: Committing a Session Without Await
```python
session.commit()
```

### Right: Await on Async Commit
```python
await session.commit()
```
* Async functions need await; missing it can lead to unhandled coroutines.

## References

- [Homepage](https://www.sqlalchemy.org)
- [Documentation](https://docs.sqlalchemy.org)
- [Changelog](https://docs.sqlalchemy.org/latest/changelog/index.html)
- [Source Code](https://github.com/sqlalchemy/sqlalchemy)
- [Issue Tracker](https://github.com/sqlalchemy/sqlalchemy/issues)
- [Discussions](https://github.com/sqlalchemy/sqlalchemy/discussions)

## Migration from v1.x

What changed in this version:
- The `declarative_base` function now requires an explicit metadata argument.
  - **Migration**: Change `Base = declarative_base()` to `Base = declarative_base(metadata=MetaData())`.
- The `session.commit()` method is now an async function.
  - **Migration**: Use `await session.commit()` instead of `session.commit()`.

## API Reference

- **create_engine(url: Union[str, URL], **kwargs)** - Creates a new SQLAlchemy engine.
- **Session(bind: Optional[Engine] = None, autoflush: bool = True, autocommit: bool = False, expire_on_commit: bool = True)** - ORM session for database operations.
- **relationship(...)** - Defines relationships between mapped classes.
- **mapped_column(...)** - Defines a mapped column in a class.
- **select(*entities: Union[FromClause, ColumnElement, str], **kwargs)** - Constructs a SELECT statement.
- **Session.add(instance: Any, _warn: bool = True) -> None** - Adds an instance to the session.
- **Session.commit() -> None** - Commits the current transaction.