//! Reconciling our tables against the Stellar network's own state.
//!
//! One subcommand, `snapshot-seed`, and the five layers it is built from —
//! read them in this order, because it is the order the data moves:
//!
//! | module            | concern                                             |
//! | ----------------- | --------------------------------------------------- |
//! | [`archive`]       | the SDF history archive: manifest, buckets, framed   |
//! |                   | XDR, per-bucket SHA-256. Knows nothing about us.     |
//! | [`network_state`] | what those records MEAN: classification into our key |
//! |                   | space, first-wins dedup into one live register.      |
//! | [`verdict`]       | the comparison rule — one of our rows against one    |
//! |                   | network holding. Pure, no I/O, eleven outcomes.      |
//! | [`report`]        | counters, samples, `summary.txt` — the document an   |
//! |                   | operator signs off on before anything is written.    |
//! | [`seed`]          | the command: reads our `balances`, builds the        |
//! |                   | corrections, and — only with `--execute` — inserts.  |
//!
//! Everything above [`seed`] is read-only by construction. Task 0502 extracts
//! [`archive`] + [`network_state`] into their own crate. Only [`archive`] is
//! genuinely schema-free: [`network_state`] classifies INTO our key space and
//! calls `db_clickhouse::persist::ids` to do it. That is a small, pure hash
//! module which travels with the extraction, so the seam still falls here —
//! but "knows nothing about our schema" was true of one module, not two.

pub mod archive;
pub mod network_state;
pub mod report;
pub mod seed;
pub mod seed_lp;
pub mod verdict;
