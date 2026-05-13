#!/usr/bin/env bash

set -euo pipefail

cargo run --release --example flush_batch_bench -- \
    --operations 10240 \
    --payload-bytes 256 \
    --wait-us 1000,2000,5000 \
    --batch-size 256,512,1024,2048,4096 \
    --output docs/group_sync_report.md
