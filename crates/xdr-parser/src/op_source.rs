//! Per-contract op-source extraction for the deployer attribution path.
//!
//! Per Stellar protocol, the deployer of a Soroban contract is the account
//! that authorized the `CreateContract*` host function — either as the
//! operation's effective source (op override OR tx source fallback) for a
//! top-level deploy, or as the signer of the enclosing
//! `SorobanAuthorizationEntry` for a factory-pattern deploy nested inside
//! an `InvokeContract` auth tree.
//!
//! `crate::state::extract_contract_deployments` historically stored the
//! inner-tx source as `deployer_account` unconditionally, which yields a
//! wrong answer in ~12 % of mainnet deploys whose op overrides the tx
//! source or whose deploy is nested in a factory call. This helper
//! produces the per-contract override map that
//! `extract_contract_deployments` consumes as the authoritative deployer
//! (with fallback to tx source preserved for the 88 % of deploys where
//! op inherits tx source).
//!
//! Task 0255 Phase 1.

use stellar_xdr::curr::{
    ContractIdPreimage, CreateContractArgs, CreateContractArgsV2, HostFunction, OperationBody,
    ScAddress, SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials,
};

use crate::envelope::{InnerTxRef, muxed_to_g_strkey};
use crate::sac::derive_sac_contract_id;

/// Collect `(contract_id, deployer_strkey)` pairs reachable from every
/// `InvokeHostFunction` operation in a single inner transaction.
///
/// Two production surfaces:
///
/// 1. **Top-level `CreateContract` / `CreateContractV2`** — deployer is the
///    operation's effective source (`op.source_account.or(tx_source)`).
///    Captures the common per-op-source-override case that motivated the
///    bug fix.
/// 2. **Auth-tree `CreateContractHostFn` / `CreateContractV2HostFn`**
///    (factory pattern) — deployer is the signer of the enclosing
///    `SorobanAuthorizationEntry`. The signer is derived from the entry's
///    credentials:
///      - `SourceAccount` → effective op source (op override OR tx source)
///      - `Address(Account)` → the explicit account StrKey
///      - `Address(Contract)` → contract-signed; skipped (no human deployer)
///
/// `network_id` is required because `contract_id` is derived deterministically
/// from `ContractIdPreimage` per stellar-core's
/// `SHA256(XDR(HashIdPreimage::ContractId{ network_id, preimage }))`.
/// See `crate::sac::derive_sac_contract_id` — same derivation works for
/// non-SAC preimages (`Address(salt)`) because the hash input only cares
/// about the preimage XDR, not the variant.
///
/// Mirrors the shape of `crate::sac::extract_sac_identities` and is
/// expected to be `collect()`-ed into a `HashMap<String, String>` at the
/// indexer call site — see `crates/indexer/src/handler/process.rs`.
pub fn extract_op_source_per_contract(
    envelope: &InnerTxRef<'_>,
    tx_source: &str,
    network_id: &[u8; 32],
) -> Vec<(String, String)> {
    let ops = match envelope {
        InnerTxRef::V0(tx) => tx.operations.as_slice(),
        InnerTxRef::V1(tx) => tx.operations.as_slice(),
    };

    let mut out = Vec::new();
    for op in ops {
        let effective_source = op
            .source_account
            .as_ref()
            .map(muxed_to_g_strkey)
            .unwrap_or_else(|| tx_source.to_string());

        let OperationBody::InvokeHostFunction(ref invoke) = op.body else {
            continue;
        };

        match &invoke.host_function {
            HostFunction::CreateContract(args) => {
                push_preimage_deployer(
                    &args.contract_id_preimage,
                    &effective_source,
                    network_id,
                    &mut out,
                );
            }
            HostFunction::CreateContractV2(args) => {
                push_preimage_deployer(
                    &args.contract_id_preimage,
                    &effective_source,
                    network_id,
                    &mut out,
                );
            }
            _ => {}
        }

        for auth_entry in invoke.auth.iter() {
            let Some(auth_signer) = credentials_signer(&auth_entry.credentials, &effective_source)
            else {
                continue;
            };
            walk_auth_for_creates(
                &auth_entry.root_invocation,
                &auth_signer,
                network_id,
                &mut out,
            );
        }
    }

    out
}

/// Resolve the human signer of a `SorobanAuthorizationEntry` for deployer
/// attribution. Returns `None` for contract-signed credentials (no human
/// deployer) and for non-account address shapes (claimable balance, liquidity
/// pool) which cannot sign Soroban auth entries in practice but appear in
/// the `ScAddress` enum.
fn credentials_signer(creds: &SorobanCredentials, effective_op_source: &str) -> Option<String> {
    match creds {
        SorobanCredentials::SourceAccount => Some(effective_op_source.to_string()),
        SorobanCredentials::Address(addr_creds) => match &addr_creds.address {
            ScAddress::Account(account_id) => Some(account_id.0.to_string()),
            // Muxed-account signer: the underlying ed25519 IS the signer
            // (the 8-byte muxing id is a recipient-side memo, not a key).
            // Canonicalise to the bare G-strkey to match the
            // `accounts.account_id` shape per ADR 0026.
            ScAddress::MuxedAccount(med) => {
                Some(stellar_xdr::curr::MuxedAccount::Ed25519(med.ed25519.clone()).to_string())
            }
            ScAddress::Contract(_)
            | ScAddress::ClaimableBalance(_)
            | ScAddress::LiquidityPool(_) => None,
        },
    }
}

/// Walk an auth invocation tree, pushing `(contract_id, signer)` for every
/// `CreateContractHostFn` / `CreateContractV2HostFn` node. Mirrors
/// `crate::sac::walk_auth_node` but emits the auth signer rather than the
/// SAC asset identity.
fn walk_auth_for_creates(
    node: &SorobanAuthorizedInvocation,
    signer: &str,
    network_id: &[u8; 32],
    out: &mut Vec<(String, String)>,
) {
    match &node.function {
        SorobanAuthorizedFunction::CreateContractHostFn(CreateContractArgs {
            contract_id_preimage,
            ..
        }) => push_preimage_deployer(contract_id_preimage, signer, network_id, out),
        SorobanAuthorizedFunction::CreateContractV2HostFn(CreateContractArgsV2 {
            contract_id_preimage,
            ..
        }) => push_preimage_deployer(contract_id_preimage, signer, network_id, out),
        SorobanAuthorizedFunction::ContractFn(_) => {}
    }
    for child in node.sub_invocations.iter() {
        walk_auth_for_creates(child, signer, network_id, out);
    }
}

/// Derive the deterministic `contract_id` StrKey from a preimage and push
/// the `(contract_id, deployer)` pair. Derivation failures are logged via
/// `tracing::warn!` and the pair is dropped — a malformed preimage must
/// not poison the parser, but the event needs to be observable in
/// production so a regression in the XDR layer surfaces in logs rather
/// than as a quiet data-quality drift.
fn push_preimage_deployer(
    preimage: &ContractIdPreimage,
    deployer: &str,
    network_id: &[u8; 32],
    out: &mut Vec<(String, String)>,
) {
    match derive_sac_contract_id(preimage, network_id) {
        Ok(contract_id) => out.push((contract_id, deployer.to_string())),
        Err(e) => tracing::warn!(
            target: "xdr_parser::op_source",
            error = %e.message,
            "derive contract_id failed for op-source deployer attribution",
        ),
    }
}

#[cfg(test)]
mod tests {
    //! Coverage for task 0255 Phase 1 — deployer attribution must read the
    //! op-level effective source (op.source_account override OR tx source)
    //! for top-level `CreateContract*` ops, and the auth-entry credentials
    //! signer for factory-pattern `CreateContractHostFn` nested in an
    //! `InvokeContract` auth tree. Fee-bump envelopes must unwrap to the
    //! inner tx; the outer `fee_source` must never reach the deployer slot.
    use super::*;
    use crate::envelope::inner_transaction;
    use crate::sac::{MAINNET_PASSPHRASE, derive_sac_contract_id, network_id};
    use stellar_xdr::curr::ScVal;
    use stellar_xdr::curr::{
        AccountId, Asset, ContractExecutable, ContractId, CreateContractArgs, FeeBumpTransaction,
        FeeBumpTransactionEnvelope, FeeBumpTransactionExt, FeeBumpTransactionInnerTx, Hash,
        HostFunction, InvokeContractArgs, InvokeHostFunctionOp, Memo, MuxedAccount, Operation,
        OperationBody, Preconditions, PublicKey, ScAddress, ScSymbol, SequenceNumber,
        SorobanAddressCredentials, SorobanAuthorizationEntry, SorobanAuthorizedFunction,
        SorobanAuthorizedInvocation, SorobanCredentials, Transaction, TransactionEnvelope,
        TransactionExt, TransactionV1Envelope, Uint256, VecM,
    };

    fn g_strkey(payload: [u8; 32]) -> String {
        MuxedAccount::Ed25519(Uint256(payload)).to_string()
    }

    fn account_id(payload: [u8; 32]) -> AccountId {
        AccountId(PublicKey::PublicKeyTypeEd25519(Uint256(payload)))
    }

    /// XLM-SAC preimage. Mainnet derivation is the canonical
    /// `CAS3J7GY...XOWMA` published across Horizon, SDK, stellar.expert.
    /// We reuse it here so test assertions are anchored to a value the
    /// rest of the suite (sac.rs) already pins.
    fn xlm_preimage() -> ContractIdPreimage {
        ContractIdPreimage::Asset(Asset::Native)
    }

    fn expected_xlm_contract_id(net: &[u8; 32]) -> String {
        derive_sac_contract_id(&xlm_preimage(), net).expect("XLM-SAC contract_id")
    }

    fn build_tx_with_op(op: Operation, tx_source_payload: [u8; 32]) -> Transaction {
        Transaction {
            source_account: MuxedAccount::Ed25519(Uint256(tx_source_payload)),
            fee: 100,
            seq_num: SequenceNumber(1),
            cond: Preconditions::None,
            memo: Memo::None,
            operations: vec![op].try_into().unwrap(),
            ext: TransactionExt::V0,
        }
    }

    fn top_level_create_contract_op(op_source_payload: Option<[u8; 32]>) -> Operation {
        Operation {
            source_account: op_source_payload.map(|p| MuxedAccount::Ed25519(Uint256(p))),
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::CreateContract(CreateContractArgs {
                    contract_id_preimage: xlm_preimage(),
                    executable: ContractExecutable::StellarAsset,
                }),
                auth: VecM::default(),
            }),
        }
    }

    fn factory_invoke_op(
        op_source_payload: Option<[u8; 32]>,
        auth_credentials: SorobanCredentials,
    ) -> Operation {
        let nested_create = SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::CreateContractHostFn(CreateContractArgs {
                contract_id_preimage: xlm_preimage(),
                executable: ContractExecutable::StellarAsset,
            }),
            sub_invocations: VecM::default(),
        };
        let factory_root = SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: ScAddress::Contract(ContractId(Hash([0xFA; 32]))),
                function_name: ScSymbol::try_from(b"deploy_pair".to_vec()).unwrap(),
                args: VecM::default(),
            }),
            sub_invocations: vec![nested_create].try_into().unwrap(),
        };
        let auth = SorobanAuthorizationEntry {
            credentials: auth_credentials,
            root_invocation: factory_root,
        };
        Operation {
            source_account: op_source_payload.map(|p| MuxedAccount::Ed25519(Uint256(p))),
            body: OperationBody::InvokeHostFunction(InvokeHostFunctionOp {
                host_function: HostFunction::InvokeContract(InvokeContractArgs {
                    contract_address: ScAddress::Contract(ContractId(Hash([0xFA; 32]))),
                    function_name: ScSymbol::try_from(b"deploy_pair".to_vec()).unwrap(),
                    args: VecM::default(),
                }),
                auth: vec![auth].try_into().unwrap(),
            }),
        }
    }

    /// Case 1 — plain top-level `CreateContract` with no per-op source
    /// override. The deployer is the inner-tx source (the call site
    /// passes `tx_source` as fallback). This is the ~88 % mainnet shape
    /// that the original parser handled "correctly by accident".
    #[test]
    fn top_level_create_contract_without_override_uses_tx_source() {
        let tx_source_payload = [0xAA; 32];
        let tx = build_tx_with_op(top_level_create_contract_op(None), tx_source_payload);
        let inner = InnerTxRef::V1(&tx);
        let net = network_id(MAINNET_PASSPHRASE);

        let pairs = extract_op_source_per_contract(&inner, &g_strkey(tx_source_payload), &net);

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, expected_xlm_contract_id(&net));
        assert_eq!(
            pairs[0].1,
            g_strkey(tx_source_payload),
            "no op override → deployer falls through to tx source"
        );
    }

    /// Case 2 — the bug case. `op.source_account` overrides the tx source;
    /// the helper must emit the OP source, NOT the tx source. This is the
    /// 12 % mainnet shape that the original parser mis-attributed.
    #[test]
    fn top_level_create_contract_with_op_override_uses_op_source() {
        let tx_source_payload = [0xAA; 32];
        let op_source_payload = [0xBB; 32];
        let tx = build_tx_with_op(
            top_level_create_contract_op(Some(op_source_payload)),
            tx_source_payload,
        );
        let inner = InnerTxRef::V1(&tx);
        let net = network_id(MAINNET_PASSPHRASE);

        let pairs = extract_op_source_per_contract(&inner, &g_strkey(tx_source_payload), &net);

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].0, expected_xlm_contract_id(&net));
        assert_eq!(
            pairs[0].1,
            g_strkey(op_source_payload),
            "op.source_account override wins over tx source"
        );
        assert_ne!(pairs[0].1, g_strkey(tx_source_payload));
    }

    /// Case 3 — factory pattern. Top-level `InvokeContract` carries an
    /// auth entry with `SorobanCredentials::SourceAccount`; the nested
    /// `CreateContractHostFn` is signed by the effective op source. With
    /// an op-source override, the deployer is that override (not the
    /// tx source).
    #[test]
    fn factory_source_account_credentials_uses_effective_op_source() {
        let tx_source_payload = [0xAA; 32];
        let op_source_payload = [0xBB; 32];
        let tx = build_tx_with_op(
            factory_invoke_op(Some(op_source_payload), SorobanCredentials::SourceAccount),
            tx_source_payload,
        );
        let inner = InnerTxRef::V1(&tx);
        let net = network_id(MAINNET_PASSPHRASE);

        let pairs = extract_op_source_per_contract(&inner, &g_strkey(tx_source_payload), &net);

        assert_eq!(pairs.len(), 1, "nested CreateContractHostFn discovered");
        assert_eq!(pairs[0].0, expected_xlm_contract_id(&net));
        assert_eq!(
            pairs[0].1,
            g_strkey(op_source_payload),
            "SourceAccount credentials → effective op source signs the deploy"
        );
    }

    /// Case 4 — factory pattern with explicit account credentials. The
    /// auth entry's credentials are `Address(Account(D))`; deployer = D
    /// regardless of tx/op source. Captures the "factory deploy signed
    /// by a third account" shape.
    #[test]
    fn factory_address_account_credentials_uses_credentials_account() {
        let tx_source_payload = [0xAA; 32];
        let op_source_payload = [0xBB; 32];
        let signer_payload = [0xCC; 32];
        let creds = SorobanCredentials::Address(SorobanAddressCredentials {
            address: ScAddress::Account(account_id(signer_payload)),
            nonce: 0,
            signature_expiration_ledger: 0,
            signature: ScVal::Void,
        });
        let tx = build_tx_with_op(
            factory_invoke_op(Some(op_source_payload), creds),
            tx_source_payload,
        );
        let inner = InnerTxRef::V1(&tx);
        let net = network_id(MAINNET_PASSPHRASE);

        let pairs = extract_op_source_per_contract(&inner, &g_strkey(tx_source_payload), &net);

        assert_eq!(pairs.len(), 1);
        assert_eq!(
            pairs[0].1,
            g_strkey(signer_payload),
            "Address(Account) credentials → that account signs the deploy"
        );
        assert_ne!(pairs[0].1, g_strkey(op_source_payload));
        assert_ne!(pairs[0].1, g_strkey(tx_source_payload));
    }

    /// Case 5 — factory deploy auth'd by a contract address. No human
    /// signer to attribute to; the helper skips the pair so the
    /// downstream fallback (tx source) lands on `soroban_contracts.deployer_id`.
    #[test]
    fn factory_address_contract_credentials_skipped() {
        let tx_source_payload = [0xAA; 32];
        let creds = SorobanCredentials::Address(SorobanAddressCredentials {
            address: ScAddress::Contract(ContractId(Hash([0xDD; 32]))),
            nonce: 0,
            signature_expiration_ledger: 0,
            signature: ScVal::Void,
        });
        let tx = build_tx_with_op(factory_invoke_op(None, creds), tx_source_payload);
        let inner = InnerTxRef::V1(&tx);
        let net = network_id(MAINNET_PASSPHRASE);

        let pairs = extract_op_source_per_contract(&inner, &g_strkey(tx_source_payload), &net);

        assert!(
            pairs.is_empty(),
            "contract-signed credentials produce no deployer attribution; \
             call-site fallback applies tx source"
        );
    }

    /// Case 6 — fee-bump envelope. fee_source ≠ inner tx source ≠ op
    /// source. After `inner_transaction(env)` unwraps the inner tx, the
    /// op-level override must reach the deployer slot; the outer fee
    /// payer must NEVER appear. Anchors the real mainnet CB5GADAT…
    /// shape that motivated the bug report.
    #[test]
    fn fee_bump_unwraps_to_inner_then_op_source_wins() {
        let fee_source_payload = [0xFF; 32];
        let inner_source_payload = [0xAA; 32];
        let op_source_payload = [0xBB; 32];

        let inner_tx = build_tx_with_op(
            top_level_create_contract_op(Some(op_source_payload)),
            inner_source_payload,
        );
        let env = TransactionEnvelope::TxFeeBump(FeeBumpTransactionEnvelope {
            tx: FeeBumpTransaction {
                fee_source: MuxedAccount::Ed25519(Uint256(fee_source_payload)),
                fee: 200,
                inner_tx: FeeBumpTransactionInnerTx::Tx(TransactionV1Envelope {
                    tx: inner_tx,
                    signatures: VecM::default(),
                }),
                ext: FeeBumpTransactionExt::V0,
            },
            signatures: VecM::default(),
        });

        let inner = inner_transaction(&env);
        let tx_source = inner.source_account();
        assert_eq!(
            tx_source,
            g_strkey(inner_source_payload),
            "inner_transaction unwraps fee-bump to inner tx source"
        );
        let net = network_id(MAINNET_PASSPHRASE);
        let pairs = extract_op_source_per_contract(&inner, &tx_source, &net);

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].1, g_strkey(op_source_payload));
        assert_ne!(pairs[0].1, g_strkey(inner_source_payload));
        assert_ne!(
            pairs[0].1,
            g_strkey(fee_source_payload),
            "fee_source must never reach the deployer slot",
        );
    }
}
