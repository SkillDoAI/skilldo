---

name: cryptography
description: Cryptographic primitives and recipes for Python including symmetric encryption, asymmetric cryptography, key derivation, and X.509 certificate handling
version: 47.0.0.dev1
ecosystem: python
license: Apache-2.0 OR BSD-3-Clause
generated_with: claude-sonnet-4-5-20250929
---

## Imports

```python
from cryptography import x509
from cryptography.fernet import Fernet
from cryptography.hazmat.primitives import hashes, hmac, serialization
from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes
from cryptography.hazmat.primitives.ciphers.aead import AESGCM, ChaCha20Poly1305
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
from cryptography.hazmat.primitives.kdf.hkdf import HKDF
from cryptography.hazmat.primitives.asymmetric import rsa, padding, ec
from cryptography.hazmat.primitives.twofactor.totp import TOTP
from cryptography.hazmat.primitives.twofactor.hotp import HOTP
```

## Core Patterns

### High-Level Symmetric Encryption with Fernet ✅ Current
```python
from cryptography.fernet import Fernet
import os

# Generate a key and save it securely
key = Fernet.generate_key()  # Returns URL-safe base64-encoded 32-byte key
# Put this somewhere safe!

# Encrypt data
f = Fernet(key)
token = f.encrypt(b"my secret message")

# Decrypt data
plaintext = f.decrypt(token)
```
* Fernet provides authenticated encryption with automatic key rotation support
* Best choice for simple symmetric encryption needs
* Handles IV generation, authentication, and timestamp automatically

### Authenticated Encryption with AESGCM ✅ Current
```python
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
import os

# Generate a random 256-bit key
key = os.urandom(32)
aesgcm = AESGCM(key)

# Encrypt with associated data (AAD)
nonce = os.urandom(12)  # 96-bit nonce for GCM
plaintext = b"sensitive data"
associated_data = b"header info"
ciphertext = aesgcm.encrypt(nonce, plaintext, associated_data)

# Decrypt
decrypted = aesgcm.decrypt(nonce, ciphertext, associated_data)
```
* Use AESGCM for performance-critical authenticated encryption
* Nonce must be unique for each encryption operation with the same key
* AAD provides additional context that's authenticated but not encrypted

### Key Derivation with PBKDF2 ✅ Current
```python
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC
import os

# Derive a key from a password
password = b"user_password"
salt = os.urandom(16)

kdf = PBKDF2HMAC(
    algorithm=hashes.SHA256(),
    length=32,
    salt=salt,
    iterations=600000,  # OWASP recommendation as of 2023
)
key = kdf.derive(password)

# Verify a password
kdf2 = PBKDF2HMAC(
    algorithm=hashes.SHA256(),
    length=32,
    salt=salt,
    iterations=600000,
)
kdf2.verify(password, key)  # Raises InvalidKey if wrong
```
* Use PBKDF2 for password-based key derivation
* Store salt with the derived key for verification
* Use at least 600,000 iterations for SHA256

### X.509 Certificate Loading and Verification ✅ Current
```python
from cryptography import x509
from cryptography.x509.oid import NameOID
import datetime

# Load a PEM certificate
with open("cert.pem", "rb") as f:
    cert = x509.load_pem_x509_certificate(f.read())

# Extract certificate information
subject = cert.subject
common_name = subject.get_attributes_for_oid(NameOID.COMMON_NAME)[0].value
issuer = cert.issuer
not_valid_before = cert.not_valid_before_utc
not_valid_after = cert.not_valid_after_utc

# Load DER certificate
with open("cert.der", "rb") as f:
    cert_der = x509.load_der_x509_certificate(f.read())

# Certificate chain verification
from cryptography.x509 import verification
import certifi

with open(certifi.where(), "rb") as f:
    store = verification.Store(x509.load_pem_x509_certificates(f.read()))

verifier = (
    verification.PolicyBuilder()
    .store(store)
    .time(datetime.datetime.now(datetime.timezone.utc))
    .build_server_verifier(x509.DNSName("example.com"))
)
verifier.verify(leaf_cert, [intermediate_cert])
```
* Use `load_pem_x509_certificate` for PEM-encoded certificates
* Use `load_der_x509_certificate` for DER-encoded certificates
* PolicyBuilder provides flexible certificate chain verification

### Two-Factor Authentication (TOTP) ✅ Current
```python
from cryptography.hazmat.primitives.twofactor.totp import TOTP
from cryptography.hazmat.primitives.hashes import SHA1
from cryptography.hazmat.primitives.twofactor import InvalidToken
import os
import time

# Generate a secret key (minimum 128 bits, recommend 160 bits)
key = os.urandom(20)  # 160 bits

# Create TOTP instance
totp = TOTP(key, 8, SHA1(), 30)  # 8 digits, 30 second window

# Generate current token
token = totp.generate(time.time())

# Verify token
try:
    totp.verify(token, time.time())
    print("Token valid")
except InvalidToken:
    print("Invalid token")

# Generate provisioning URI for QR code
uri = totp.get_provisioning_uri("user@example.com", "MyApp")
```
* Key must be at least 128 bits (160 bits recommended)
* Implement throttling to prevent brute force attacks
* Time window allows for clock drift between client and server

## Configuration

### Random Number Generation
```python
import os

# Always use os.urandom() for cryptographic random bytes
random_bytes = os.urandom(32)

# For random integers
random_int = int.from_bytes(os.urandom(4), byteorder='big')

# For text-based tokens, use secrets module
import secrets
token = secrets.token_urlsafe(32)
```

### Cipher Block Sizes and Key Lengths
```python
# AES supports 128, 192, or 256-bit keys
from cryptography.hazmat.primitives.ciphers import algorithms

aes128 = algorithms.AES(os.urandom(16))  # 128-bit
aes192 = algorithms.AES(os.urandom(24))  # 192-bit
aes256 = algorithms.AES(os.urandom(32))  # 256-bit

# Block size is always 128 bits (16 bytes) for AES
# IV must match block size
```

### HOTP Counter Management
```python
from cryptography.hazmat.primitives.twofactor.hotp import HOTP
from cryptography.hazmat.primitives.hashes import SHA1

def verify_hotp_with_lookahead(key, token, counter, look_ahead=10):
    """Verify HOTP with counter resynchronization window."""
    hotp = HOTP(key, 6, SHA1())
    
    for count in range(counter, counter + look_ahead):
        try:
            hotp.verify(token, count)
            return count  # Return new counter value
        except InvalidToken:
            continue
    
    return None  # Token invalid
```

## Pitfalls

### Wrong: Using standard random module for cryptography
```python
import random

# NEVER do this - not cryptographically secure!
key = bytes([random.randint(0, 255) for _ in range(32)])
iv = bytes([random.randint(0, 255) for _ in range(16)])
```

### Right: Use os.urandom() for cryptographic randomness
```python
import os

# Always use os.urandom() for cryptographic purposes
key = os.urandom(32)
iv = os.urandom(16)

# Or use secrets module for tokens
import secrets
token = secrets.token_bytes(32)
```
* The standard `random` module is not cryptographically secure and can lead to predictable keys/IVs
* `os.urandom()` provides cryptographically strong random bytes from the OS

### Wrong: Using decrepit cipher modes for new applications
```python
from cryptography.hazmat.decrepit.ciphers.modes import CFB
from cryptography.hazmat.primitives.ciphers import algorithms

# Don't use decrepit modes like CFB, CFB8, OFB in new code
cipher = Cipher(algorithms.AES(key), CFB(iv))
```

### Right: Use modern authenticated encryption
```python
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
import os

# Use AESGCM or ChaCha20Poly1305 for new applications
key = os.urandom(32)
aesgcm = AESGCM(key)
ciphertext = aesgcm.encrypt(os.urandom(12), plaintext, associated_data)
```
* CFB, CFB8, and OFB modes are in the `decrepit` module and should only be used for legacy compatibility
* Modern AEAD modes like AESGCM provide both encryption and authentication

### Wrong: No throttling on HOTP/TOTP verification
```python
def verify_totp(user_token):
    totp = TOTP(get_user_key(), 6, SHA1(), 30)
    try:
        totp.verify(user_token, time.time())
        return True
    except InvalidToken:
        return False
```

### Right: Implement rate limiting and account lockout
```python
import time
from collections import defaultdict

failed_attempts = defaultdict(int)
lockout_until = {}

def verify_totp_secure(user_id, user_token, max_attempts=5):
    # Check if account is locked out
    if user_id in lockout_until:
        if time.time() < lockout_until[user_id]:
            raise Exception("Account temporarily locked")
        else:
            del lockout_until[user_id]
            failed_attempts[user_id] = 0
    
    totp = TOTP(get_user_key(user_id), 6, SHA1(), 30)
    try:
        totp.verify(user_token, time.time())
        failed_attempts[user_id] = 0  # Reset on success
        return True
    except InvalidToken:
        failed_attempts[user_id] += 1
        if failed_attempts[user_id] >= max_attempts:
            lockout_until[user_id] = time.time() + 900  # 15 min lockout
        return False
```
* HOTP/TOTP tokens are only 6-8 digits and vulnerable to brute force
* Always implement throttling with exponential backoff or account lockout

### Wrong: Using weak key lengths for HOTP/TOTP
```python
# 80-bit key (too weak, will raise ValueError by default)
key = os.urandom(10)
hotp = HOTP(key, 6, SHA1())  # Raises ValueError
```

### Right: Use minimum 128-bit keys (160 bits recommended)
```python
import os
from cryptography.hazmat.primitives.twofactor.hotp import HOTP
from cryptography.hazmat.primitives.hashes import SHA1

# 160-bit key (recommended)
key = os.urandom(20)
hotp = HOTP(key, 6, SHA1())

# If you MUST support 80-bit keys (not recommended):
key_80bit = os.urandom(10)
hotp = HOTP(key_80bit, 6, SHA1(), enforce_key_length=False)
# Add additional security checks before using
```
* Keys should be at least 128 bits, preferably 160 bits
* `enforce_key_length=True` (default) prevents weak keys

### Wrong: Reusing nonces in AESGCM
```python
from cryptography.hazmat.primitives.ciphers.aead import AESGCM

key = os.urandom(32)
aesgcm = AESGCM(key)
nonce = os.urandom(12)

# NEVER reuse the same nonce with the same key!
ct1 = aesgcm.encrypt(nonce, b"message 1", None)
ct2 = aesgcm.encrypt(nonce, b"message 2", None)  # CATASTROPHIC
```

### Right: Generate a unique nonce for every encryption
```python
from cryptography.hazmat.primitives.ciphers.aead import AESGCM
import os

key = os.urandom(32)
aesgcm = AESGCM(key)

# Generate fresh nonce for each encryption
nonce1 = os.urandom(12)
ct1 = aesgcm.encrypt(nonce1, b"message 1", None)

nonce2 = os.urandom(12)
ct2 = aesgcm.encrypt(nonce2, b"message 2", None)

# Store nonce with ciphertext: nonce1 + ct1, nonce2 + ct2
```
* Nonce reuse with the same key completely breaks GCM security
* Generate a new random nonce for every encryption operation
* Store the nonce alongside the ciphertext for decryption

## References

- [homepage](https://github.com/pyca/cryptography)
- [documentation](https://cryptography.io/)
- [source](https://github.com/pyca/cryptography/)
- [issues](https://github.com/pyca/cryptography/issues)
- [changelog](https://cryptography.io/en/latest/changelog/)

## Migration from v46.x

### Deprecated Cipher Modes Moved to `decrepit` Module
```python
# Before (v46.x)
from cryptography.hazmat.primitives.ciphers.modes import CFB, CFB8, OFB

# After (v47.0.0)
from cryptography.hazmat.decrepit.ciphers.modes import CFB, CFB8, OFB
```

### Deprecated Algorithms Moved to `decrepit` Module
```python
# Before (v42.x)
from cryptography.hazmat.primitives.ciphers.algorithms import (
    ARC4, TripleDES, CAST5, SEED, Blowfish, IDEA
)

# After (v43.0.0+)
from cryptography.hazmat.decrepit.ciphers.algorithms import (
    ARC4, TripleDES, CAST5, SEED, Blowfish, IDEA
)
```

### TripleDES Key Length Changes (Future)
TripleDES will only accept 192-bit (24-byte) keys in a future release. Prepare now:

```python
from cryptography.hazmat.decrepit.ciphers.algorithms import TripleDES

# If you have a 64-bit single DES key:
single_key = b"12345678"
triple_key = single_key + single_key + single_key  # 192 bits

# If you have a 128-bit two-key TripleDES key:
two_key = b"1234567890123456"
triple_key = two_key + two_key[:8]  # 192 bits

cipher = Cipher(TripleDES(triple_key), mode)
```

**Important:** All algorithms and modes in the `decrepit` module should not be used for new applications. They are only provided for backwards compatibility with legacy systems.