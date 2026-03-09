---

name: testify
description: Testify is a Go testing toolkit providing assertion functions, mock objects, and test suite scaffolding to simplify writing unit tests.
license: MIT
metadata:
  version: "1.10.0"
  ecosystem: go

## Imports

```go
import (
    "testing"

    "github.com/stretchr/testify/assert"  // non-fatal assertions
    "github.com/stretchr/testify/require" // fatal assertions (stops test on failure)
    "github.com/stretchr/testify/mock"    // mock objects
    "github.com/stretchr/testify/suite"   // test suites
)
```

## Core Patterns

### Standalone Assertions ✅ Current

Use package-level functions from `assert` or `require`. Every function takes `t *testing.T` as its first argument and returns `bool`.

```go
package mypackage_test

import (
    "errors"
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/require"
)

func TestUserCreation(t *testing.T) {
    user, err := CreateUser("alice")

    // require stops the test immediately on failure; use it when
    // subsequent assertions would panic on a nil/zero value.
    require.NoError(t, err, "CreateUser should not return an error")
    require.NotNil(t, user)

    assert.Equal(t, "alice", user.Name)
    assert.NotEmpty(t, user.ID)
    assert.True(t, user.Active, "new user should be active")

    // Guard further assertions when a nil dereference is possible.
    if assert.NotNil(t, user.Profile) {
        assert.Equal(t, "alice", user.Profile.DisplayName)
    }
}

func TestErrorHandling(t *testing.T) {
    _, err := CreateUser("")
    assert.Error(t, err)
    assert.ErrorIs(t, err, ErrInvalidName)
    assert.ErrorContains(t, err, "name")
}
```

### Bound Assertions Object ✅ Current

Use `assert.New(t)` to create an `*Assertions` object when making many assertions, avoiding repeated `t` arguments.

```go
package mypackage_test

import (
    "testing"

    "github.com/stretchr/testify/assert"
)

func TestOrderProcessing(t *testing.T) {
    a := assert.New(t)

    order := ProcessOrder(42)

    a.NotNil(order)
    a.Equal(42, order.ID)
    a.Equal("pending", order.Status)
    a.Greater(order.Total, 0.0)
    a.Len(order.Items, 3)
}
```

### Mock Objects ✅ Current

Embed `mock.Mock` in a struct that implements the target interface. Each method calls `m.Called(args...)` and extracts typed return values. Call `AssertExpectations` at the end of the test.

```go
package mypackage_test

import (
    "testing"

    "github.com/stretchr/testify/assert"
    "github.com/stretchr/testify/mock"
)

// EmailSender is the interface under test.
type EmailSender interface {
    Send(to, subject, body string) error
}

// MockEmailSender is the mock implementation.
type MockEmailSender struct {
    mock.Mock
}

func (m *MockEmailSender) Send(to, subject, body string) error {
    args := m.Called(to, subject, body)
    return args.Error(0)
}

func TestNotifyUser(t *testing.T) {
    sender := new(MockEmailSender)
    sender.On("Send", "alice@example.com", mock.Anything, mock.Anything).
        Return(nil)

    err := NotifyUser(sender, "alice@example.com")
    assert.NoError(t, err)

    sender.AssertExpectations(t)
}
```

### Test Suites ✅ Current

Embed `suite.Suite` in a struct. Methods starting with `Test` are run as test cases. A standard `TestXxx` wrapper function is required for `go test` discovery.

```go
package mypackage_test

import (
    "testing"

    "github.com/stretchr/testify/suite"
)

type InventorySuite struct {
    suite.Suite
    db *FakeDB
}

// SetupTest runs before each test method.
func (s *InventorySuite) SetupTest() {
    s.db = NewFakeDB()
}

// TearDownTest runs after each test method.
func (s *InventorySuite) TearDownTest() {
    s.db.Close()
}

func (s *InventorySuite) TestAddItem() {
    err := s.db.Add("widget", 10)
    s.NoError(err)
    count, err := s.db.Count("widget")
    s.NoError(err)
    s.Equal(10, count)
}

func (s *InventorySuite) TestRemoveItem() {
    _ = s.db.Add("gadget", 5)
    err := s.db.Remove("gadget", 3)
    s.NoError(err)
}

// Required: standard test function for go test discovery.
func TestInventorySuite(t *testing.T) {
    suite.Run(t, new(InventorySuite))
}
```

### Comparison and Collection Assertions ✅ Current

Testify provides ordering, collection membership, and numeric range assertions.

```go
package mypackage_test

import (
    "testing"
    "time"

    "github.com/stretchr/testify/assert"
)

func TestMetrics(t *testing.T) {
    scores := []int{10, 20, 30, 40}
    assert.IsIncreasing(t, scores)
    assert.IsNonDecreasing(t, scores)

    latencies := []float64{0.9, 0.8, 0.7}
    assert.IsDecreasing(t, latencies)

    assert.Contains(t, []string{"admin", "editor", "viewer"}, "editor")
    assert.NotContains(t, []string{"admin", "editor"}, "superuser")
    assert.ElementsMatch(t, []int{3, 1, 2}, []int{1, 2, 3})
    assert.Subset(t, []int{1, 2, 3, 4}, []int{2, 3})

    assert.InDelta(t, 3.14, 3.141592, 0.01, "pi within tolerance")
    assert.Positive(t, 42)
    assert.Negative(t, -1)

    now := time.Now()
    assert.WithinDuration(t, now, now.Add(time.Millisecond*50), time.Second)
}
```

## Configuration

### assert vs require

| Package | On failure | Use when |
|
  generated-by: skilldo/claude-sonnet-4-6 + review:gpt-5.3-codex, test:gpt-5.3-codex
---------|-----------|----------|
| `assert` | Marks test failed, continues | Collecting multiple failures |
| `require` | Calls `t.FailNow()`, stops test | Nil dereference risk or setup steps |

### Custom Failure Messages

All assertion functions accept optional `msgAndArgs ...interface{}` at the end, formatted with `fmt.Sprintf`.

```go
assert.Equal(t, 200, resp.StatusCode, "expected OK, got %d for path %s", resp.StatusCode, path)
```

### Sentinel Error Value

`assert.AnError` is a general-purpose non-nil error for use in tests that need an error but do not care about its content.

```go
import "github.com/stretchr/testify/assert"

// Use assert.AnError when the specific error value is irrelevant.
mockSvc.On("Fetch", 99).Return(nil, assert.AnError)
```

### Assertion Function Types

Testify defines function-type aliases for use in table-driven tests:

```go
package mypackage_test

import (
    "testing"

    "github.com/stretchr/testify/assert"
)

func TestValidation(t *testing.T) {
    tests := []struct {
        value   int
        checkFn assert.ValueAssertionFunc
    }{
        {1, assert.NotZero},
        {0, assert.Zero},
    }
    for _, tc := range tests {
        tc.checkFn(t, tc.value)
    }
}
```

## Pitfalls

### Using require Inside a Goroutine

**Wrong** — `require` calls `t.FailNow()` via `runtime.Goexit()`; calling it from a spawned goroutine panics instead of failing cleanly.

```go
func TestBad(t *testing.T) {
    go func() {
        result := compute()
        require.Equal(t, 42, result) // panics: Goexit called from wrong goroutine
    }()
}
```

**Right** — Use `assert` inside goroutines; synchronize before the test function returns.

```go
func TestGood(t *testing.T) {
    var wg sync.WaitGroup
    wg.Add(1)
    go func() {
        defer wg.Done()
        result := compute()
        assert.Equal(t, 42, result) // safe: marks failure without Goexit
    }()
    wg.Wait()
}
```

### Skipping AssertExpectations on Mocks

**Wrong** — Without `AssertExpectations`, a test passes even if expected mock calls were never made.

```go
func TestBad(t *testing.T) {
    sender := new(MockEmailSender)
    sender.On("Send", mock.Anything, mock.Anything, mock.Anything).Return(nil)
    // function under test never called Send — test still passes
    NotifyUser(sender, "alice@example.com")
}
```

**Right** — Always call `AssertExpectations` at the end of tests that use mocks.

```go
func TestGood(t *testing.T) {
    sender := new(MockEmailSender)
    sender.On("Send", mock.Anything, mock.Anything, mock.Anything).Return(nil)
    NotifyUser(sender, "alice@example.com")
    sender.AssertExpectations(t) // fails if Send was not called
}
```

### Not Guarding After a Nil Check

**Wrong** — If `obj` is nil, the second line panics with a nil pointer dereference.

```go
func TestBad(t *testing.T) {
    obj := mayReturnNil()
    assert.NotNil(t, obj)
    assert.Equal(t, "expected", obj.Value) // panic if obj is nil
}
```

**Right** — Use the bool return value of the assertion to guard further access.

```go
func TestGood(t *testing.T) {
    obj := mayReturnNil()
    if assert.NotNil(t, obj) {
        assert.Equal(t, "expected", obj.Value)
    }
}
```

### Using t.Parallel() Inside a Suite

**Wrong** — The `suite` package does not support parallel test methods.

```go
func (s *MyTestSuite) TestFoo() {
    s.T().Parallel() // undefined behavior; data races on suite state
    s.Equal(1, 1)
}
```

**Right** — Never call `t.Parallel()` inside suite test methods.

```go
func (s *MyTestSuite) TestFoo() {
    s.Equal(1, 1)
}
```

### Forgetting the suite.Run Wrapper

**Wrong** — Without a standard `TestXxx` function, `go test` will not discover or run any suite methods.

```go
type MyTestSuite struct{ suite.Suite }

func (s *MyTestSuite) TestFoo() { s.True(true) }
// go test runs zero tests — no discovery entry point
```

**Right** — Always add a standard test function that calls `suite.Run`.

```go
type MyTestSuite struct{ suite.Suite }

func (s *MyTestSuite) TestFoo() { s.True(true) }

func TestMyTestSuite(t *testing.T) {
    suite.Run(t, new(MyTestSuite))
}
```

## References

- [Documentation](https://pkg.go.dev/github.com/stretchr/testify)
- [Source](https://github.com/stretchr/testify)

## Migration from v1.x

Testify v1 guarantees no breaking changes within the v1.x line. No API migration is required when upgrading between v1 minor or patch releases.

**Deprecated items to avoid in new code:**

| Deprecated | Replacement | Status |
|-----------|-------------|--------|
| `assert.ObjectsExportedFieldsAreEqual` | `assert.EqualExportedValues(t, expected, actual)` | ⚠️ Soft Deprecation — still works, prefer new API |
| `assert.CompareType` | _(internal type, do not use)_ | ⚠️ Soft Deprecation since v1.6.0 — accidentally exported |
| `github.com/stretchr/testify/http` | `net/http/httptest` (stdlib) | ❌ Hard Deprecation — do not use in new code |

**Upgrade command:**

```sh
go get -u github.com/stretchr/testify
```

## API Reference

**assert.Equal(t, expected, actual, msgAndArgs...)** — Asserts that two values are deeply equal; fails with a diff on mismatch.

**assert.NotEqual(t, expected, actual, msgAndArgs...)** — Asserts that two values are not equal.

**assert.Nil(t, object, msgAndArgs...)** — Asserts that the value is nil.

**assert.NotNil(t, object, msgAndArgs...)** — Asserts that the value is not nil.

**assert.NoError(t, err, msgAndArgs...)** — Asserts that an error value is nil.

**assert.Error(t, err, msgAndArgs...)** — Asserts that an error value is non-nil.

**assert.ErrorIs(t, err, target, msgAndArgs...)** — Asserts that `errors.Is(err, target)` is true.

**assert.ErrorAs(t, err, target, msgAndArgs...)** — Asserts that `errors.As(err, target)` is true.

**assert.True(t, value, msgAndArgs...)** — Asserts that a boolean value is true.

**assert.False(t, value, msgAndArgs...)** — Asserts that a boolean value is false.

**assert.Contains(t, s, contains, msgAndArgs...)** — Asserts that a string, slice, or map contains the given element.

**assert.ElementsMatch(t, listA, listB, msgAndArgs...)** — Asserts that two lists contain the same elements regardless of order.

**assert.Len(t, object, length, msgAndArgs...)** — Asserts that an object (string, slice, map, channel) has the given length.

**assert.Greater(t, e1, e2, msgAndArgs...)** — Asserts that `e1 > e2` for comparable types (int, float, string, time.Time, []byte).

**assert.New(t) *Assertions** — Returns a bound `*Assertions` object that omits the `t` argument on every subsequent call.