//! The capped TSV dumps `snapshot-seed` writes beside its summary: the row sets
//! that invent an entity rather than restate a holding. Split out of `seed.rs`
//! by topic (module size limit).

use std::path::Path;

use db_clickhouse::persist::rows::{AccountEntryStateRow, AccountRow, AssetRow};

use crate::error::BackfillError;
use crate::snapshot::network_state::NetworkState;

/// Cap on the two row sets too large to dump whole. Truncation is always
/// stated in the file itself — a dump that silently stops reads as a complete
/// one to whoever audits it.
const DUMP_CAP: usize = 5_000;

/// Write `lines` to `dir/name`, capped, with the cut recorded in the file.
fn write_dump(
    dir: &Path,
    name: &str,
    total: usize,
    lines: impl Iterator<Item = String>,
) -> Result<(), BackfillError> {
    let mut out: Vec<String> = lines.collect();
    if total > out.len() {
        out.push(format!("# TRUNCATED — {} of {total} rows shown", out.len()));
    }
    let path = dir.join(name);
    std::fs::write(&path, out.join("\n") + "\n")
        .map_err(|e| BackfillError::Incomplete(format!("write {}: {e}", path.display())))?;
    println!(
        "    wrote {} of {total} rows -> {}",
        out.len(),
        path.display()
    );
    Ok(())
}

/// Dump the three row sets the verdict samples never covered: the ones that
/// invent an ENTITY rather than restate a holding. A wrong amount on a real
/// asset is visible to anyone who looks the asset up; an asset that does not
/// exist on chain is not, because nobody knows to look for it. Asset stubs are
/// therefore dumped WHOLE — they are the smallest set and the only one that
/// writes a new row into a dimension table.
///
/// The two capped dumps take an ARBITRARY prefix, not the deterministic
/// bottom-k the verdict samples use: both vectors are built by iterating a
/// `HashMap`, whose order is per-process, so these files are not comparable
/// across runs. They exist to be eyeballed and chain-checked, not diffed.
pub(crate) fn write_correction_dumps(
    dir: &Path,
    asset_stubs: &[AssetRow],
    account_stubs: &[AccountRow],
    entry_states: &[AccountEntryStateRow],
    state: &NetworkState,
) -> Result<(), BackfillError> {
    write_dump(
        dir,
        "asset_stubs.tsv",
        asset_stubs.len(),
        asset_stubs.iter().map(|a| {
            // The registry issuer is the StrKey the surrogate was derived from;
            // printing both lets an audit recompute `credit_asset_id` offline.
            let issuer = state
                .asset_registry
                .get(&a.id)
                .map_or("?", |(_, issuer)| issuer.as_str());
            format!("{}\t{}\t{}\t{}", a.asset_code, issuer, a.id, a.issuer_id)
        }),
    )?;
    write_dump(
        dir,
        "account_stubs.tsv",
        account_stubs.len(),
        account_stubs
            .iter()
            .take(DUMP_CAP)
            .map(|a| format!("{}\t{}\t{}", a.account_id, a.id, a.first_seen_ledger)),
    )?;
    write_dump(
        dir,
        "entry_states.tsv",
        entry_states.len(),
        entry_states.iter().take(DUMP_CAP).map(|s| {
            // The StrKey, not the surrogate: a signer set is audited by asking
            // the chain for the account, which needs the G-address.
            let who = state
                .account_details
                .get(&s.account_id)
                .map_or("?", |d| d.strkey.as_str());
            format!(
                "{who}\t{}/{}/{}/{}\t{}\t{}",
                s.master_weight,
                s.threshold_low,
                s.threshold_med,
                s.threshold_high,
                s.signer_keys.join(","),
                s.signer_weights
                    .iter()
                    .map(u32::to_string)
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }),
    )
}
