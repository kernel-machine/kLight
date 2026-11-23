//! Strongly typed identifiers.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}#{}", stringify!($name), self.0)
            }
        }
    };
}

id_type!(
    /// Identifier of a DMX universe.
    UniverseId
);
id_type!(
    /// Identifier of a patched fixture.
    FixtureId
);
id_type!(
    /// Identifier of a fixture group.
    GroupId
);
id_type!(
    /// Identifier of a function (scene, chaser, cue list, effect).
    FunctionId
);
id_type!(
    /// Identifier of a palette.
    PaletteId
);
id_type!(
    /// Identifier of a virtual console widget.
    WidgetId
);
id_type!(
    /// Identifier of an input mapping.
    MappingId
);
id_type!(
    /// Identifier of a folder of functions.
    FolderId
);
id_type!(
    /// Identifier of an API client connection.
    ClientId
);

/// Key of a fixture definition: `"<manufacturer>/<model>"`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(transparent)]
pub struct DefinitionKey(pub String);

impl DefinitionKey {
    pub fn new(manufacturer: &str, model: &str) -> Self {
        Self(format!("{manufacturer}/{model}"))
    }
}

impl std::fmt::Display for DefinitionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
