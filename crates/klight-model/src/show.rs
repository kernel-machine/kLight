//! The show: everything needed to run a performance.

use serde::{Deserialize, Serialize};

use crate::fixture_def::{FixtureDefinition, ModeDef};
use crate::function::{Function, FunctionFolder};
use crate::ids::{DefinitionKey, FixtureId, FolderId, FunctionId, GroupId, PaletteId, UniverseId};
use crate::input_map::{InputDeviceConfig, InputMapping};
use crate::palette::Palette;
use crate::patch::{FixtureGroup, PatchedFixture, Universe};
use crate::vc::VirtualConsole;

/// Current show file schema version. Bump together with a migration in
/// `klight-persist`.
pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct ShowSettings {
    /// Tick frequency of the engine.
    pub tick_hz: u16,
    /// PIN required to switch to Design mode from remote clients.
    #[serde(default)]
    pub design_pin: Option<String>,
    /// Refresh period of outputs when nothing changes (ms).
    pub output_refresh_ms: u32,
}

impl Default for ShowSettings {
    fn default() -> Self {
        Self {
            tick_hz: 44,
            design_pin: None,
            output_refresh_ms: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct Show {
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub universes: Vec<Universe>,
    /// Embedded copies of every definition used: the show opens identically
    /// on machines without the `.qxf` files.
    #[serde(default)]
    pub definitions: Vec<FixtureDefinition>,
    #[serde(default)]
    pub fixtures: Vec<PatchedFixture>,
    #[serde(default)]
    pub groups: Vec<FixtureGroup>,
    #[serde(default)]
    pub palettes: Vec<Palette>,
    #[serde(default)]
    pub functions: Vec<Function>,
    /// Folders organising functions, per function type.
    #[serde(default)]
    pub function_folders: Vec<FunctionFolder>,
    #[serde(default)]
    pub virtual_console: VirtualConsole,
    #[serde(default)]
    pub input_devices: Vec<InputDeviceConfig>,
    #[serde(default)]
    pub input_mappings: Vec<InputMapping>,
    #[serde(default)]
    pub settings: ShowSettings,
}

impl Default for Show {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

impl Show {
    pub fn new(name: &str) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: name.to_owned(),
            universes: Vec::new(),
            definitions: Vec::new(),
            fixtures: Vec::new(),
            groups: Vec::new(),
            palettes: Vec::new(),
            functions: Vec::new(),
            function_folders: Vec::new(),
            virtual_console: VirtualConsole::default(),
            input_devices: Vec::new(),
            input_mappings: Vec::new(),
            settings: ShowSettings::default(),
        }
    }

    pub fn universe(&self, id: UniverseId) -> Option<&Universe> {
        self.universes.iter().find(|u| u.id == id)
    }
    pub fn fixture(&self, id: FixtureId) -> Option<&PatchedFixture> {
        self.fixtures.iter().find(|f| f.id == id)
    }
    pub fn group(&self, id: GroupId) -> Option<&FixtureGroup> {
        self.groups.iter().find(|g| g.id == id)
    }
    pub fn palette(&self, id: PaletteId) -> Option<&Palette> {
        self.palettes.iter().find(|p| p.id == id)
    }
    pub fn function(&self, id: FunctionId) -> Option<&Function> {
        self.functions.iter().find(|f| f.id == id)
    }
    pub fn folder(&self, id: FolderId) -> Option<&FunctionFolder> {
        self.function_folders.iter().find(|f| f.id == id)
    }

    pub fn next_folder_id(&self) -> FolderId {
        FolderId(
            self.function_folders
                .iter()
                .map(|f| f.id.0)
                .max()
                .map_or(1, |m| m.saturating_add(1)),
        )
    }

    pub fn definition(&self, key: &DefinitionKey) -> Option<&FixtureDefinition> {
        self.definitions.iter().find(|d| &d.key() == key)
    }

    /// Definition and mode of a patched fixture.
    pub fn definition_and_mode(
        &self,
        fixture: &PatchedFixture,
    ) -> Option<(&FixtureDefinition, &ModeDef)> {
        let def = self.definition(&fixture.definition)?;
        let mode = def.mode(&fixture.mode)?;
        Some((def, mode))
    }

    pub fn next_fixture_id(&self) -> FixtureId {
        FixtureId(
            self.fixtures
                .iter()
                .map(|f| f.id.0)
                .max()
                .map_or(1, |m| m.saturating_add(1)),
        )
    }
    pub fn next_function_id(&self) -> FunctionId {
        FunctionId(
            self.functions
                .iter()
                .map(|f| f.id.0)
                .max()
                .map_or(1, |m| m.saturating_add(1)),
        )
    }
    pub fn next_group_id(&self) -> GroupId {
        GroupId(
            self.groups
                .iter()
                .map(|f| f.id.0)
                .max()
                .map_or(1, |m| m.saturating_add(1)),
        )
    }
    pub fn next_palette_id(&self) -> PaletteId {
        PaletteId(
            self.palettes
                .iter()
                .map(|f| f.id.0)
                .max()
                .map_or(1, |m| m.saturating_add(1)),
        )
    }
    pub fn next_universe_id(&self) -> UniverseId {
        UniverseId(
            self.universes
                .iter()
                .map(|f| f.id.0)
                .max()
                .map_or(1, |m| m.saturating_add(1)),
        )
    }
}
