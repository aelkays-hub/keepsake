//! Typed identifiers and hashes stored as BLOBs.

use std::fmt;

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! uuid_id {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            /// A fresh random identifier.
            pub fn new_random() -> Self {
                Self(Uuid::new_v4())
            }

            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl std::str::FromStr for $name {
            type Err = uuid::Error;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.0.as_bytes().to_vec()))
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                let b = value.as_blob()?;
                let arr: [u8; 16] = b.try_into().map_err(|_| FromSqlError::InvalidBlobSize {
                    expected_size: 16,
                    blob_size: b.len(),
                })?;
                Ok(Self(Uuid::from_bytes(arr)))
            }
        }
    };
}

uuid_id!(
    /// Identifier of an [`crate::catalog::Asset`].
    AssetId
);
uuid_id!(
    /// Identifier of a physical [`crate::catalog::File`].
    FileId
);
uuid_id!(
    /// Identifier of a [`crate::catalog::Volume`].
    VolumeId
);
uuid_id!(
    /// Identifier of an [`crate::catalog::Event`].
    EventId
);
uuid_id!(
    /// Identifier of an [`crate::catalog::EditRecipe`].
    RecipeId
);
uuid_id!(
    /// Identifier of a [`crate::catalog::Rendition`].
    RenditionId
);
uuid_id!(
    /// Identifier of an [`crate::catalog::ImportSession`].
    ImportSessionId
);

impl FileId {
    /// Deterministic file id derived from its location, so a rebuilt catalog
    /// assigns the same id to the same file.
    pub fn for_location(volume: VolumeId, rel_path_key: &str) -> Self {
        Self(derived_uuid(
            b"keepsake.file",
            volume.0.as_bytes(),
            rel_path_key,
        ))
    }
}

impl AssetId {
    /// Deterministic asset id for assets discovered without a Keepsake sidecar
    /// (derived from the original file's location).
    pub fn for_original(volume: VolumeId, rel_path_key: &str) -> Self {
        Self(derived_uuid(
            b"keepsake.asset",
            volume.0.as_bytes(),
            rel_path_key,
        ))
    }
}

fn derived_uuid(domain: &[u8], ns: &[u8], name: &str) -> Uuid {
    let mut h = blake3::Hasher::new();
    h.update(domain);
    h.update(&[0]);
    h.update(ns);
    h.update(name.as_bytes());
    let digest = h.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    uuid::Builder::from_custom_bytes(bytes).into_uuid()
}

/// A BLAKE3 digest.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Blake3Hash(pub [u8; 32]);

impl Blake3Hash {
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }

    pub fn to_hex(&self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        blake3::Hash::from_hex(s).ok().map(|h| Self(*h.as_bytes()))
    }
}

impl fmt::Debug for Blake3Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Blake3Hash({})", self.to_hex())
    }
}

impl fmt::Display for Blake3Hash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl Serialize for Blake3Hash {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}

impl<'de> Deserialize<'de> for Blake3Hash {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Blake3Hash::from_hex(&s).ok_or_else(|| serde::de::Error::custom("invalid BLAKE3 hex"))
    }
}

impl ToSql for Blake3Hash {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.to_vec()))
    }
}

impl FromSql for Blake3Hash {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let b = value.as_blob()?;
        let arr: [u8; 32] = b.try_into().map_err(|_| FromSqlError::InvalidBlobSize {
            expected_size: 32,
            blob_size: b.len(),
        })?;
        Ok(Self(arr))
    }
}
