# Compatibility fixtures

These files contain synthetic test data, encrypted by the original Python
app's `crypto.encrypt_json` implementation before its removal.
Both use the test password `Compatibility test 🔐` and the original default
Argon2id parameters (3 passes, 64 MiB, 2 lanes).

- `python-v1.json`: multiple-note schema, Unicode content and credentials,
  categories, timestamps, and an unknown metadata field that must survive saves.
- `python-legacy.json`: the older single-string notes schema.

The Rust tests decrypt both fixtures and check the contents. During migration,
a Rust-saved version was also decrypted with the original Python implementation
to verify compatibility in the other direction. The application and automated
test suite require no Python installation.
