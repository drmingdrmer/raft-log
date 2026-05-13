# WAL Flush Batch Benchmark Report

- Generated at Unix time: 1778640144
- Build profile: release
- Operations per case: 1024
- Payload bytes: 256
- Wait windows: [0, 100, 1000, 2000, 5000] us

## Methodology

- Each case opens a fresh temporary RaftLog directory.
- The benchmark submits all append + sync flush operations first, then waits for all callbacks.
- Callback latency is measured from flush submission to callback completion, so it includes flush-worker queueing, file write, and fsync time.
- QPS is computed from total operations divided by wall-clock time for submitting and completing all callbacks.

## Summary

| case | wait | elapsed ms | qps | avg us | p50 us | p90 us | p99 us | max us | batches | sync batches | writes/batch | max batch | sync avg us | sync max us | group avg us | group max us |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| wait_0us | 0us | 5481.55 | 187 | 2740314 | 2747614 | 4948348 | 5432040 | 5480163 | 1024 | 1024 | 1.00 | 1 | 5270.16 | 133762 | 0.00 | 0 |
| wait_100us | 100us | 31.11 | 32911 | 20228 | 22974 | 28854 | 29164 | 29179 | 4 | 4 | 256.00 | 466 | 5269.50 | 5710 | 112.00 | 144 |
| wait_1000us | 1ms | 17.28 | 59251 | 11376 | 9582 | 16083 | 16147 | 16153 | 2 | 2 | 512.00 | 714 | 4992.50 | 5288 | 1142.50 | 1278 |
| wait_2000us | 2ms | 10.56 | 96951 | 9967 | 10058 | 10259 | 10292 | 10298 | 1 | 1 | 1024.00 | 1024 | 5020.00 | 5020 | 103.00 | 103 |
| wait_5000us | 5ms | 11.59 | 88318 | 9611 | 9603 | 9920 | 10092 | 10096 | 1 | 1 | 1024.00 | 1024 | 5432.00 | 5432 | 1116.00 | 1116 |

## wait_0us

- wait: 0us
- operations: 1024
- payload bytes: 256
- elapsed: 5481.55 ms
- write bytes: 294912
- queued wait max: 5475683 us
- write max: 882 us
- batch max: 133916 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 0 | 0.00% |
| 10001..20000 us | 2 | 0.20% |
| 20001..50000 us | 6 | 0.59% |
| 50001..100000 us | 10 | 0.98% |
| 100001..250000 us | 29 | 2.83% |
| 250001..500000 us | 50 | 4.88% |
| 500001..1000000 us | 101 | 9.86% |
| >1000000 us | 826 | 80.66% |

## wait_100us

- wait: 100us
- operations: 1024
- payload bytes: 256
- elapsed: 31.11 ms
- write bytes: 294912
- queued wait max: 23037 us
- write max: 4057 us
- batch max: 8866 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 53 | 5.18% |
| 10001..20000 us | 362 | 35.35% |
| 20001..50000 us | 609 | 59.47% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_1000us

- wait: 1ms
- operations: 1024
- payload bytes: 256
- elapsed: 17.28 ms
- write bytes: 294912
- queued wait max: 10166 us
- write max: 3442 us
- batch max: 8739 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 714 | 69.73% |
| 10001..20000 us | 310 | 30.27% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_2000us

- wait: 2ms
- operations: 1024
- payload bytes: 256
- elapsed: 10.56 ms
- write bytes: 294912
- queued wait max: 1250 us
- write max: 4013 us
- batch max: 9041 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 432 | 42.19% |
| 10001..20000 us | 592 | 57.81% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_5000us

- wait: 5ms
- operations: 1024
- payload bytes: 256
- elapsed: 11.59 ms
- write bytes: 294912
- queued wait max: 1135 us
- write max: 3516 us
- batch max: 8958 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 966 | 94.34% |
| 10001..20000 us | 58 | 5.66% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |
