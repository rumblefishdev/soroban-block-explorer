"""Common helpers for task 0252 Phase B/B.5/C/D compare scripts.

Design choices:
  * CH queries via `docker exec app-clickhouse-1 clickhouse-client` subprocess
    — avoids installing clickhouse-driver and matches the operator script
    style. JSONEachRow output is parsed line-by-line.
  * Horizon REST via `requests` with built-in pagination + retry on 429.
  * Per-endpoint compare writes a TSV row to OUT_DIR/phase_b_e<NN>.tsv
    and (on field mismatch) a JSON dump to OUT_DIR/diffs/<endpoint>/<key>.json.
  * Checkpointing: each script reads its TSV before running and skips
    already-completed keys. Re-running = resume from last unfinished.

Environment:
  SBE_CH_CONTAINER   default: app-clickhouse-1
  SBE_CH_USER        default: default
  SBE_CH_DB          default: default
  SBE_OUT_DIR        default: /tmp/sbe-artifacts/0252
  SBE_SAMPLE_DIR     default: same as SBE_OUT_DIR
  HORIZON_BASE       default: https://horizon.stellar.org
  SBE_HORIZON_DELAY  default: 0.35 (s) — sleep between Horizon requests
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Iterable, Iterator

import requests


CH_CONTAINER = os.environ.get("SBE_CH_CONTAINER", "app-clickhouse-1")
CH_USER = os.environ.get("SBE_CH_USER", "default")
CH_DB = os.environ.get("SBE_CH_DB", "default")
OUT_DIR = Path(os.environ.get("SBE_OUT_DIR", "/tmp/sbe-artifacts/0252"))
SAMPLE_DIR = Path(os.environ.get("SBE_SAMPLE_DIR", str(OUT_DIR)))
HORIZON_BASE = os.environ.get("HORIZON_BASE", "https://horizon.stellar.org")
HORIZON_DELAY = float(os.environ.get("SBE_HORIZON_DELAY", "0.35"))

OUT_DIR.mkdir(parents=True, exist_ok=True)
(OUT_DIR / "diffs").mkdir(exist_ok=True)


# --- CH ----------------------------------------------------------------

def ch_query(sql: str, format: str = "TabSeparated") -> str:
    """Execute SQL on Hetzner CH, return raw stdout (without trailing nl)."""
    cmd = [
        "docker", "exec", CH_CONTAINER,
        "clickhouse-client",
        f"--user={CH_USER}",
        f"--database={CH_DB}",
        f"--format={format}",
        f"--query={sql}",
    ]
    proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
    if proc.returncode != 0:
        raise RuntimeError(f"CH query failed:\n{sql}\n---\n{proc.stderr}")
    return proc.stdout.rstrip("\n")


def ch_query_json(sql: str) -> list[dict[str, Any]]:
    """Execute SQL, parse JSONEachRow output to list of dicts."""
    out = ch_query(sql, format="JSONEachRow")
    return [json.loads(line) for line in out.splitlines() if line.strip()]


# --- Horizon -----------------------------------------------------------

def horizon_get(path: str, params: dict[str, Any] | None = None,
                max_retries: int = 5) -> dict[str, Any]:
    """GET against Horizon with exponential backoff on 429 + 5xx."""
    url = f"{HORIZON_BASE}{path}"
    for attempt in range(max_retries):
        r = requests.get(url, params=params, timeout=30)
        if r.status_code == 200:
            time.sleep(HORIZON_DELAY)
            return r.json()
        if r.status_code == 429 or r.status_code >= 500:
            wait = 2 ** attempt
            time.sleep(wait)
            continue
        if r.status_code == 404:
            return {}  # treat as "not found" silently
        r.raise_for_status()
    raise RuntimeError(f"Horizon {path} failed after {max_retries} retries")


def horizon_paginate(path: str, params: dict[str, Any] | None = None,
                     max_pages: int = 20) -> Iterator[dict[str, Any]]:
    """Walk paginated `_embedded.records` until no `next` link or empty page."""
    params = dict(params or {})
    params.setdefault("limit", 200)
    params.setdefault("order", "asc")

    next_url = f"{HORIZON_BASE}{path}"
    first_params = params

    for page in range(max_pages):
        if page == 0:
            r = requests.get(next_url, params=first_params, timeout=30)
        else:
            r = requests.get(next_url, timeout=30)
        if r.status_code != 200:
            return
        body = r.json()
        records = body.get("_embedded", {}).get("records", [])
        if not records:
            return
        for rec in records:
            yield rec
        next_link = body.get("_links", {}).get("next", {}).get("href")
        if not next_link:
            return
        next_url = next_link
        time.sleep(HORIZON_DELAY)


# --- Diff + TSV --------------------------------------------------------

@dataclass
class FieldResult:
    pass_count: int = 0
    fail_count: int = 0
    tolerance_count: int = 0


@dataclass
class EndpointResult:
    endpoint: str
    sample_size: int = 0
    pass_total: int = 0
    fail_total: int = 0
    tolerance_total: int = 0
    fields: dict[str, FieldResult] = field(default_factory=dict)
    elapsed_ms: int = 0

    def record_field(self, name: str, verdict: str) -> None:
        fr = self.fields.setdefault(name, FieldResult())
        if verdict == "pass":
            fr.pass_count += 1
            self.pass_total += 1
        elif verdict == "tolerance":
            fr.tolerance_count += 1
            self.tolerance_total += 1
        else:
            fr.fail_count += 1
            self.fail_total += 1


def write_tsv_header(path: Path) -> None:
    if path.exists():
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("endpoint\tkey\tpass\ttolerance\tfail\tnote\n")


def append_tsv_row(path: Path, endpoint: str, key: str,
                   pass_n: int, tol_n: int, fail_n: int,
                   note: str = "") -> None:
    with path.open("a") as f:
        f.write(f"{endpoint}\t{key}\t{pass_n}\t{tol_n}\t{fail_n}\t{note}\n")


def read_completed_keys(path: Path, endpoint: str) -> set[str]:
    """Resume support: read existing TSV, return set of keys already done."""
    if not path.exists():
        return set()
    done = set()
    with path.open() as f:
        next(f, None)  # header
        for line in f:
            cols = line.rstrip("\n").split("\t")
            if len(cols) >= 2 and cols[0] == endpoint:
                done.add(cols[1])
    return done


def dump_diff(endpoint: str, key: str, ch_record: Any, horizon_record: Any,
              diffs: list[str]) -> None:
    """Save full diff context for offline analysis."""
    diff_dir = OUT_DIR / "diffs" / endpoint
    diff_dir.mkdir(parents=True, exist_ok=True)
    out = diff_dir / f"{key}.json"
    payload = {
        "endpoint": endpoint,
        "key": key,
        "diffs": diffs,
        "ch": ch_record,
        "horizon": horizon_record,
    }
    out.write_text(json.dumps(payload, indent=2, default=str))


# --- Sample loader -----------------------------------------------------

def load_samples(filename: str) -> list[str]:
    path = SAMPLE_DIR / filename
    if not path.exists():
        raise FileNotFoundError(
            f"Sample file {path} missing. Run sample_pools.sh first."
        )
    return [l.strip() for l in path.read_text().splitlines() if l.strip()]
