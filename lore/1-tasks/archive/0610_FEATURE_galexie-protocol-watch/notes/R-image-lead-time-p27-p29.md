# Why the image: lead time of the first Galexie with the new core (2026-10-06)

**Why the image, not the network's readiness (2026-10-06).** The first
version compared our core with Horizon's `core_supported_protocol_version`.
A review found its one page said "the network is ready" while the moment
someone can act — the image appearing — reached no one. The image date is
that moment, and it led every measured vote (first image with the new core
on Docker Hub, read from the registry 2026-10-06):

| Protocol | First image with the new core                       | Pubnet vote              | Lead       |
| -------- | --------------------------------------------------- | ------------------------ | ---------- |
| 27       | 2026-06-10 (`c97d648`, `27.0.0`)                    | 2026-07-08               | ~4 weeks   |
| 28       | 2026-08-03 (`e746a5b`, pre-release; `28.0.0` 08-14) | mid-September (estimate) | ~4–6 weeks |
| 29       | 2026-09-24 (`c927ffc`, later tagged `29.0.0`)       | 2026-10-01               | 7 days     |

The Galexie 29 GitHub release (`galexie-v29.0.0`) was published
2026-10-01 20:32 UTC and the `29.0.0` Docker Hub tag 20:38 UTC, both after
the 17:00 UTC vote; only commit tag `c927ffc` (2026-09-24, the commit
"Bump stellar-core to 29.0.0 for Protocol 29") came before it.
