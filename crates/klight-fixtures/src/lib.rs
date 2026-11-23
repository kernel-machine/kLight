//! Fixture definitions: QLC+ `.qxf` parsing, resolved profiles and unit
//! conversions. Pure code: reading files from disk is up to the caller.

pub mod color_sequence;
pub mod convert;
pub mod profile;
pub mod qxf;
pub mod qxi;
mod xml;

use std::collections::BTreeMap;

use klight_model::{DefinitionKey, FixtureDefinition};

pub use convert::{Converter, Ignored, Resolved};
pub use profile::{FixtureProfile, HeadProfile, Slot};
pub use qxf::parse_qxf;
pub use qxi::{parse_qxi, InputProfileDef};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QxfError {
    #[error("malformed XML: {0}")]
    Xml(String),
    #[error("invalid fixture definition: {0}")]
    Invalid(String),
    #[error("input too large")]
    TooLarge,
}

/// In-memory library of fixture definitions, keyed by manufacturer/model.
#[derive(Debug, Default, Clone)]
pub struct FixtureLibrary {
    definitions: BTreeMap<DefinitionKey, FixtureDefinition>,
}

/// A file that could not be loaded into the library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rejected {
    pub source: String,
    pub error: QxfError,
}

impl FixtureLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses and adds a definition. Malformed files are rejected with a
    /// descriptive error and leave the library unchanged.
    pub fn add_qxf(&mut self, source: &str, text: &str) -> Result<DefinitionKey, Rejected> {
        let def = parse_qxf(text).map_err(|error| Rejected {
            source: source.to_owned(),
            error,
        })?;
        let key = def.key();
        self.definitions.insert(key.clone(), def);
        Ok(key)
    }

    pub fn insert(&mut self, def: FixtureDefinition) -> DefinitionKey {
        let key = def.key();
        self.definitions.insert(key.clone(), def);
        key
    }

    pub fn get(&self, key: &DefinitionKey) -> Option<&FixtureDefinition> {
        self.definitions.get(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = &FixtureDefinition> {
        self.definitions.values()
    }

    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }
}
