# Compatibility Tests

This directory contains compatibility tests for the Raft-log implementation to ensure data written by different versions can be correctly read and processed.

Each version has a subdirectory containing sample data and expected outputs. The tests verify that:
1. The current version can read and process data written by older versions
2. The current version generates data in a format compatible with older versions


When a new version is added, the following steps are required:

1. Generate sample data for the new version by running:
   ```bash
   cargo test --package raft-log --test test_compat generate_data -- --ignored
   ```

2. Verify that the generated data is correct by running the compatibility tests:
   ```bash
   cargo test --package raft-log --test test_compat
   ```

The generated data will be stored in a subdirectory named after the current version. This data will be used as a reference for future compatibility testing.





