---
name: scikit-learn
description: A Python library for machine learning that provides simple and efficient tools for data mining and data analysis.
version: unknown
ecosystem: python
license: BSD-3-Clause
---

## Imports

Show the standard import patterns. Most common first:
```python
from sklearn.model_selection import train_test_split
from sklearn.linear_model import LogisticRegression
from sklearn.preprocessing import StandardScaler
from sklearn.metrics import accuracy_score
from sklearn.datasets import load_iris
```

## Core Patterns

### Data Loading ✅ Current
```python
from sklearn.datasets import load_iris

# Load iris dataset
iris = load_iris()
X, y = iris.data, iris.target
```
* Loads standard datasets for machine learning experiments.
* **Status**: Current, stable

### Train-Test Split ✅ Current
```python
from sklearn.model_selection import train_test_split

X_train, X_test, y_train, y_test = train_test_split(
    X, y, test_size=0.2, random_state=42
)
```
* Splits data into training and testing sets.
* **Status**: Current, stable

### Model Training ✅ Current
```python
from sklearn.linear_model import LogisticRegression

# Create and train model
model = LogisticRegression(random_state=42)
model.fit(X_train, y_train)
```
* Initializes and trains a machine learning model.
* **Status**: Current, stable

### Model Prediction ✅ Current
```python
from sklearn.metrics import accuracy_score

# Make predictions
y_pred = model.predict(X_test)
accuracy = accuracy_score(y_test, y_pred)
print(f"Accuracy: {accuracy:.2f}")
```
* Makes predictions using a trained model and evaluates performance.
* **Status**: Current, stable

### Feature Scaling ✅ Current
```python
from sklearn.preprocessing import StandardScaler

# Scale features
scaler = StandardScaler()
X_train_scaled = scaler.fit_transform(X_train)
X_test_scaled = scaler.transform(X_test)
```
* Normalizes features to have zero mean and unit variance.
* **Status**: Current, stable

## Configuration

Standard configuration and setup:
- Default values for all major algorithms are set to sensible defaults
- Common customizations include setting `random_state` for reproducibility
- No environment variables or config files are required
- All settings are passed as parameters to constructors

## Pitfalls

### Wrong: Using `fit_transform` on test data
```python
from sklearn.preprocessing import StandardScaler

scaler = StandardScaler()
X_train_scaled = scaler.fit_transform(X_train)
X_test_scaled = scaler.fit_transform(X_test)  # Wrong!
```

### Right: Using `fit` on train and `transform` on test
```python
from sklearn.preprocessing import StandardScaler

scaler = StandardScaler()
X_train_scaled = scaler.fit_transform(X_train)
X_test_scaled = scaler.transform(X_test)  # Correct!
```
* Using `fit_transform()` on test data causes data leakage and overfitting.
* **Status**: Common mistake

### Wrong: Not splitting data before preprocessing
```python
from sklearn.preprocessing import StandardScaler

scaler = StandardScaler()
X_scaled = scaler.fit_transform(X)  # Wrong!
```

### Right: Split data first, then apply scaling
```python
from sklearn.model_selection import train_test_split
from sklearn.preprocessing import StandardScaler

X_train, X_test, y_train, y_test = train_test_split(X, y, test_size=0.2, random_state=42)
scaler = StandardScaler()
X_train_scaled = scaler.fit_transform(X_train)
X_test_scaled = scaler.transform(X_test)
```
* Preprocessing before splitting leads to data leakage.
* **Status**: Common mistake

### Wrong: Ignoring `random_state` parameter
```python
from sklearn.model_selection import train_test_split

X_train, X_test, y_train, y_test = train_test_split(X, y)  # Wrong!
```

### Right: Setting `random_state` for reproducibility
```python
from sklearn.model_selection import train_test_split

X_train, X_test, y_train, y_test = train_test_split(
    X, y, test_size=0.2, random_state=42
)
```
* Not setting `random_state` leads to non-reproducible results.
* **Status**: Common mistake

### Wrong: Using deprecated `cross_validation` module
```python
from sklearn.cross_validation import train_test_split  # Wrong!
```

### Right: Using updated `model_selection` module
```python
from sklearn.model_selection import train_test_split  # Correct!
```
* The `cross_validation` module was deprecated in favor of `model_selection`.
* **Status**: Common mistake

## References

- [homepage](https://scikit-learn.org)
- [source](https://github.com/scikit-learn/scikit-learn)
- [download](https://pypi.org/project/scikit-learn/#files)
- [tracker](https://github.com/scikit-learn/scikit-learn/issues)
- [release notes](https://scikit-learn.org/stable/whats_new)

## Migration from v1.1

### Breaking Changes
- The `cross_validation` module was removed and replaced with `model_selection`
- Some parameters in estimators have changed default values

### Deprecated → Current Mapping
- `sklearn.cross_validation` → `sklearn.model_selection`
- `sklearn.utils.validation` functions may have changed signatures

### Before/After Code Examples
**Before**: `from sklearn.cross_validation import train_test_split`
**After**: `from sklearn.model_selection import train_test_split`

## API Reference

Brief reference of the most important public APIs:

- **train_test_split()** - Splits arrays or matrices into random train and test subsets
- **LogisticRegression()** - Logistic regression classifier with various regularization options
- **StandardScaler()** - Standardizes features by removing the mean and scaling to unit variance
- **accuracy_score()** - Computes the accuracy of predictions
- **load_iris()** - Loads the iris dataset for machine learning experiments
- **fit()** - Fits the model to training data
- **predict()** - Makes predictions using the trained model
- **fit_transform()** - Fits the transformer to data and returns transformed data
- **transform()** - Transforms data according to the fitted transformer
- **cross_val_score()** - Evaluates model performance using cross-validation
- **GridSearchCV()** - Exhaustive search over specified parameter values for an estimator
- **Pipeline()** - Chains multiple steps together for preprocessing and modeling
- **OneHotEncoder()** - Encodes categorical features as a one-hot numeric array
- **LabelEncoder()** - Encodes labels with value between 0 and n_classes-1
- **R2Score()** - Computes the coefficient of determination R^2

Focus on the 10-15 most used APIs.