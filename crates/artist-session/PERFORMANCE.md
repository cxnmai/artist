# Session storage performance

## Reproduce

```sh
cargo run --release -p artist-session --example session_perf
# Smaller dataset:
ARTIST_PERF_MAX_HISTORY=10000 cargo run --release -p artist-session --example session_perf
```

The runner creates a fresh temporary directory and never opens the default store.
An optional absolute root argument must not already exist. Output is JSONL with
sample counts, p50/p95/max latency, database/WAL sizes, and throughput. Fixtures use
~1 KB messages, 100–100,000 initial messages, 1,000 sessions (100 matching a cwd),
large native replay blocks, and 1/4 independent writer processes. The final checks
exercise a large resume contending with another writer and cancellation safety.
No model/network calls or user-session data are involved.

## Measurements

Measured on Linux x86_64, AMD Ryzen AI 7 PRO 350, ext4, optimized release builds,
warm OS filesystem cache, FULL synchronous commits. Baseline is `d63d474`; fixed
storage is `b45370b`. An additional confirmation run at `a73d7e1` agrees on the
main results and adds mixed-workload/cancellation checks. These are local
microbenchmarks, not cross-platform guarantees or cold-cache/power-loss tests.
Read operations have 5–10 samples and appends have 30; percentiles are descriptive,
not confidence intervals. One bulk-initialization sample is not a distribution.

At 10,000 messages, times in milliseconds:

| Operation | Baseline p50 | Fixed p50 | Fixed p95 |
|---|---:|---:|---:|
| Durable ~1 KB append | 24.26 | 1.99 | 2.47 |
| Metadata lookup | 9.27 | 1.39 | 1.61 |
| First 100-record transcript page | 9.83 | 1.86 | 2.05 |
| Full canonical transcript | 46.48 | 44.58 | 48.18 |
| Resume | 58.27 | 47.85 | 51.62 |
| Resume after compaction | 59.95 | 48.29 | 50.08 |
| 1 ms executor timer, observed interval | 24.62 | 2.07 | 2.22 |

The timer's ~2 ms interval includes platform timer granularity. Its maximum interval
in this workload fell from 29.7 ms to 2.3 ms. Bulk seeding 10,000 messages fell from
5.57 seconds to 0.17 seconds. End-of-workload WAL size fell from 23.7 MB to 0.94 MB.
Listing 100 matching sessions among 1,000 fell from 11.83 ms to 2.44 ms median.
Independent writer throughput was ~395 appends/s with one process and ~266/s with
four processes; the shared database deliberately serializes storage operations.

At 100,000 messages (~105 MB canonical JSONL, ~142 MB database):

| Operation | Fixed p50 | Fixed p95 |
|---|---:|---:|
| Durable ~1 KB append | 2.15 ms | 2.71 ms |
| Metadata lookup | 1.28 ms | 1.45 ms |
| First 100-record transcript page | 1.95 ms | 2.40 ms |
| Full transcript | 358 ms | 359 ms |
| Resume | 355 ms | 359 ms |
| Resume after compaction | 362 ms | 372 ms |

Checkpoint maintenance introduces outliers: the first append after the synthetic
100 MB bulk seed took 234 ms (212 ms in the confirmation run). Large transactions
can exceed the 4 MiB WAL threshold until the next open checkpoints them. This is
not a hard per-operation latency bound or a hard instantaneous WAL-size limit.

Reading a page containing 20 native replay blocks totalling ~5 MiB took 7.6 ms
median; its safe display representation was below 10 KB. Projection currently
requires decoding the hidden native payloads too. Paging bounds rows, not bytes.

### JSONL comparison

The comparison file contains exactly the same canonical messages as the database,
including benchmark appends. At 100,000 messages, reading and decoding the entire
JSONL transcript took ~125 ms versus ~358 ms through the store. Reading raw bytes
alone took ~51 ms; that is not equivalent to constructing typed chat messages.
The first 100 JSONL messages took ~0.16 ms versus ~1.95 ms for the database's first
100 scanned records (which can include metadata). Sequential files are genuinely
faster; indexed pages remain fast enough for frontend display without loading
an entire archive. No JSONL tail-offset index was constructed or compared.

## Fixes made from these measurements

1. Allocate sequences with one indexed descending lookup per transaction and
   reuse a prepared explicit INSERT. The old wrapped MAX aggregate scanned
   history for every insert, making bulk insertion quadratic.
2. Perform checked, thresholded `wal_checkpoint(TRUNCATE)` under the operation
   lock. Turso 0.8.3's default PASSIVE checkpoints do not reclaim this
   reopen-per-operation WAL lifecycle reliably.
3. Offload the complete storage operation, including fsync and decoding, from
   CLI/ACP's current-thread executor. In-flight workers retain the session lock
   after caller cancellation; cancelled writers require reopening/replay. The
   regression probe verifies poisoning, lock retention, and eventual recovery.

## Assessment and remaining limits

The single authoritative Turso database is a sound fit for local-agent writes,
indexed discovery, and paged chat display. Appends and small reads no longer scale
with total session history in the measured range. Do not weaken FULL durability or
enable experimental multiprocess WAL merely to improve these figures.

**Resume is still linear in the full archive, even after compaction.** It rebuilds
history before applying checkpoints; it does not seek directly to the active
checkpoint. The confirmation run measured another session's append at **364 ms
p95 / 372 ms max** while three 100,000-message resumes competed for the global
lock. Whole-transcript reads and ACP's current whole-history replay have the same
head-of-line blocking risk, though the frontend executor itself is no longer
blocked by native IO.

The next architectural improvement should be an indexed active-context checkpoint
with the settings/recovery state needed to resume from its suffix, plus paged ACP
replay. Preserve the canonical records; any projection index must be transactionally
consistent and rebuildable. At very high concurrency, use bounded storage work
queues and cancellation-aware lock waits. No claim is made that the current
implementation has solved large-history resume or arbitrary concurrent workloads.
