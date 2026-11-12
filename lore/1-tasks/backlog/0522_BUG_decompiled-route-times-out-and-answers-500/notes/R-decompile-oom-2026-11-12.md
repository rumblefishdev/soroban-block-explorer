# R: decompile out-of-memory class, measured 2026-11-12

A second failure class, worse than the timeout: the Lambda process dies, so
the handler's timeout never answers.

- **Production, 2026-11-11.** Seven `Runtime.OutOfMemory` REPORT lines on the
  API Lambda (256 MB), all `GET /v1/contracts/CDLPZXMJUDMMMPRYMY5GVG5CQDE33KCM74WJKFDO2B7RFPSBFDO2QKVK/decompiled`
  (X-Ray), each after 4–12 s. API Gateway answered 502. No `level=ERROR`
  line exists for them: the process was killed, so the alarm's Logs
  Insights recipe finds nothing. Found from the `AWS/Lambda Errors` metric,
  then the REPORT lines, then X-Ray for the URL. 0 OOMs in the 30 days
  before; daily peak memory there 56–92 MB.
- **The frontend cannot help.** It falls back to WAT only on a 500 whose body
  says `decompile_failed`; a 502 has no body, so the hook retries once and
  the tab shows its error state.
- **The class is the sweep's `timeout` rows.** That contract's WASM
  (`d385f66c…`, 56 KB) is one of the 8 hashes 0465's
  `benchmark/results.csv` marks `timeout` (of 3,326). Locally, on the same
  `soroban-ret` 0.0.4 and the same call (`decompile_with_options`, default
  options), sampled every 0.5 s on macOS:

  | Hash       | WASM   | First sample > 256 MB | Peak in 16 s |
  | ---------- | ------ | --------------------- | ------------ |
  | `d385f66c` | 56 KB  | 1 s                   | 3.1 GB       |
  | `84a3f14a` | 62 KB  | 1 s                   | 2.9 GB       |
  | `6fc60b27` | 51 KB  | 1 s                   | 2.9 GB       |
  | `04a8eda2` | 66 KB  | 1 s                   | 2.2 GB       |
  | `1d64fdb0` | 74 KB  | 2 s                   | 1.9 GB       |
  | `7d342470` | 48 KB  | 15 s                  | 315 MB       |
  | `4d24790f` | 129 KB | never                 | 185 MB       |
  | `e4860606` | 129 KB | never                 | 172 MB       |

  Six hashes (8 contracts) kill a 256 MB Lambda; two (4 contracts) stay
  small and hit the 10 s timeout, which already ends in WAT. `d385f66c` ran
  180 s at 0.3–1.5 GB without a result. More memory does not fix it.

- **Where it blows up.** `spec_only: true` (signatures, no bodies) finishes
  all 8 in 0.24–0.59 s at 5–7 MB, so the growth is in function-body
  lifting. `mode: Generic` blows up the same way. `wasm_to_wat` on
  `d385f66c` takes 8 ms.
- **Five more slow hashes.** The sweep's `ok` rows above 10 s (`6bcd873c`,
  `7bed311b`, `baf978f1`, `df06cfa8`, `df8fc2ba`, 10–104 s there) did not
  finish in 40 s locally. Four stayed at 20–42 MB; `df06cfa8` passed 256 MB
  at 11 s (777 MB at 40 s), so it crosses the limit only after the 10 s
  timeout has answered. The class is 13 hashes: 7 cross 256 MB.
- **Not covered:** hashes uploaded after the 0465 sweep; why the five ran
  slower here than in the sweep; Lambda's allocator
  (macOS figures are an order of magnitude, not exact).
- **No budget upstream.** `DecompileOptions` in 0.0.4 has `spec_only`,
  `pre_optimize`, `mode`, `hints` and no time, memory or fuel limit; on
  stable Rust an allocation failure aborts the process, so an in-process
  allocator budget cannot turn it into an error. Bounding it means a
  separate process or function.
