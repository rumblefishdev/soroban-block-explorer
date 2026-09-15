//! Invocation tree extraction from Soroban transaction metadata.
//!
//! Two complementary sources feed `ExtractedInvocation`:
//!
//! 1. **Auth entries** (`SorobanAuthorizationEntry.root_invocation` in the
//!    transaction envelope) — the authorization call graph. Always available
//!    on Soroban transactions, structured (depth + nested args + per-node
//!    function name), but a *subset* of execution: invocations that do not
//!    require caller authorization (read-only sub-calls, contract-authority
//!    sub-calls in DeFi routers) are missing. Used to build the JSON
//!    `operation_tree` returned to the API for the transaction detail page,
//!    where rich per-node detail matters.
//!
//! 2. **Diagnostic events** (`fn_call` / `fn_return` host-VM trace entries
//!    in `*.diagnostic_events`) — the *execution* call graph. Galexie
//!    captive-core emits diagnostic mode by default, so this stream is
//!    reliably present in our ingest. Walked by
//!    [`extract_invocations_from_diagnostics`] and merged into the flat
//!    `ExtractedInvocation` rows that feed the
//!    `soroban_invocations_appearances` appearance index. Closes the
//!    auth-tree coverage gap (~53 % of Soroban tx had zero rows on a local
//!    100-ledger sample — task 0183).
//!
//! Per-row detail (function name, args, return value) for the diagnostic
//! tree is intentionally not persisted — ADR 0029 routes detail to the
//! public archive; the appearance index is goal-of-coverage only.

use serde_json::{Value, json};
use stellar_xdr::*;

use crate::envelope::{InnerTxRef, muxed_to_g_strkey};
use crate::scval::scval_to_typed_json;
use crate::types::ExtractedInvocation;

/// Result of invocation tree extraction.
pub struct InvocationResult {
    /// Flat invocation rows aggregated at indexer staging into
    /// `soroban_invocations_appearances` (ADR 0034).
    pub invocations: Vec<ExtractedInvocation>,
    /// Nested JSON hierarchy for `transactions.operation_tree`.
    /// `None` if the transaction has no Soroban auth entries.
    pub operation_tree: Option<Value>,
}

/// Extract the invocation tree from a transaction envelope's auth entries
/// and execution diagnostics.
///
/// Produces:
/// * `invocations` — flat depth-first rows. When the meta carries diagnostic
///   `fn_call` / `fn_return` events, the **execution** tree (full coverage,
///   contract-to-contract callers included) is the source. Otherwise falls
///   back to the auth-entry tree (subset of execution, but always
///   available). This is what feeds the appearance index.
/// * `operation_tree` — nested JSON hierarchy for `transactions.operation_tree`,
///   built from auth entries only. Per-node function args and return values
///   come straight from XDR, which the host-VM trace does not preserve in a
///   recoverable form for the API consumer. Auth-tree shape is the
///   established read-time contract for the transaction detail page; coverage
///   gaps for that surface are out of scope (ADR 0029 keeps detail in the
///   public archive).
///
/// `successful` is derived from the parent transaction's success status.
/// `tx_meta` is used both to populate the root invocation's `return_value`
/// from `SorobanTransactionMeta` and to surface diagnostic events; pass
/// `None` if not available (auth-tree-only path).
pub fn extract_invocations(
    envelope: &InnerTxRef<'_>,
    tx_meta: Option<&TransactionMeta>,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
    tx_source_account: &str,
    successful: bool,
) -> InvocationResult {
    let ops = match envelope {
        InnerTxRef::V0(tx) => tx.operations.as_slice(),
        InnerTxRef::V1(tx) => tx.operations.as_slice(),
    };

    let root_return_value = tx_meta
        .and_then(soroban_return_value)
        .map(|v| scval_to_typed_json(&v))
        .unwrap_or(Value::Null);

    // Always build the JSON `operation_tree` from auth entries. The diagnostic
    // execution tree carries fewer per-node fields (no nested args object the
    // way auth entries do), and the operation_tree shape is already the
    // contract for `transactions.operation_tree`.
    let mut trees = Vec::new();
    for op in ops {
        if let OperationBody::InvokeHostFunction(ref invoke_op) = op.body {
            for auth_entry in invoke_op.auth.iter() {
                let tree_json = invocation_to_json(
                    &auth_entry.root_invocation,
                    root_return_value.clone(),
                    successful,
                );
                trees.push(tree_json);
            }
        }
    }
    let operation_tree = if trees.is_empty() {
        None
    } else {
        Some(json!(trees))
    };

    // Diagnostic-event execution tree (preferred — superset of auth tree).
    // When the meta carries no diagnostic events, fall back to the auth tree.
    //
    // Effective root caller honours per-op `source_account` overrides (matching
    // `flatten_auth_tree`'s task-0177 canonicalisation): muxed M-strkey →
    // underlying ed25519 G-strkey, op override beats tx source. Protocol 21+
    // allows at most one InvokeHostFunction op per tx, so the first match
    // covers every real-world case; if none exists, the tx source is the
    // legitimate fallback.
    let root_caller = ops
        .iter()
        .find_map(|op| match op.body {
            OperationBody::InvokeHostFunction(_) => Some(
                op.source_account
                    .as_ref()
                    .map(muxed_to_g_strkey)
                    .unwrap_or_else(|| tx_source_account.to_string()),
            ),
            _ => None,
        })
        .unwrap_or_else(|| tx_source_account.to_string());

    let diag_invocations = tx_meta
        .map(|tm| {
            extract_invocations_from_diagnostics(
                tm,
                transaction_hash,
                ledger_sequence,
                created_at,
                &root_caller,
                successful,
            )
        })
        .unwrap_or_default();

    let invocations = if diag_invocations.is_empty() {
        flatten_auth_tree(
            ops,
            tx_source_account,
            transaction_hash,
            ledger_sequence,
            created_at,
            successful,
            root_return_value,
        )
    } else {
        diag_invocations
    };

    InvocationResult {
        invocations,
        operation_tree,
    }
}

/// Auth-tree fallback path. Same shape as the original pre-task-0183
/// `extract_invocations` body — preserved as-is for transactions that have
/// no diagnostic events at all (degenerate/Protocol-22 cases). Auth-tree
/// coverage matches its long-standing semantic: subset of execution, root
/// caller is the per-op source account.
fn flatten_auth_tree(
    ops: &[Operation],
    tx_source_account: &str,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
    successful: bool,
    root_return_value: Value,
) -> Vec<ExtractedInvocation> {
    let mut ctx = FlattenCtx {
        transaction_hash,
        ledger_sequence,
        created_at,
        successful,
        index: 0,
    };
    let mut out = Vec::new();
    for op in ops {
        if let OperationBody::InvokeHostFunction(ref invoke_op) = op.body {
            // Per-op source_account overrides the tx source (same as extract_operations).
            // Canonicalize muxed → underlying ed25519 G-strkey so callers see the
            // same 56-char form they'd see for a non-muxed source. See task 0177.
            let caller = op
                .source_account
                .as_ref()
                .map(muxed_to_g_strkey)
                .unwrap_or_else(|| tx_source_account.to_string());

            for auth_entry in invoke_op.auth.iter() {
                flatten_invocation(
                    &mut ctx,
                    &auth_entry.root_invocation,
                    Some(caller.clone()),
                    root_return_value.clone(),
                    &mut out,
                );
            }
        }
    }
    out
}

/// Shared context for invocation flattening.
struct FlattenCtx<'a> {
    transaction_hash: &'a str,
    ledger_sequence: u32,
    created_at: i64,
    successful: bool,
    index: u32,
}

/// Flatten an invocation tree into `ExtractedInvocation` rows using iterative DFS.
///
/// Uses an explicit stack to avoid stack overflow on deep auth trees
/// (XDR depth limit allows up to ~1000 levels).
fn flatten_invocation(
    ctx: &mut FlattenCtx<'_>,
    root: &SorobanAuthorizedInvocation,
    root_caller: Option<String>,
    root_return_value: Value,
    out: &mut Vec<ExtractedInvocation>,
) {
    struct Frame<'a> {
        node: &'a SorobanAuthorizedInvocation,
        depth: u32,
        caller_account: Option<String>,
        return_value: Value,
    }

    let mut stack = vec![Frame {
        node: root,
        depth: 0,
        caller_account: root_caller,
        return_value: root_return_value,
    }];

    while let Some(frame) = stack.pop() {
        let (contract_id, function_name, function_args) =
            decode_authorized_function(&frame.node.function);

        out.push(ExtractedInvocation {
            transaction_hash: ctx.transaction_hash.to_string(),
            contract_id: contract_id.clone(),
            caller_account: frame.caller_account,
            function_name,
            function_args,
            return_value: frame.return_value,
            successful: ctx.successful,
            invocation_index: ctx.index,
            depth: frame.depth,
            ledger_sequence: ctx.ledger_sequence,
            created_at: ctx.created_at,
        });

        ctx.index += 1;

        // Push children in reverse so left-to-right DFS order is preserved on pop.
        for child in frame.node.sub_invocations.iter().rev() {
            stack.push(Frame {
                node: child,
                depth: frame.depth + 1,
                caller_account: contract_id.clone(),
                return_value: Value::Null,
            });
        }
    }
}

/// Extract the **execution** invocation tree from `fn_call` / `fn_return`
/// host-VM diagnostic events.
///
/// The host emits a depth-first stream around every contract entry/exit:
///
/// * `fn_call`  — `topics = [Symbol("fn_call"), Address(contract_to_call),
///                Symbol(function_name)]`, `data = Vec(args)`. The event's
///   `contract_id` field is `None` (host event); the called contract lives
///   in `topics[1]`.
/// * `fn_return` — `topics = [Symbol("fn_return"), Symbol(function_name)]`,
///   `data = ScVal(return_value)`. The event's `contract_id` field carries
///   the contract that's returning; we use it to validate the stack pop
///   but never depend on it for correctness.
///
/// Walking the stream:
///
/// 1. On `fn_call`, push a frame and emit an `ExtractedInvocation` whose
///    `caller` is the contract on top of the active stack (the caller is
///    "the frame currently executing"). When the stack is empty — the
///    very first call of the tx, or the tx-source-account-rooted root —
///    the caller is `tx_source_account`.
/// 2. On `fn_return`, pop the topmost frame.
/// 3. On execution traps that leave residual unmatched calls, the
///    remaining frames are silently dropped at end-of-stream — they were
///    already emitted on their `fn_call`.
///
/// Returns an empty Vec when the meta has no diagnostic events
/// (signalling the auth-tree fallback path in the caller).
///
/// The caller chain is the contract `C…` StrKey, not an account `G…` —
/// the indexer staging layer routes contract callers to
/// `caller_contract_id` (task 0183 schema) while keeping account callers
/// on the existing `caller_id` column.
pub fn extract_invocations_from_diagnostics(
    tx_meta: &TransactionMeta,
    transaction_hash: &str,
    ledger_sequence: u32,
    created_at: i64,
    tx_source_account: &str,
    successful: bool,
) -> Vec<ExtractedInvocation> {
    let diags = collect_diagnostic_events(tx_meta);
    if diags.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut stack: Vec<DiagFrame> = Vec::new();
    let mut index: u32 = 0;

    for diag in diags {
        // Diagnostic-typed only — host-VM trace entries. Contract-typed
        // copies of consensus events live alongside in v4.diagnostic_events
        // (Galexie diagnostic mode) and must not be misread as fn_call /
        // fn_return.
        if !matches!(diag.event.type_, ContractEventType::Diagnostic) {
            continue;
        }
        let ContractEventBody::V0(ref v0) = diag.event.body;

        let Some(kind) = classify_diag_topic(&v0.topics) else {
            continue;
        };

        match kind {
            DiagKind::FnCall { contract_id } => {
                let caller = match stack.last() {
                    Some(frame) => frame.contract_id.clone(),
                    None => tx_source_account.to_string(),
                };
                out.push(ExtractedInvocation {
                    transaction_hash: transaction_hash.to_string(),
                    contract_id: Some(contract_id.clone()),
                    caller_account: Some(caller),
                    function_name: None,
                    function_args: Value::Null,
                    return_value: Value::Null,
                    successful,
                    invocation_index: index,
                    depth: stack.len() as u32,
                    ledger_sequence,
                    created_at,
                });
                index += 1;
                stack.push(DiagFrame { contract_id });
            }
            DiagKind::FnReturn => {
                stack.pop();
            }
        }
    }

    out
}

struct DiagFrame {
    contract_id: String,
}

enum DiagKind {
    FnCall { contract_id: String },
    FnReturn,
}

fn classify_diag_topic(topics: &VecM<ScVal>) -> Option<DiagKind> {
    // Topic 0 distinguishes the kind. Anything other than fn_call/fn_return
    // is a different host trace (core_metrics, log, error, host_fn_failed)
    // — not part of the call graph.
    let head_sym = match topics.first()? {
        ScVal::Symbol(sym) => std::str::from_utf8(sym.as_vec()).ok()?,
        _ => return None,
    };

    match head_sym {
        "fn_call" => Some(DiagKind::FnCall {
            contract_id: decode_call_target(topics.get(1)?)?,
        }),
        "fn_return" => Some(DiagKind::FnReturn),
        _ => None,
    }
}

/// Decode the called-contract identity from a `fn_call` event's
/// `topics[1]`. The host historically encodes this two ways:
///
/// * `ScVal::Bytes` carrying the raw 32-byte contract hash — this is
///   what mainnet captive-core actually emits today (verified against
///   ledger 62016086 on 2026-04-30).
/// * `ScVal::Address(ScAddress::Contract(_))` — the structured form some
///   newer host revisions use. Accepted for forward compatibility so
///   the walker keeps working through future upgrades.
///
/// Returns `None` for any other shape (incl. account `Address` variants
/// — fn_call always targets a contract; an account topic indicates
/// either a malformed event or an unrelated diagnostic kind).
fn decode_call_target(topic: &ScVal) -> Option<String> {
    match topic {
        ScVal::Bytes(bytes) => {
            let raw = bytes.as_slice();
            if raw.len() != 32 {
                return None;
            }
            let mut buf = [0u8; 32];
            buf.copy_from_slice(raw);
            Some(ScAddress::Contract(ContractId(Hash(buf))).to_string())
        }
        ScVal::Address(addr @ ScAddress::Contract(_)) => Some(addr.to_string()),
        _ => None,
    }
}

/// Pull `diagnostic_events` from V3 (`soroban_meta.diagnostic_events`) or
/// V4 (`v4.diagnostic_events`) meta. Galexie's captive-core enables
/// diagnostic mode by default, so the V4 stream is reliably populated;
/// the V3 path is kept for parity with `extract_events`.
fn collect_diagnostic_events(meta: &TransactionMeta) -> Vec<&DiagnosticEvent> {
    match meta {
        TransactionMeta::V3(v3) => v3
            .soroban_meta
            .as_ref()
            .map(|m| m.diagnostic_events.iter().collect())
            .unwrap_or_default(),
        TransactionMeta::V4(v4) => v4.diagnostic_events.iter().collect(),
        _ => Vec::new(),
    }
}

/// Build a nested JSON tree from an invocation node using iterative post-order traversal.
///
/// Uses an explicit stack to avoid stack overflow on deep auth trees.
fn invocation_to_json(
    root: &SorobanAuthorizedInvocation,
    root_return_value: Value,
    successful: bool,
) -> Value {
    // Post-order: process children before parents. Use two passes:
    // 1. DFS to collect nodes in visit order
    // 2. Process in reverse, building children arrays bottom-up

    struct Visit<'a> {
        node: &'a SorobanAuthorizedInvocation,
        return_value: Value,
        child_count: usize,
    }

    let mut visits = Vec::new();
    let mut dfs_stack: Vec<(&SorobanAuthorizedInvocation, Value)> = vec![(root, root_return_value)];

    while let Some((node, ret_val)) = dfs_stack.pop() {
        let child_count = node.sub_invocations.len();
        visits.push(Visit {
            node,
            return_value: ret_val,
            child_count,
        });
        // Push children in reverse for left-to-right order
        for child in node.sub_invocations.iter().rev() {
            dfs_stack.push((child, Value::Null));
        }
    }

    // Build JSON bottom-up: process visits in reverse
    let mut result_stack: Vec<Value> = Vec::new();
    for visit in visits.into_iter().rev() {
        let (contract_id, function_name, function_args) =
            decode_authorized_function(&visit.node.function);

        // Pop this node's children from the result stack
        let children: Vec<Value> = result_stack.split_off(result_stack.len() - visit.child_count);

        let node_json = json!({
            "contractId": contract_id,
            "functionName": function_name,
            "args": function_args,
            "returnValue": visit.return_value,
            "successful": successful,
            "children": children,
        });
        result_stack.push(node_json);
    }

    result_stack.pop().unwrap_or(Value::Null)
}

/// Decode a `SorobanAuthorizedFunction` into (contract_id, function_name, args_json).
fn decode_authorized_function(
    func: &SorobanAuthorizedFunction,
) -> (Option<String>, Option<String>, Value) {
    match func {
        SorobanAuthorizedFunction::ContractFn(args) => {
            let contract_id = args.contract_address.to_string();
            let function_name = std::str::from_utf8(args.function_name.as_vec())
                .unwrap_or("<invalid-utf8>")
                .to_string();
            let function_args: Vec<Value> = args.args.iter().map(scval_to_typed_json).collect();
            (Some(contract_id), Some(function_name), json!(function_args))
        }
        SorobanAuthorizedFunction::CreateContractHostFn(args) => {
            let executable = format_contract_executable(&args.executable);
            (
                None,
                Some("createContract".to_string()),
                json!({
                    "type": "createContract",
                    "executable": executable,
                }),
            )
        }
        SorobanAuthorizedFunction::CreateContractV2HostFn(args) => {
            let executable = format_contract_executable(&args.executable);
            let constructor_args: Vec<Value> = args
                .constructor_args
                .iter()
                .map(scval_to_typed_json)
                .collect();
            (
                None,
                Some("createContractV2".to_string()),
                json!({
                    "type": "createContractV2",
                    "executable": executable,
                    "constructorArgs": constructor_args,
                }),
            )
        }
    }
}

/// Extract the Soroban return value from transaction metadata, if present.
fn soroban_return_value(meta: &TransactionMeta) -> Option<ScVal> {
    match meta {
        TransactionMeta::V3(v3) => v3.soroban_meta.as_ref().map(|m| m.return_value.clone()),
        TransactionMeta::V4(v4) => v4
            .soroban_meta
            .as_ref()
            .and_then(|m| m.return_value.clone()),
        _ => None,
    }
}

fn format_contract_executable(exec: &ContractExecutable) -> Value {
    match exec {
        ContractExecutable::Wasm(hash) => json!({ "type": "wasm", "hash": hex::encode(hash.0) }),
        ContractExecutable::StellarAsset => json!({ "type": "stellar_asset" }),
        // CAP-85 (protocol 28): code owned by another contract — no own hash.
        ContractExecutable::ExternalRef(r) => json!({
            "type": "external_ref",
            "owner": r.executable_owner.to_string(),
            "tag": std::str::from_utf8(r.tag.0.as_slice()).unwrap_or("<invalid-utf8>"),
        }),
    }
}

#[cfg(test)]
#[path = "invocation_tests.rs"]
mod tests;
