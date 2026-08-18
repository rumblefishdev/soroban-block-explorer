//! OpenAPI specification root document.
//!
//! This module defines [`ApiDoc`] — the metadata-only root document
//! (title, version, description, shared schema components). Endpoint
//! paths are registered dynamically at runtime through
//! `utoipa_axum::router::OpenApiRouter::routes`, and the runtime
//! `servers` block is stamped onto the registered spec by `main::app`
//! so the advertised base URL can vary per deployment environment.

pub mod schemas;

use domain::OperationType;
use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::accounts::dto::{
    AccountBalance, AccountDetailResponse, AccountListItem, AccountTransactionItem,
};
use crate::assets::dto::{AssetDetailResponse, AssetItem, AssetTransactionItem};
use crate::contracts::dto::{
    ContractDetailResponse, ContractFunctionParam, ContractFunctionSig, ContractInterfaceMetadata,
    ContractListItem, ContractStats, DecompileDiagnostic, DecompiledResponse, EventItem,
    InterfaceResponse, InvocationItem, SacAsset,
};
use crate::liquidity_pools::dto::{
    ChartDataPoint, ChartResponse, PoolActivityItem, PoolAssetLeg, PoolEvent, PoolItem,
};
use crate::nfts::dto::{NftItem, NftTransferItem};
use crate::runtime_enrichment::stellar_archive::dto::{
    E3HeavyFields, E3Response, HeavyFieldsStatus, SignatureDto, XdrEventDto, XdrOperationDto,
};
use crate::search::dto::{EntityType, SearchGroups, SearchHit, SearchResults};
use crate::transactions::dto::{
    EventAppearanceItem, InvocationAppearanceItem, OperationItem, TransactionDetailLight,
    TransactionListItem,
};
use schemas::{ErrorEnvelope, PageInfo, Paginated};

/// Injects the two security schemes the auth gate accepts (task 0277) into the
/// OpenAPI components so Swagger UI renders an "Authorize" dialog and "Try it
/// out" can reach the gated `/v1` surface:
///   - `api_key`    — paid tier, `x-api-key` request header
///   - `bearer_jwt` — free tier, `Authorization: Bearer <session JWT>`
///
/// `ApiDoc`'s `security(...)` attribute references these schemes by name; the
/// schemes themselves can only be constructed in a `Modify` impl (the derive
/// attribute can't build an `ApiKey`/`Http` scheme value).
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "api_key",
            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("x-api-key"))),
        );
        components.add_security_scheme(
            "bearer_jwt",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

/// Root OpenAPI document. Holds API metadata and declares shared
/// schema components that are referenced across multiple endpoints.
///
/// Paths are registered dynamically through `OpenApiRouter::routes`
/// so handler modules don't need to be listed here explicitly — M2
/// endpoint modules add routes without touching this file.
#[derive(OpenApi)]
#[openapi(
    modifiers(&SecurityAddon),
    info(
        title = "Soroban Block Explorer API",
        version = env!("CARGO_PKG_VERSION"),
        description = "REST API exposing ledger, transaction, contract, and NFT \
                       data for the Soroban block explorer. All list endpoints \
                       use cursor-based pagination (see ADR 0008).",
        contact(name = "Rumble Fish", url = "https://rumblefish.dev"),
        license(name = "Proprietary"),
    ),
    components(schemas(
        ErrorEnvelope,
        PageInfo,
        Paginated<TransactionListItem>,
        TransactionListItem,
        E3Response<TransactionDetailLight>,
        TransactionDetailLight,
        OperationItem,
        EventAppearanceItem,
        InvocationAppearanceItem,
        E3HeavyFields,
        SignatureDto,
        XdrEventDto,
        XdrOperationDto,
        HeavyFieldsStatus,
        Paginated<ContractListItem>,
        ContractListItem,
        ContractDetailResponse,
        ContractStats,
        SacAsset,
        InterfaceResponse,
        ContractInterfaceMetadata,
        ContractFunctionSig,
        ContractFunctionParam,
        DecompiledResponse,
        DecompileDiagnostic,
        Paginated<InvocationItem>,
        InvocationItem,
        Paginated<EventItem>,
        EventItem,
        Paginated<AssetItem>,
        AssetItem,
        AssetDetailResponse,
        Paginated<AssetTransactionItem>,
        AssetTransactionItem,
        Paginated<AccountListItem>,
        AccountListItem,
        AccountDetailResponse,
        AccountBalance,
        Paginated<AccountTransactionItem>,
        AccountTransactionItem,
        Paginated<NftItem>,
        NftItem,
        Paginated<NftTransferItem>,
        NftTransferItem,
        Paginated<PoolItem>,
        PoolItem,
        PoolAssetLeg,
        Paginated<PoolActivityItem>,
        PoolActivityItem,
        PoolEvent,
                        ChartResponse,
        ChartDataPoint,
        SearchResults,
        SearchGroups,
        SearchHit,
        EntityType,
        // Canonical Stellar operation-type enum. Not referenced by a DTO
        // field (response op-type fields stay wire-`string`), but registered
        // here so the generated TS client emits a named `OperationType` union
        // — the single source of truth the frontend keys its op-type label
        // map against, replacing a hand-maintained 27-entry mirror that
        // silently drifted from this enum (audit F-Z-2 / lore-0280).
        OperationType,
    )),
    // Auth advertised in the spec so Swagger UI's "Authorize" works and "Try it
    // out" can hit the gated /v1 surface (task 0287). Two separate requirement
    // objects = OR: a request passes the gate with EITHER the paid `x-api-key`
    // OR a free-tier `Bearer` session JWT. Schemes are injected by SecurityAddon.
    // Exempt endpoints (e.g. /health) override this with `security(())`.
    security(
        ("api_key" = []),
        ("bearer_jwt" = []),
    ),
)]
pub struct ApiDoc;

/// Build the `OpenApiRouter` carrying every endpoint advertised by the
/// API. Shared between the live Lambda app (`main::app`) and the
/// build-time `extract_openapi` binary so neither can quietly drop a
/// route from the spec the frontend codegen consumes.
///
/// Returns a router typed with `AppState` but not yet bound to a state
/// value. Callers attach state via `.with_state(...)` (live app) or
/// call `.split_for_parts()` directly (extractor — the spec does not
/// depend on `AppState`).
pub fn register_routes() -> OpenApiRouter<crate::AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .routes(routes!(crate::ops::health))
        .nest("/v1", crate::network::router())
        .nest("/v1", crate::transactions::router())
        .nest("/v1", crate::contracts::router())
        .nest("/v1", crate::liquidity_pools::router())
        .nest("/v1", crate::nfts::router())
        .nest("/v1", crate::assets::router())
        .nest("/v1", crate::ledgers::router())
        .nest("/v1", crate::accounts::router())
        .nest("/v1", crate::search::router())
        .layer(axum::middleware::from_fn(
            crate::common::cache_control::enforce_no_store_on_errors,
        ))
}
