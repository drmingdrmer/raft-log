# WAL Flush Batch Benchmark Report

- Generated at Unix time: 1778643126
- Build profile: release
- Operations per case: 10240
- Payload bytes: 256
- Wait windows: [1000, 2000, 5000] us
- Batch sizes: [256, 512, 1024, 2048, 4096]

## Methodology

- Each case opens a fresh temporary RaftLog directory.
- The benchmark submits all append + sync flush operations first, then waits for all callbacks.
- Callback latency is measured from flush submission to callback completion, so it includes flush-worker queueing, file write, and fsync time.
- QPS is computed from total operations divided by wall-clock time for submitting and completing all callbacks.

## Summary

| case | wait | configured batch | elapsed ms | qps | avg us | p50 us | p90 us | p99 us | max us | batches | sync batches | writes/batch | max batch | sync avg us | sync max us | group avg us | group max us |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| wait_1000us_batch_256 | 1ms | 256 | 224.61 | 45590 | 26282 | 27051 | 29551 | 30925 | 36061 | 40 | 40 | 256.00 | 256 | 5175.52 | 7778 | 48.27 | 357 |
| wait_2000us_batch_256 | 2ms | 256 | 225.85 | 45339 | 26368 | 27528 | 29178 | 30235 | 35621 | 40 | 40 | 256.00 | 256 | 5231.05 | 6185 | 52.35 | 285 |
| wait_5000us_batch_256 | 5ms | 256 | 271.90 | 37661 | 31798 | 31217 | 43526 | 45718 | 50775 | 40 | 40 | 256.00 | 256 | 6274.38 | 20125 | 39.08 | 341 |
| wait_1000us_batch_512 | 1ms | 512 | 108.84 | 94085 | 15121 | 15506 | 16399 | 16997 | 21734 | 20 | 20 | 512.00 | 512 | 5127.00 | 6516 | 51.60 | 281 |
| wait_2000us_batch_512 | 2ms | 512 | 118.31 | 86554 | 16049 | 16356 | 18672 | 19147 | 24851 | 20 | 20 | 512.00 | 512 | 5346.10 | 6240 | 92.95 | 480 |
| wait_5000us_batch_512 | 5ms | 512 | 120.93 | 84675 | 16473 | 17126 | 18647 | 19401 | 25197 | 20 | 20 | 512.00 | 512 | 5423.55 | 6241 | 86.20 | 669 |
| wait_1000us_batch_1024 | 1ms | 1024 | 75.73 | 135213 | 11851 | 12038 | 13743 | 14433 | 19832 | 11 | 11 | 930.91 | 1024 | 5356.00 | 6356 | 296.36 | 1253 |
| wait_2000us_batch_1024 | 2ms | 1024 | 66.12 | 154865 | 10445 | 10064 | 13365 | 13982 | 18702 | 11 | 11 | 930.91 | 1024 | 5082.00 | 7893 | 511.55 | 2509 |
| wait_5000us_batch_1024 | 5ms | 1024 | 55.57 | 184260 | 9568 | 9833 | 10775 | 11731 | 15326 | 10 | 10 | 1024.00 | 1024 | 4786.60 | 5429 | 267.30 | 1011 |
| wait_1000us_batch_2048 | 1ms | 2048 | 48.02 | 213264 | 9552 | 10792 | 14342 | 14702 | 14769 | 7 | 7 | 1462.86 | 2011 | 4798.29 | 5662 | 1350.71 | 2152 |
| wait_2000us_batch_2048 | 2ms | 2048 | 48.69 | 210315 | 10047 | 9854 | 13871 | 15825 | 15900 | 6 | 6 | 1706.67 | 2048 | 5246.67 | 6982 | 1658.67 | 2508 |
| wait_5000us_batch_2048 | 5ms | 2048 | 41.03 | 249548 | 9327 | 8179 | 13450 | 13890 | 14139 | 5 | 5 | 2048.00 | 2048 | 5017.60 | 6318 | 1821.80 | 3509 |
| wait_1000us_batch_4096 | 1ms | 4096 | 50.32 | 203498 | 9899 | 11464 | 13336 | 13800 | 14143 | 7 | 7 | 1462.86 | 1923 | 5307.43 | 6008 | 1059.43 | 1277 |
| wait_2000us_batch_4096 | 2ms | 4096 | 52.46 | 195204 | 10851 | 8950 | 15168 | 16060 | 16112 | 6 | 6 | 1706.67 | 2462 | 5166.33 | 6028 | 2228.83 | 2525 |
| wait_5000us_batch_4096 | 5ms | 4096 | 36.39 | 281375 | 11648 | 10313 | 18602 | 19758 | 19824 | 3 | 3 | 3413.33 | 4096 | 5391.00 | 6249 | 4290.67 | 5579 |

## wait_1000us_batch_256

- wait: 1ms
- configured batch: 256
- operations: 10240
- payload bytes: 256
- elapsed: 224.61 ms
- write bytes: 2949120
- queued wait max: 30559 us
- write max: 838 us
- batch max: 8019 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 256 | 2.50% |
| 10001..20000 us | 512 | 5.00% |
| 20001..50000 us | 9472 | 92.50% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_2000us_batch_256

- wait: 2ms
- configured batch: 256
- operations: 10240
- payload bytes: 256
- elapsed: 225.85 ms
- write bytes: 2949120
- queued wait max: 30128 us
- write max: 660 us
- batch max: 6393 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 256 | 2.50% |
| 10001..20000 us | 512 | 5.00% |
| 20001..50000 us | 9472 | 92.50% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_5000us_batch_256

- wait: 5ms
- configured batch: 256
- operations: 10240
- payload bytes: 256
- elapsed: 271.90 ms
- write bytes: 2949120
- queued wait max: 44908 us
- write max: 3943 us
- batch max: 20322 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 256 | 2.50% |
| 10001..20000 us | 256 | 2.50% |
| 20001..50000 us | 9724 | 94.96% |
| 50001..100000 us | 4 | 0.04% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_1000us_batch_512

- wait: 1ms
- configured batch: 512
- operations: 10240
- payload bytes: 256
- elapsed: 108.84 ms
- write bytes: 2949120
- queued wait max: 16764 us
- write max: 360 us
- batch max: 6562 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 512 | 5.00% |
| 10001..20000 us | 9711 | 94.83% |
| 20001..50000 us | 17 | 0.17% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_2000us_batch_512

- wait: 2ms
- configured batch: 512
- operations: 10240
- payload bytes: 256
- elapsed: 118.31 ms
- write bytes: 2949120
- queued wait max: 18668 us
- write max: 700 us
- batch max: 6518 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 512 | 5.00% |
| 10001..20000 us | 9711 | 94.83% |
| 20001..50000 us | 17 | 0.17% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_5000us_batch_512

- wait: 5ms
- configured batch: 512
- operations: 10240
- payload bytes: 256
- elapsed: 120.93 ms
- write bytes: 2949120
- queued wait max: 19027 us
- write max: 892 us
- batch max: 6630 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 512 | 5.00% |
| 10001..20000 us | 9711 | 94.83% |
| 20001..50000 us | 17 | 0.17% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_1000us_batch_1024

- wait: 1ms
- configured batch: 1024
- operations: 10240
- payload bytes: 256
- elapsed: 75.73 ms
- write bytes: 2949120
- queued wait max: 13103 us
- write max: 3117 us
- batch max: 8004 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 1186 | 11.58% |
| 10001..20000 us | 9054 | 88.42% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_2000us_batch_1024

- wait: 2ms
- configured batch: 1024
- operations: 10240
- payload bytes: 256
- elapsed: 66.12 ms
- write bytes: 2949120
- queued wait max: 13449 us
- write max: 445 us
- batch max: 8136 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 4864 | 47.50% |
| 10001..20000 us | 5376 | 52.50% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_5000us_batch_1024

- wait: 5ms
- configured batch: 1024
- operations: 10240
- payload bytes: 256
- elapsed: 55.57 ms
- write bytes: 2949120
- queued wait max: 10626 us
- write max: 325 us
- batch max: 5769 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 6175 | 60.30% |
| 10001..20000 us | 4065 | 39.70% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_1000us_batch_2048

- wait: 1ms
- configured batch: 2048
- operations: 10240
- payload bytes: 256
- elapsed: 48.02 ms
- write bytes: 2949120
- queued wait max: 9829 us
- write max: 1339 us
- batch max: 7015 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 178 | 1.74% |
| 5001..10000 us | 4557 | 44.50% |
| 10001..20000 us | 5505 | 53.76% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_2000us_batch_2048

- wait: 2ms
- configured batch: 2048
- operations: 10240
- payload bytes: 256
- elapsed: 48.69 ms
- write bytes: 2949120
- queued wait max: 10224 us
- write max: 869 us
- batch max: 7634 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 5381 | 52.55% |
| 10001..20000 us | 4859 | 47.45% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_5000us_batch_2048

- wait: 5ms
- configured batch: 2048
- operations: 10240
- payload bytes: 256
- elapsed: 41.03 ms
- write bytes: 2949120
- queued wait max: 8612 us
- write max: 840 us
- batch max: 6875 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 6140 | 59.96% |
| 10001..20000 us | 4100 | 40.04% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_1000us_batch_4096

- wait: 1ms
- configured batch: 4096
- operations: 10240
- payload bytes: 256
- elapsed: 50.32 ms
- write bytes: 2949120
- queued wait max: 8000 us
- write max: 918 us
- batch max: 6359 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 4618 | 45.10% |
| 10001..20000 us | 5622 | 54.90% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_2000us_batch_4096

- wait: 2ms
- configured batch: 4096
- operations: 10240
- payload bytes: 256
- elapsed: 52.46 ms
- write bytes: 2949120
- queued wait max: 9691 us
- write max: 978 us
- batch max: 6610 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 5182 | 50.61% |
| 10001..20000 us | 5058 | 49.39% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |

## wait_5000us_batch_4096

- wait: 5ms
- configured batch: 4096
- operations: 10240
- payload bytes: 256
- elapsed: 36.39 ms
- write bytes: 2949120
- queued wait max: 13514 us
- write max: 1964 us
- batch max: 7438 us

### Callback Latency Histogram

| bucket | count | percent |
|---:|---:|---:|
| 0..100 us | 0 | 0.00% |
| 101..250 us | 0 | 0.00% |
| 251..500 us | 0 | 0.00% |
| 501..1000 us | 0 | 0.00% |
| 1001..2000 us | 0 | 0.00% |
| 2001..5000 us | 0 | 0.00% |
| 5001..10000 us | 4390 | 42.87% |
| 10001..20000 us | 5850 | 57.13% |
| 20001..50000 us | 0 | 0.00% |
| 50001..100000 us | 0 | 0.00% |
| 100001..250000 us | 0 | 0.00% |
| 250001..500000 us | 0 | 0.00% |
| 500001..1000000 us | 0 | 0.00% |
| >1000000 us | 0 | 0.00% |
