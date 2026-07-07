//! Explorer-synthetic `assets.asset_type` domain (3 variants).
//!
//! Maps to `assets.asset_type SMALLINT NOT NULL`. The variants overlap
//! with XDR `AssetType` on `native` / `classic_credit` but diverge for a
//! pure `soroban` (bespoke contract token), which cannot be expressed in
//! the raw XDR discriminator. Kept as a separate enum so each column tells
//! its reader which domain it speaks.
//!
//! A Stellar Asset Contract (SAC) is **not** a distinct variant (ADR 0051):
//! a SAC is the same economic asset as its classic credit / native asset,
//! so SAC-ness is carried in the `asset_sac` side table (`sac_contract_id` /
//! `sac_deployed`) keyed on the `classic_credit` / `native` identity, not a
//! separate `asset_type`. Discriminant `2` (the former `Sac`) is retired and
//! left unused — `TryFrom<i16>` rejects it.

use serde::{Deserialize, Serialize};

use super::EnumDecodeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[repr(i16)]
pub enum TokenAssetType {
    Native = 0,
    ClassicCredit = 1,
    // 2 was `Sac` — retired (ADR 0051); SAC-ness is a property of the
    // classic_credit / native row, not a distinct asset_type.
    Soroban = 3,
}

impl TokenAssetType {
    pub const VARIANTS: &'static [Self] = &[Self::Native, Self::ClassicCredit, Self::Soroban];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::ClassicCredit => "classic_credit",
            Self::Soroban => "soroban",
        }
    }
}

impl TryFrom<i16> for TokenAssetType {
    type Error = EnumDecodeError;

    fn try_from(v: i16) -> Result<Self, Self::Error> {
        match v {
            0 => Ok(Self::Native),
            1 => Ok(Self::ClassicCredit),
            // 2 (`Sac`) retired (ADR 0051) — reject it like any unknown value.
            3 => Ok(Self::Soroban),
            _ => Err(EnumDecodeError::UnknownDiscriminant {
                enum_name: "TokenAssetType",
                value: v,
            }),
        }
    }
}

impl std::str::FromStr for TokenAssetType {
    type Err = EnumDecodeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::VARIANTS
            .iter()
            .copied()
            .find(|v| v.as_str() == s)
            .ok_or_else(|| EnumDecodeError::UnknownLabel {
                enum_name: "TokenAssetType",
                value: s.to_string(),
            })
    }
}

impl std::fmt::Display for TokenAssetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for v in TokenAssetType::VARIANTS {
            assert_eq!(TokenAssetType::try_from(*v as i16).unwrap(), *v);
            assert_eq!(v.as_str().parse::<TokenAssetType>().unwrap(), *v);
        }
    }
}
