# Operator session — 2026-10-02

- ACM certificate for the testnet API host (issued, validation record kept).
- Three Lambda client certs (`lambda-{api,ingestion,enrichment}-testnet`) in
  Secrets Manager under `soroban/testnet/mtls/`, tagged; their CN pairs in
  the operator env secret and in the Caddy map.
- Slack: own alarm channel with the AWS app invited; workspace and channel ids
  under `/soroban-explorer/testnet/`.
- Hetzner `--tags app` from a clean worktree at `e1717a626`: `testnet_*`
  users and quotas live, database `testnet` holds the same 40 objects as
  `default`, task 0396's pending `dict.xml` removal shipped with it. The
  ClickHouse recreate took ~30 s; mainnet kept every ledger (no gap).

## Lessons for the next Hetzner run

- Take the operator env from its Secrets Manager secret, not a laptop copy: a
  copy from 2026-09-10 lacked a CN pair added by hand on 2026-09-21, and the
  Caddy map render would have dropped it.
- The CA key is in Secrets Manager; on macOS stage it on a RAM disk
  (`hdiutil` + `diskutil erasevolume`), since there is no `/dev/shm`.
- From a worktree, point `ANSIBLE_COLLECTIONS_PATH` at the main checkout's
  `infra-hetzner/ansible/collections`; the worktree has none and the pipx
  Ansible would fall back to its bundled versions.
- The box shares ClickHouse with another tenant: a recreate goes in a window
  agreed with them, between steps of anything they run.
