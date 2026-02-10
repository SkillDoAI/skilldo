---

name: sqlalchemy
description: Python SQL toolkit and ORM for defining schemas and issuing SQL/ORM queries with explicit transaction control.
version: 2.0
ecosystem: python
license: MIT
generated_with: gpt-5.2
---

## Imports

```python
import sqlalchemy
from sqlalchemy import (
    create_engine,
    text,
    select,
    insert,
    update,
    delete,
    bindparam,
    ForeignKey,
    Integer,
    String,
)
from sqlalchemy.orm import (
    DeclarativeBase,
    Mapped,
    mapped_column,
    relationship,
    Session,
    sessionmaker,
)
```

## Core Patterns

### Engine + explicit transactions (Core) ✅ Current
```python
from __future__ import annotations

from sqlalchemy import create_engine, text

def main() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:", future=True)

    with engine.begin() as conn:
        conn.execute(text("CREATE TABLE user_account (id INTEGER PRIMARY KEY, name TEXT NOT NULL)"))
        conn.execute(
            text("INSERT INTO user_account (name) VALUES (:name)"),
            [{"name": "alice"}, {"name": "bob"}],
        )

    with engine.connect() as conn:
        rows = conn.execute(text("SELECT id, name FROM user_account ORDER BY id")).all()
        print(rows)

if __name__ == "__main__":
    main()
```
* Use `engine.begin()` for an explicit transaction boundary (commit on success, rollback on error).
* Use `text()` with bound parameters (e.g., `:name`) instead of string interpolation.

### SQL Expression Language CRUD with bound parameters ✅ Current
```python
from __future__ import annotations

from sqlalchemy import create_engine, Integer, String, bindparam, select, insert, update, delete
from sqlalchemy import MetaData, Table, Column

def main() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
    metadata = MetaData()

    user_account = Table(
        "user_account",
        metadata,
        Column("id", Integer, primary_key=True),
        Column("name", String, nullable=False),
    )

    metadata.create_all(engine)

    with engine.begin() as conn:
        conn.execute(insert(user_account), [{"name": "alice"}, {"name": "bob"}])

        # UPDATE with bound parameters (safe + cacheable)
        conn.execute(
            update(user_account)
            .where(user_account.c.name == bindparam("old_name"))
            .values(name=bindparam("new_name")),
            {"old_name": "bob", "new_name": "robert"},
        )

        # SELECT
        names = conn.execute(select(user_account.c.id, user_account.c.name).order_by(user_account.c.id)).all()
        print(names)

        # DELETE
        conn.execute(delete(user_account).where(user_account.c.name == "alice"))

    with engine.connect() as conn:
        remaining = conn.execute(select(user_account.c.name).order_by(user_account.c.name)).scalars().all()
        print(remaining)

if __name__ == "__main__":
    main()
```
* Prefer `select()/insert()/update()/delete()` over handwritten SQL when practical.
* Use `bindparam()` for explicit parameter binding in reusable statements.

### Declarative ORM models + relationships ✅ Current
```python
from __future__ import annotations

from typing import List

from sqlalchemy import create_engine, ForeignKey, String
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column, relationship

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user_account"

    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(50), nullable=False)

    addresses: Mapped[List["Address"]] = relationship(back_populates="user", cascade="all, delete-orphan")

class Address(Base):
    __tablename__ = "address"

    id: Mapped[int] = mapped_column(primary_key=True)
    email: Mapped[str] = mapped_column(String(255), nullable=False)
    user_id: Mapped[int] = mapped_column(ForeignKey("user_account.id"), nullable=False)

    user: Mapped[User] = relationship(back_populates="addresses")

def main() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
    Base.metadata.create_all(engine)

    with Session(engine) as session:
        u = User(name="alice", addresses=[Address(email="alice@example.com")])
        session.add(u)
        session.commit()

    with Session(engine) as session:
        users = session.execute(
            sqlalchemy.select(User).order_by(User.id)  # type: ignore[attr-defined]
        ).scalars().all()
        print([(user.id, user.name, [a.email for a in user.addresses]) for user in users])

if __name__ == "__main__":
    main()
```
* Use `DeclarativeBase`, `Mapped[...]`, and `mapped_column()` for SQLAlchemy 2.0-style typed ORM mappings.
* Model relationships explicitly with `relationship()` and `ForeignKey()`.

### ORM unit-of-work with explicit commit/rollback ✅ Current
```python
from __future__ import annotations

from sqlalchemy import create_engine, select, String
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user_account"
    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(50), nullable=False)

def main() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
    Base.metadata.create_all(engine)

    # Write transaction
    with Session(engine) as session:
        session.add_all([User(name="alice"), User(name="bob")])
        session.commit()

    # Read-only pattern (no commit needed)
    with Session(engine) as session:
        names = session.execute(select(User.name).order_by(User.name)).scalars().all()
        print(names)

if __name__ == "__main__":
    main()
```
* ORM changes are not durable until `Session.commit()` succeeds; structure code around clear unit-of-work boundaries.
* Use `Session(...)` as a context manager to ensure resources are released.

## Configuration

- **Database URL**: pass to `create_engine()` (sync) as `"dialect+driver://user:pass@host/dbname"`.
  - SQLite in-memory: `"sqlite+pysqlite:///:memory:"`
- **Connection pooling**: configured via `create_engine()` kwargs (e.g., `pool_size`, `max_overflow`, `pool_pre_ping`).
- **Echo / SQL logging**: `create_engine(..., echo=True)` to log SQL emitted by SQLAlchemy.
- **Session configuration**:
  - Create ad-hoc sessions with `Session(engine)`.
  - Or create a factory with `sessionmaker(bind=engine)` for application-wide reuse.
- **Transactions**:
  - Core: prefer `with engine.begin() as conn: ...`
  - ORM: prefer `with Session(engine) as session: ...; session.commit()`
- **Parameter binding**: always use bound parameters (`text("... :name")`, `bindparam("name")`) rather than interpolating literals into SQL strings.

## Pitfalls

### Wrong: assuming ORM `add()` persists without `commit()`
```python
from __future__ import annotations

from sqlalchemy import create_engine, String
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user_account"
    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(50), nullable=False)

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
Base.metadata.create_all(engine)

session = Session(engine)
session.add(User(name="alice"))
session.close()  # closes without commit; transaction is rolled back
```

### Right: commit within a clear unit of work
```python
from __future__ import annotations

from sqlalchemy import create_engine, String
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user_account"
    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(50), nullable=False)

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
Base.metadata.create_all(engine)

with Session(engine) as session:
    session.add(User(name="alice"))
    session.commit()
```

### Wrong: SQL injection via string interpolation with `exec_driver_sql()`
```python
from __future__ import annotations

from sqlalchemy import create_engine

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)

name = "alice' OR 1=1 --"
with engine.connect() as conn:
    conn.exec_driver_sql(f"SELECT '{name}'")  # interpolated SQL; unsafe pattern
```

### Right: use `text()` + bound parameters
```python
from __future__ import annotations

from sqlalchemy import create_engine, text

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)

name = "alice' OR 1=1 --"
with engine.connect() as conn:
    value = conn.execute(text("SELECT :name"), {"name": name}).scalar_one()
    print(value)
```

### Wrong: forgetting to wrap Core writes in a transaction
```python
from __future__ import annotations

from sqlalchemy import create_engine, text

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)

with engine.connect() as conn:
    conn.execute(text("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)"))
    conn.execute(text("INSERT INTO t (name) VALUES ('alice')"))
    # no commit; many DBAPIs will roll back when the connection closes
```

### Right: use `engine.begin()` for Core writes
```python
from __future__ import annotations

from sqlalchemy import create_engine, text

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)

with engine.begin() as conn:
    conn.execute(text("CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT)"))
    conn.execute(text("INSERT INTO t (name) VALUES (:name)"), {"name": "alice"})
```

### Wrong: selecting ORM entities but not using `.scalars()`
```python
from __future__ import annotations

from sqlalchemy import create_engine, select, String
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user_account"
    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(50), nullable=False)

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
Base.metadata.create_all(engine)

with Session(engine) as session:
    session.add_all([User(name="alice"), User(name="bob")])
    session.commit()

with Session(engine) as session:
    rows = session.execute(select(User)).all()
    # rows are Row objects containing User at index 0, not a list[User]
    users = [r for r in rows]
    print(users)
```

### Right: use `.scalars()` to get mapped instances
```python
from __future__ import annotations

from sqlalchemy import create_engine, select, String
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user_account"
    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(50), nullable=False)

engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
Base.metadata.create_all(engine)

with Session(engine) as session:
    session.add_all([User(name="alice"), User(name="bob")])
    session.commit()

with Session(engine) as session:
    users = session.execute(select(User).order_by(User.id)).scalars().all()
    print([u.name for u in users])
```

## References

- [Homepage](https://www.sqlalchemy.org)
- [Documentation](https://docs.sqlalchemy.org)
- [Changelog](https://docs.sqlalchemy.org/latest/changelog/index.html)
- [Source Code](https://github.com/sqlalchemy/sqlalchemy)
- [Issue Tracker](https://github.com/sqlalchemy/sqlalchemy/issues)
- [Discussions](https://github.com/sqlalchemy/sqlalchemy/discussions)

## Migration from v1.4

- SQLAlchemy 2.0 formalizes “2.0 style” usage that was available in 1.4:
  - Prefer `select()` constructs and `Session.execute(select(...))` over legacy `Query` patterns.
  - Prefer explicit transaction scopes: `engine.begin()` (Core) and `Session(...); commit()` (ORM).
  - Prefer typed ORM mappings: `DeclarativeBase`, `Mapped[...]`, `mapped_column()`.

### Legacy ORM query style ⚠️ Soft Deprecation
* Deprecated since: 2.0 (legacy ORM `Query` patterns are considered legacy in 2.0-style code)
* Still works: True (in many configurations), but prefer 2.0 style for new code
* Modern alternative: `Session.execute(select(...)).scalars()`
* Migration guidance: replace `session.query(Model).filter(...)` with `session.execute(select(Model).where(...)).scalars()`

```python
from __future__ import annotations

from sqlalchemy import create_engine, select, String
from sqlalchemy.orm import DeclarativeBase, Mapped, Session, mapped_column

class Base(DeclarativeBase):
    pass

class User(Base):
    __tablename__ = "user_account"
    id: Mapped[int] = mapped_column(primary_key=True)
    name: Mapped[str] = mapped_column(String(50), nullable=False)

def main() -> None:
    engine = create_engine("sqlite+pysqlite:///:memory:", future=True)
    Base.metadata.create_all(engine)

    with Session(engine) as session:
        session.add_all([User(name="alice"), User(name="bob")])
        session.commit()

    with Session(engine) as session:
        # 2.0 style:
        users = session.execute(select(User).where(User.name == "alice")).scalars().all()
        print([u.name for u in users])

if __name__ == "__main__":
    main()
```

## API Reference

- **sqlalchemy.create_engine(url, \*\*kwargs)** - Create an `Engine`; key kwargs include `echo`, pool options, and dialect/driver URL.
- **sqlalchemy.Engine.connect()** - Acquire a `Connection` for SQL execution (explicit transaction management required for writes).
- **sqlalchemy.Engine.begin()** - Context manager that provides a `Connection` with an explicit transaction (commit/rollback).
- **sqlalchemy.text(sql_text)** - Create a textual SQL statement supporting bound parameters (`:name`).
- **sqlalchemy.select(\*entities)** - Build a SELECT statement from tables/columns/ORM entities.
- **sqlalchemy.insert(table)** - Build an INSERT statement.
- **sqlalchemy.update(table)** - Build an UPDATE statement.
- **sqlalchemy.delete(table)** - Build a DELETE statement.
- **sqlalchemy.bindparam(name)** - Define an explicit bound parameter for SQL constructs.
- **sqlalchemy.orm.Session(bind=engine)** - ORM session (unit of work / identity map); use `commit()` to persist.
- **sqlalchemy.orm.sessionmaker(bind=engine, \*\*kwargs)** - Factory for creating configured `Session` objects.
- **sqlalchemy.orm.DeclarativeBase** - Base class for declarative ORM mappings (2.0 style).
- **sqlalchemy.orm.Mapped[T]** - Typing annotation used for ORM-mapped attributes.
- **sqlalchemy.orm.mapped_column(\*\*kwargs)** - Declare an ORM-mapped column with typing support.
- **sqlalchemy.orm.relationship(\*\*kwargs)** - Define ORM relationships between mapped classes.