//! The engine: applies commands and computes DMX frames. Pure and
//! deterministic: time comes from the caller.

use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use klight_fixtures::Resolved;
use klight_model::palette::FixturePaletteValues;
use klight_model::runtime::{
    ActiveFunction, ChannelOverride, ProgrammerEntry, RuntimeSnapshot, RuntimeState,
};
use klight_model::validate::{self, PatchError};
use klight_model::vc::{ButtonAction, SliderMode, WidgetKind};
use klight_model::{
    Attribute, ChannelRef, Command, CommandEnvelope, CommandError, CommandOutcome, CommandResult,
    CommandSource, ControlKey, FixtureId, Function, FunctionId, FunctionKind, OperatingMode,
    Palette, PaletteId, Scene, SceneValue, Show, Target, ValueSource, WidgetId, UNIVERSE_SIZE,
};

use crate::compile::{
    CompiledFunction, CompiledMatrix, CompiledMovement, CompiledShow, LogicalChannel, Merge, Role,
    StepContent, NO_ADDR, NO_CHANNEL,
};
use crate::effects::{cycle_turns, effect_size, movement_size, render_matrix, render_movement};
use crate::frame::{DmxFrame, FrameSlot};
use crate::music::MusicClock;
use crate::playback::{
    scale, ChaserState, Ctx, CueState, Layer, Playback, PlaybackKind, SceneInstance,
};
use crate::scene_edit::add_scene_members;
use crate::shared::{EngineShared, UniverseOutput};

#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Maximum commands applied per tick (the rest wait for the next tick).
    pub max_commands_per_tick: usize,
    pub undo_limit: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            max_commands_per_tick: 256,
            undo_limit: 100,
        }
    }
}

type EditResult = Result<Vec<String>, CommandError>;

/// How long a preview lasts after the last time it was sent: an editor that
/// goes away (closed page, lost connection) leaves nothing on the lights.
pub const PREVIEW_HOLD_US: u64 = 3_000_000;

#[derive(Debug)]
struct Runtime {
    mode: OperatingMode,
    grand_master: f32,
    blackout: bool,
    playbacks: Vec<Playback>,
    pool: Vec<SceneInstance>,
    base: Layer,
    effects: Layer,
    /// Work layer to merge the scenes of a collection.
    mix: Layer,
    programmer: Layer,
    programmer_entries: Vec<ProgrammerEntry>,
    /// Values an editor is trying out, on top of the other layers.
    preview: Layer,
    preview_values: Vec<SceneValue>,
    preview_members: Vec<Target>,
    /// When the preview ends unless renewed; `None` without one.
    preview_until: Option<u64>,
    /// Raw Simple Desk channels per universe (parallel to `compiled.universes`).
    raw: Vec<[u16; UNIVERSE_SIZE]>,
    merged: Vec<u16>,
    music: MusicClock,
    holds: BTreeMap<ControlKey, u8>,
    next_order: u64,
    /// Levels set while a function was inactive (applied when it starts).
    pending_levels: BTreeMap<FunctionId, f32>,
}

pub struct Engine {
    show: Arc<Show>,
    compiled: Arc<CompiledShow>,
    shared: Arc<EngineShared>,
    undo: Vec<Arc<Show>>,
    redo: Vec<Arc<Show>>,
    rt: Runtime,
    frames: Vec<DmxFrame>,
    outputs: Vec<Arc<FrameSlot>>,
    scratch: Vec<Resolved>,
    scratch_pairs: Vec<(u32, u16)>,
    dirty: bool,
    seq: u64,
    now_us: u64,
    config: EngineConfig,
}

fn f32_to16(v: f32) -> u16 {
    klight_fixtures::convert::unit_to16(v)
}

fn finite01(v: f32, what: &str) -> Result<f32, CommandError> {
    if v.is_finite() && (0.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err(CommandError::invalid(format!(
            "{what} must be between 0 and 1"
        )))
    }
}

impl Engine {
    pub fn new(show: Show, config: EngineConfig) -> Self {
        let show = Arc::new(show);
        let shared = Arc::new(EngineShared::new(show.clone()));
        let mut e = Self {
            compiled: Arc::new(CompiledShow::default()),
            show,
            shared,
            undo: Vec::new(),
            redo: Vec::new(),
            rt: Runtime {
                mode: OperatingMode::Design,
                grand_master: 1.0,
                blackout: false,
                playbacks: Vec::with_capacity(256),
                pool: Vec::with_capacity(256),
                base: Layer::default(),
                effects: Layer::default(),
                mix: Layer::default(),
                programmer: Layer::default(),
                programmer_entries: Vec::new(),
                preview: Layer::default(),
                preview_values: Vec::new(),
                preview_members: Vec::new(),
                preview_until: None,
                raw: Vec::new(),
                merged: Vec::new(),
                music: MusicClock::new(120.0),
                holds: BTreeMap::new(),
                next_order: 0,
                pending_levels: BTreeMap::new(),
            },
            frames: Vec::new(),
            outputs: Vec::new(),
            scratch: Vec::with_capacity(64),
            scratch_pairs: Vec::with_capacity(64),
            dirty: true,
            seq: 0,
            now_us: 0,
            config,
        };
        e.recompile();
        e.publish_snapshot();
        e
    }

    pub fn shared(&self) -> Arc<EngineShared> {
        self.shared.clone()
    }

    pub fn show(&self) -> &Arc<Show> {
        &self.show
    }

    pub fn compiled(&self) -> &CompiledShow {
        &self.compiled
    }

    pub fn config(&self) -> &EngineConfig {
        &self.config
    }

    /// Last computed frame of a universe.
    pub fn frame(&self, id: klight_model::UniverseId) -> Option<&DmxFrame> {
        let i = self.compiled.universe_index(id)?;
        self.frames.get(i)
    }

    pub fn runtime_state(&self) -> RuntimeState {
        RuntimeState {
            mode: self.rt.mode,
            grand_master: self.rt.grand_master,
            blackout: self.rt.blackout,
            active: self
                .rt
                .playbacks
                .iter()
                .filter(|p| !p.stopping)
                .map(|p| {
                    let mut playing = Vec::new();
                    p.playing(&mut playing);
                    ActiveFunction {
                        id: p.id,
                        level: f32::from(p.level) / 65535.0,
                        position: p.position(),
                        paused: p.paused_since.is_some(),
                        playing,
                    }
                })
                .collect(),
            programmer: self.rt.programmer_entries.clone(),
            tempo: self.rt.music.status(),
            channels: self
                .compiled
                .universes
                .iter()
                .zip(&self.rt.raw)
                .flat_map(|(u, raw)| {
                    raw.iter()
                        .enumerate()
                        .filter(|(_, v)| **v != RAW_UNSET)
                        .map(move |(i, v)| ChannelOverride {
                            universe: *u,
                            address: u16::try_from(i + 1).unwrap_or(u16::MAX),
                            value: u8::try_from(*v).unwrap_or(u8::MAX),
                        })
                })
                .collect(),
        }
    }

    pub fn snapshot(&self) -> RuntimeSnapshot {
        RuntimeSnapshot {
            seq: self.seq,
            state: self.runtime_state(),
            undo_depth: u32::try_from(self.undo.len()).unwrap_or(u32::MAX),
            redo_depth: u32::try_from(self.redo.len()).unwrap_or(u32::MAX),
            warnings: self.compiled.warnings.clone(),
        }
    }

    fn publish_snapshot(&mut self) {
        self.seq += 1;
        let snap = self.snapshot();
        self.shared.snapshot.store(Arc::new(snap));
        self.dirty = false;
    }

    // ---------------------------------------------------------------- compile

    fn recompile(&mut self) {
        let compiled = Arc::new(CompiledShow::build(&self.show));
        let n = compiled.channel_count();
        self.rt.base.resize(n);
        self.rt.effects.resize(n);
        self.rt.mix.resize(n);
        self.rt.programmer.resize(n);
        self.rt.preview.resize(n);
        self.rt.merged.resize(n, 0);

        // Outputs: keep existing slots for universes that still exist.
        let old: Vec<UniverseOutput> = self.shared.outputs.load().as_ref().clone();
        let mut outputs = Vec::with_capacity(compiled.universes.len());
        let mut published = Vec::with_capacity(compiled.universes.len());
        for id in &compiled.universes {
            let slot = old
                .iter()
                .find(|u| u.id == *id)
                .map(|u| u.slot.clone())
                .unwrap_or_else(|| Arc::new(FrameSlot::new()));
            outputs.push(slot.clone());
            published.push(UniverseOutput { id: *id, slot });
        }
        self.outputs = outputs;
        self.frames = vec![[0u8; UNIVERSE_SIZE]; compiled.universes.len()];
        self.shared.outputs.store(Arc::new(published));
        // Raw desk channels follow their universe id.
        let old_raw = std::mem::take(&mut self.rt.raw);
        self.rt.raw = compiled
            .universes
            .iter()
            .map(|id| {
                self.compiled
                    .universe_index(*id)
                    .and_then(|i| old_raw.get(i).copied())
                    .unwrap_or([RAW_UNSET; UNIVERSE_SIZE])
            })
            .collect();
        self.compiled = compiled;

        // Rebind running playbacks to the new compiled functions.
        let now = self.now_us;
        let bpm = self.rt.music.bpm();
        let playbacks = std::mem::take(&mut self.rt.playbacks);
        for mut p in playbacks {
            if p.stopping {
                continue;
            }
            let Some(cf) = self.compiled.function(p.id).cloned() else {
                continue;
            };
            let channels = &self.compiled.channels;
            let base = &self.rt.base;
            let ctx = Ctx {
                now_us: now,
                music: &self.rt.music,
                base,
                channels,
            };
            match (&mut p.kind, cf) {
                (PlaybackKind::Scene(s), CompiledFunction::Scene(new)) => {
                    *s = new.clone();
                    p.show_now(&mut self.rt.pool, &StepContent::Scene(new), &ctx);
                }
                (PlaybackKind::Chaser(state), CompiledFunction::Chaser(new)) => {
                    state.step = state.step.min(new.steps.len().saturating_sub(1));
                    let content = new.steps.get(state.step).map(|s| s.content.clone());
                    state.chaser = new;
                    match content {
                        Some(content) => p.show_now(&mut self.rt.pool, &content, &ctx),
                        None => p.instances.clear(),
                    }
                }
                (PlaybackKind::Collection(c), CompiledFunction::Collection(new)) => {
                    *c = new.clone();
                    p.show_now(&mut self.rt.pool, &StepContent::Collection(new), &ctx);
                }
                (PlaybackKind::CueList(state), CompiledFunction::CueList(new)) => {
                    p.instances.clear();
                    state.current = state.current.filter(|&c| c < new.cues.len());
                    if let Some(cue) = state.current.and_then(|c| new.cues.get(c)) {
                        let mut inst = SceneInstance::default();
                        inst.rebind(cue.scene.clone(), now, bpm, base, channels);
                        p.instances.push(inst);
                    }
                    state.list = new;
                }
                (PlaybackKind::Movement(m), CompiledFunction::Movement(new)) => *m = new,
                (PlaybackKind::Matrix(m), CompiledFunction::Matrix(new)) => *m = new,
                _ => continue,
            }
            self.rt.playbacks.push(p);
        }
        let _ = self.rebuild_programmer();
        self.rebuild_preview();
        self.shared.show.store(self.show.clone());
        self.shared.show_seq.fetch_add(1, Ordering::Relaxed);
        self.dirty = true;
    }

    fn rebuild_programmer(&mut self) -> Vec<String> {
        let mut warnings = Vec::new();
        self.rt.programmer.clear();
        for entry in &self.rt.programmer_entries {
            for (fi, head, order, len) in self.compiled.expand_target(&self.show, &entry.target) {
                self.scratch_pairs.clear();
                let value = entry.value.for_member(order, len);
                if let Err(w) = self.compiled.resolve_head(
                    &self.show,
                    fi,
                    head,
                    &value,
                    &mut self.scratch,
                    &mut self.scratch_pairs,
                ) {
                    warnings.push(w);
                }
                for &(ch, v) in &self.scratch_pairs {
                    // Inside the programmer the last value wins for every channel.
                    self.rt.programmer.write(ch as usize, Merge::Ltp, v);
                }
            }
        }
        warnings.sort();
        warnings.dedup();
        warnings
    }

    fn rebuild_preview(&mut self) {
        self.rt.preview.clear();
        if self.rt.preview_until.is_none() {
            return;
        }
        // What cannot be played is left out: the editor learns of it when the
        // values are stored.
        let (entries, _) = self.compiled.resolve_values(
            &self.show,
            &self.rt.preview_values,
            &self.rt.preview_members,
        );
        for e in entries {
            self.rt
                .preview
                .write(e.channel as usize, Merge::Ltp, e.value);
        }
    }

    fn end_preview(&mut self) {
        self.rt.preview_until = None;
        self.rt.preview.clear();
    }

    // --------------------------------------------------------------- commands

    fn control_key(cmd: &Command) -> Option<ControlKey> {
        Some(match cmd {
            Command::SetGrandMaster { .. } => ControlKey::GrandMaster,
            Command::SetBlackout { .. } => ControlKey::Blackout,
            Command::StartFunction { id }
            | Command::StopFunction { id }
            | Command::SetFunctionLevel { id, .. }
            | Command::CueGo { id }
            | Command::CueBack { id }
            | Command::CuePause { id }
            | Command::CueGoTo { id, .. }
            | Command::ChaserGoTo { id, .. } => ControlKey::Function(*id),
            Command::WidgetPress { id }
            | Command::WidgetRelease { id }
            | Command::WidgetSetValue { id, .. } => ControlKey::Widget(*id),
            _ => return None,
        })
    }

    /// Applies a command at time `now_us`. Never panics; invalid commands
    /// return a typed error and leave the state unchanged.
    pub fn apply(&mut self, now_us: u64, env: CommandEnvelope) -> CommandResult {
        self.now_us = self.now_us.max(now_us);
        // A preview is no part of the state clients see or a restart restores.
        let preview = matches!(
            env.command,
            Command::SetPreview { .. } | Command::ClearPreview
        );
        let result = self.apply_inner(&env.source, env.command);
        if result.is_ok() && !preview {
            self.dirty = true;
        }
        CommandResult {
            id: env.id,
            source: env.source,
            result,
        }
    }

    fn apply_inner(
        &mut self,
        source: &CommandSource,
        cmd: Command,
    ) -> Result<CommandOutcome, CommandError> {
        // Source priority: an operator holding a control beats automation.
        if let Some(key) = Self::control_key(&cmd) {
            let prio = source.priority();
            if let Some(&held) = self.rt.holds.get(&key) {
                if held > prio {
                    return Err(CommandError::Overridden);
                }
            }
            if source.is_operator() {
                self.rt.holds.insert(key, prio);
            }
        }
        if cmd.is_edit() && self.rt.mode == OperatingMode::Show {
            return Err(CommandError::ShowModeLocked);
        }
        let warnings = if cmd.is_edit() {
            self.apply_edit(cmd)?
        } else {
            self.apply_runtime(source, cmd)?
        };
        Ok(CommandOutcome {
            warnings,
            show_seq: self.shared.show_seq.load(Ordering::Relaxed),
        })
    }

    fn commit(&mut self, new_show: Show) {
        let old = std::mem::replace(&mut self.show, Arc::new(new_show));
        self.undo.push(old);
        if self.undo.len() > self.config.undo_limit {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.recompile();
    }

    fn edit(&mut self, f: impl FnOnce(&mut Show) -> EditResult) -> EditResult {
        let mut s = (*self.show).clone();
        let warnings = f(&mut s)?;
        let before = Arc::clone(&self.compiled);
        self.commit(s);
        // Only what the edit brings: warnings already standing stay listed in
        // the snapshot without being reported again at every change.
        let mut all = warnings;
        all.extend(
            self.compiled
                .warnings
                .iter()
                .filter(|w| !before.warnings.contains(w))
                .take(20)
                .cloned(),
        );
        Ok(all)
    }

    fn apply_edit(&mut self, cmd: Command) -> EditResult {
        match cmd {
            Command::Undo => {
                let prev = self.undo.pop().ok_or(CommandError::NothingToUndo)?;
                let cur = std::mem::replace(&mut self.show, prev);
                self.redo.push(cur);
                self.recompile();
                Ok(Vec::new())
            }
            Command::Redo => {
                let next = self.redo.pop().ok_or(CommandError::NothingToUndo)?;
                let cur = std::mem::replace(&mut self.show, next);
                self.undo.push(cur);
                self.recompile();
                Ok(Vec::new())
            }
            Command::LoadShow { show } => {
                let mut show = *show;
                show.schema_version = klight_model::SCHEMA_VERSION;
                self.edit(|s| {
                    *s = show;
                    Ok(Vec::new())
                })
            }
            Command::SetShowName { name } => self.edit(|s| {
                s.name = name;
                Ok(Vec::new())
            }),
            Command::SetShowSettings { settings } => {
                if settings.tick_hz == 0 || settings.tick_hz > 1000 {
                    return Err(CommandError::invalid("tick_hz must be in 1..=1000"));
                }
                self.edit(|s| {
                    s.settings = settings;
                    Ok(Vec::new())
                })
            }
            Command::AddUniverse { universe } => self.edit(|s| {
                if s.universe(universe.id).is_some() {
                    return Err(CommandError::invalid(format!(
                        "universe {} already exists",
                        universe.id.0
                    )));
                }
                s.universes.push(universe);
                Ok(Vec::new())
            }),
            Command::RemoveUniverse { id } => self.edit(|s| {
                if s.universe(id).is_none() {
                    return Err(CommandError::not_found(id));
                }
                // Its fixtures go with it (undo restores both).
                let removed: Vec<FixtureId> = s
                    .fixtures
                    .iter()
                    .filter(|f| f.universe == id)
                    .map(|f| f.id)
                    .collect();
                s.fixtures.retain(|f| f.universe != id);
                leave_scenes(s, |t| match t {
                    Target::Fixture { id } => removed.contains(id),
                    _ => false,
                });
                for g in &mut s.groups {
                    g.members.retain(|m| !removed.contains(&m.fixture));
                }
                for p in &mut s.palettes {
                    p.per_fixture.retain(|e| !removed.contains(&e.fixture));
                }
                s.universes.retain(|u| u.id != id);
                Ok(if removed.is_empty() {
                    Vec::new()
                } else {
                    vec![format!(
                        "removed {} fixtures patched in universe {}",
                        removed.len(),
                        id.0
                    )]
                })
            }),
            Command::RenameUniverse { id, name } => self.edit(|s| {
                let name = name.trim().to_owned();
                if name.is_empty() {
                    return Err(CommandError::invalid("name cannot be empty"));
                }
                let u = s
                    .universes
                    .iter_mut()
                    .find(|u| u.id == id)
                    .ok_or_else(|| CommandError::not_found(id))?;
                u.name = name;
                Ok(Vec::new())
            }),
            Command::SetUniverseOutput { id, output } => self.edit(|s| {
                let u = s
                    .universes
                    .iter_mut()
                    .find(|u| u.id == id)
                    .ok_or_else(|| CommandError::not_found(id))?;
                u.output = output;
                Ok(Vec::new())
            }),
            Command::ImportDefinition { definition } => self.edit(|s| {
                if definition.modes.is_empty() || definition.channels.is_empty() {
                    return Err(CommandError::invalid(
                        "definition without modes or channels",
                    ));
                }
                let key = definition.key();
                // Embedded copy: replace an older one with the same key.
                s.definitions.retain(|d| d.key() != key);
                s.definitions.push(*definition);
                Ok(Vec::new())
            }),
            Command::PatchFixture { mut fixture, force } => self.edit(|s| {
                // Names are unique: a taken one gets the next free number.
                let fallback = s
                    .definition(&fixture.definition)
                    .map_or_else(|| "Fixture".to_owned(), |d| d.model.clone());
                let name = validate::unique_fixture_name(s, &fixture.name, fixture.id, &fallback);
                let renamed = (name != fixture.name.trim() && !fixture.name.trim().is_empty())
                    .then(|| {
                        format!(
                            "name \"{}\" already in use: renamed to \"{name}\"",
                            fixture.name.trim()
                        )
                    });
                fixture.name = name;
                let fp = validate::check_fixture(s, &fixture)
                    .map_err(|error| CommandError::Patch { error })?;
                let conflicts = validate::conflicts_for(s, &fixture, fp);
                let mut warnings = Vec::new();
                if !conflicts.is_empty() {
                    if !force {
                        return Err(CommandError::AddressConflicts { conflicts });
                    }
                    for c in &conflicts {
                        warnings.push(
                            PatchError::Conflict {
                                first: c.first,
                                second: c.second,
                                address: c.address,
                            }
                            .to_string(),
                        );
                    }
                }
                match s.fixtures.iter_mut().find(|f| f.id == fixture.id) {
                    Some(existing) => *existing = fixture,
                    None => s.fixtures.push(fixture),
                }
                warnings.extend(renamed);
                Ok(warnings)
            }),
            Command::UnpatchFixture { id } => self.edit(|s| {
                if s.fixture(id).is_none() {
                    return Err(CommandError::not_found(id));
                }
                s.fixtures.retain(|f| f.id != id);
                leave_scenes(s, |t| *t == Target::Fixture { id });
                for g in &mut s.groups {
                    g.members.retain(|m| m.fixture != id);
                }
                for p in &mut s.palettes {
                    p.per_fixture.retain(|e| e.fixture != id);
                }
                Ok(Vec::new())
            }),
            Command::SetGroup { group } => self.edit(|s| {
                let mut warnings = Vec::new();
                for m in &group.members {
                    if s.fixture(m.fixture).is_none() {
                        warnings.push(format!("group member {} is not patched", m.fixture.0));
                    }
                }
                match s.groups.iter_mut().find(|g| g.id == group.id) {
                    Some(g) => *g = group,
                    None => s.groups.push(group),
                }
                Ok(warnings)
            }),
            Command::RemoveGroup { id } => self.edit(|s| {
                if s.group(id).is_none() {
                    return Err(CommandError::not_found(id));
                }
                s.groups.retain(|g| g.id != id);
                leave_scenes(s, |t| *t == Target::Group { id });
                Ok(Vec::new())
            }),
            Command::SetPalette { palette } => {
                check_palette(&palette)?;
                self.edit(|s| {
                    match s.palettes.iter_mut().find(|p| p.id == palette.id) {
                        Some(p) => *p = palette,
                        None => s.palettes.push(palette),
                    }
                    Ok(Vec::new())
                })
            }
            Command::RemovePalette { id } => self.edit(|s| {
                if s.palette(id).is_none() {
                    return Err(CommandError::not_found(id));
                }
                s.palettes.retain(|p| p.id != id);
                Ok(Vec::new())
            }),
            Command::SetFunction { function } => {
                check_function(&function)?;
                self.edit(|s| {
                    if let Some(fid) = function.folder {
                        let folder = s.folder(fid).ok_or_else(|| CommandError::not_found(fid))?;
                        if folder.category != function.kind.category() {
                            return Err(CommandError::invalid(format!(
                                "folder \"{}\" holds {:?} functions",
                                folder.name, folder.category
                            )));
                        }
                    }
                    match s.functions.iter_mut().find(|f| f.id == function.id) {
                        Some(f) => *f = function,
                        None => s.functions.push(function),
                    }
                    Ok(Vec::new())
                })
            }
            Command::SetFunctionFolder { mut folder } => self.edit(|s| {
                folder.name = folder.name.trim().to_owned();
                if folder.name.is_empty() {
                    return Err(CommandError::invalid("folder name cannot be empty"));
                }
                if let Some(parent) = folder.parent {
                    let p = s
                        .folder(parent)
                        .ok_or_else(|| CommandError::not_found(parent))?;
                    if p.category != folder.category {
                        return Err(CommandError::invalid(
                            "parent folder holds another type of function",
                        ));
                    }
                    // Walk up from the new parent: reaching the folder itself is a loop.
                    let mut cursor = Some(parent);
                    let mut steps = 0usize;
                    while let Some(c) = cursor {
                        if c == folder.id || steps > s.function_folders.len() {
                            return Err(CommandError::invalid(
                                "a folder cannot be moved inside itself",
                            ));
                        }
                        cursor = s.folder(c).and_then(|f| f.parent);
                        steps += 1;
                    }
                }
                if let Some(existing) = s.folder(folder.id) {
                    let has_content = s.functions.iter().any(|f| f.folder == Some(folder.id))
                        || s.function_folders
                            .iter()
                            .any(|f| f.parent == Some(folder.id));
                    if existing.category != folder.category && has_content {
                        return Err(CommandError::invalid(
                            "cannot change the type of a folder that is not empty",
                        ));
                    }
                }
                match s.function_folders.iter_mut().find(|f| f.id == folder.id) {
                    Some(f) => *f = folder,
                    None => s.function_folders.push(folder),
                }
                Ok(Vec::new())
            }),
            Command::RemoveFunctionFolder { id } => self.edit(|s| {
                let parent = s
                    .folder(id)
                    .ok_or_else(|| CommandError::not_found(id))?
                    .parent;
                for f in &mut s.functions {
                    if f.folder == Some(id) {
                        f.folder = parent;
                    }
                }
                for f in &mut s.function_folders {
                    if f.parent == Some(id) {
                        f.parent = parent;
                    }
                }
                s.function_folders.retain(|f| f.id != id);
                Ok(Vec::new())
            }),
            Command::RemoveFunction { id } => self.edit(|s| {
                if s.function(id).is_none() {
                    return Err(CommandError::not_found(id));
                }
                s.functions.retain(|f| f.id != id);
                Ok(Vec::new())
            }),
            Command::SetVirtualConsole { console } => self.edit(|s| {
                s.virtual_console = console;
                Ok(Vec::new())
            }),
            Command::SetInputDevices { devices } => self.edit(|s| {
                s.input_devices = devices;
                Ok(Vec::new())
            }),
            Command::SetInputMappings { mappings } => self.edit(|s| {
                s.input_mappings = mappings;
                Ok(Vec::new())
            }),
            Command::StoreProgrammerAsScene { id, name } => {
                if self.rt.programmer_entries.is_empty() {
                    return Err(CommandError::invalid("programmer is empty"));
                }
                let values: Vec<SceneValue> = self
                    .rt
                    .programmer_entries
                    .iter()
                    .filter(|e| !e.value.targets_service_channel())
                    .map(|e| SceneValue {
                        target: e.target,
                        value: ValueSource::Explicit {
                            value: e.value.clone(),
                        },
                    })
                    .collect();
                self.edit(|s| {
                    let id = id.unwrap_or_else(|| s.next_function_id());
                    // Overwriting a scene keeps it in its folder.
                    let folder = s.function(id).and_then(|f| f.folder);
                    let lights: Vec<Target> = values.iter().map(|v| v.target).collect();
                    let scene = Scene {
                        values,
                        ..Scene::default()
                    };
                    let function = Function {
                        id,
                        name,
                        kind: FunctionKind::Scene(add_scene_members(s, &scene, &lights)),
                        folder,
                    };
                    match s.functions.iter_mut().find(|f| f.id == id) {
                        Some(f) => *f = function,
                        None => s.functions.push(function),
                    }
                    Ok(Vec::new())
                })
            }
            Command::StoreProgrammerInPalette { id, name, kind } => {
                let entries = self.rt.programmer_entries.clone();
                self.edit(|s| {
                    let mut per_fixture: Vec<FixturePaletteValues> = Vec::new();
                    let mut push =
                        |fixture: FixtureId, v: &klight_model::AttributeValue| match per_fixture
                            .iter_mut()
                            .find(|e| e.fixture == fixture)
                        {
                            Some(e) => {
                                e.values.retain(|x| !x.same_slot(v));
                                e.values.push(v.clone());
                            }
                            None => per_fixture.push(FixturePaletteValues {
                                fixture,
                                values: vec![v.clone()],
                            }),
                        };
                    let mut generic = Vec::new();
                    for e in entries.iter().filter(|e| kind.accepts(e.value.kind())) {
                        match e.target {
                            Target::Fixture { id } | Target::Head { id, .. } => push(id, &e.value),
                            // Never in the programmer.
                            Target::Scene => {}
                            Target::Group { id } => {
                                // Per fixture the concrete value (a fan becomes each
                                // head's own position); the group value is the fallback.
                                if let Some(g) = s.group(id) {
                                    let len = u16::try_from(g.members.len()).unwrap_or(u16::MAX);
                                    for (i, m) in g.members.iter().enumerate() {
                                        let order = u16::try_from(i).unwrap_or(u16::MAX);
                                        push(m.fixture, &e.value.for_member(order, len));
                                    }
                                }
                                generic.retain(|x: &klight_model::AttributeValue| {
                                    !x.same_slot(&e.value)
                                });
                                generic.push(e.value.clone());
                            }
                        }
                    }
                    if per_fixture.is_empty() && generic.is_empty() {
                        return Err(CommandError::invalid(
                            "programmer has no values of that kind",
                        ));
                    }
                    let id: PaletteId = id.unwrap_or_else(|| s.next_palette_id());
                    let palette = Palette {
                        id,
                        name,
                        kind,
                        generic,
                        per_fixture,
                    };
                    match s.palettes.iter_mut().find(|p| p.id == id) {
                        Some(p) => *p = palette,
                        None => s.palettes.push(palette),
                    }
                    Ok(Vec::new())
                })
            }
            // Runtime commands never reach here.
            _ => Err(CommandError::invalid("not an edit command")),
        }
    }

    fn ctx_parts(&mut self) -> (Ctx<'_>, &mut Vec<Playback>, &mut Vec<SceneInstance>) {
        let rt = &mut self.rt;
        (
            Ctx {
                now_us: self.now_us,
                music: &rt.music,
                base: &rt.base,
                channels: &self.compiled.channels,
            },
            &mut rt.playbacks,
            &mut rt.pool,
        )
    }

    fn is_active(&self, id: FunctionId) -> bool {
        self.rt.playbacks.iter().any(|p| p.id == id && !p.stopping)
    }

    pub fn start_function(&mut self, id: FunctionId) -> Result<(), CommandError> {
        if self.is_active(id) {
            return Ok(());
        }
        let cf = self
            .compiled
            .function(id)
            .cloned()
            .ok_or_else(|| CommandError::not_found(id))?;
        // A stopping copy is replaced by the new start.
        let now = self.now_us;
        let order = self.rt.next_order;
        self.rt.next_order += 1;
        let level = self
            .rt
            .pending_levels
            .remove(&id)
            .map_or(u16::MAX, f32_to16);
        let started_beat = self.rt.music.beat_at(now);
        let kind = match cf {
            CompiledFunction::Scene(s) => PlaybackKind::Scene(s),
            CompiledFunction::Chaser(c) => {
                if c.steps.is_empty() {
                    return Err(CommandError::invalid("chaser has no valid steps"));
                }
                PlaybackKind::Chaser(ChaserState {
                    chaser: c,
                    step: 0,
                    step_start_us: now,
                    step_start_beat: started_beat,
                    forward: true,
                })
            }
            CompiledFunction::CueList(l) => PlaybackKind::CueList(CueState {
                list: l,
                current: None,
                cue_started_us: now,
            }),
            CompiledFunction::Movement(m) => PlaybackKind::Movement(m),
            CompiledFunction::Matrix(m) => PlaybackKind::Matrix(m),
            CompiledFunction::Collection(c) => {
                if c.members.is_empty() {
                    return Err(CommandError::invalid(
                        "collection has no playable functions",
                    ));
                }
                PlaybackKind::Collection(c)
            }
        };
        let mut p = Playback {
            id,
            level,
            order,
            kind,
            instances: Vec::with_capacity(4),
            stopping: false,
            started_us: now,
            started_beat,
            paused_since: None,
            groups: 0,
            effect_until: None,
        };
        {
            let (ctx, _, pool) = self.ctx_parts();
            p.start(pool, &ctx);
        }
        // Instances of the old stopping copy keep fading under the new one.
        self.rt.playbacks.push(p);
        Ok(())
    }

    pub fn stop_function(&mut self, id: FunctionId) -> Result<(), CommandError> {
        let (ctx, playbacks, _) = self.ctx_parts();
        let mut found = false;
        let mut remove_now = Vec::new();
        for (i, p) in playbacks.iter_mut().enumerate() {
            if p.id == id && !p.stopping {
                found = true;
                if p.stop(&ctx) {
                    remove_now.push(i);
                }
            }
        }
        for i in remove_now.into_iter().rev() {
            playbacks.remove(i);
        }
        if found {
            Ok(())
        } else {
            Err(CommandError::not_found(format!("active function {}", id.0)))
        }
    }

    fn with_playback<R>(
        &mut self,
        id: FunctionId,
        f: impl FnOnce(&mut Playback, &mut Vec<SceneInstance>, &Ctx<'_>) -> R,
    ) -> Option<R> {
        let (ctx, playbacks, pool) = self.ctx_parts();
        let p = playbacks
            .iter_mut()
            .rev()
            .find(|p| p.id == id && !p.stopping)?;
        Some(f(p, pool, &ctx))
    }

    fn stop_solo_siblings(&mut self, widget: WidgetId) {
        let show = self.show.clone();
        let Some(parent) = show.virtual_console.parent_of(widget) else {
            return;
        };
        if parent.kind != WidgetKind::SoloFrame {
            return;
        }
        for sibling in &parent.children {
            if sibling.id == widget {
                continue;
            }
            if let Some(f) = sibling.function {
                let _ = self.stop_function(f);
            }
        }
    }

    fn apply_widget(&mut self, cmd: Command) -> Result<Vec<String>, CommandError> {
        let show = self.show.clone();
        let id = match &cmd {
            Command::WidgetPress { id }
            | Command::WidgetRelease { id }
            | Command::WidgetSetValue { id, .. } => *id,
            _ => return Ok(Vec::new()),
        };
        let w = show
            .virtual_console
            .find(id)
            .ok_or_else(|| CommandError::not_found(id))?;
        match (&w.kind, cmd) {
            (WidgetKind::Button { action }, Command::WidgetPress { .. }) => match action {
                ButtonAction::Toggle => {
                    let f = w
                        .function
                        .ok_or_else(|| CommandError::invalid("button has no function"))?;
                    if self.is_active(f) {
                        self.stop_function(f)?;
                    } else {
                        self.stop_solo_siblings(id);
                        self.start_function(f)?;
                    }
                }
                ButtonAction::Flash => {
                    let f = w
                        .function
                        .ok_or_else(|| CommandError::invalid("button has no function"))?;
                    self.stop_solo_siblings(id);
                    self.start_function(f)?;
                }
                ButtonAction::Blackout => self.rt.blackout = !self.rt.blackout,
                ButtonAction::StopAll => self.stop_all(),
                ButtonAction::TapTempo => self.rt.music.tap(self.now_us),
            },
            (
                WidgetKind::Button {
                    action: ButtonAction::Flash,
                },
                Command::WidgetRelease { .. },
            ) => {
                if let Some(f) = w.function {
                    let _ = self.stop_function(f);
                }
            }
            (WidgetKind::CueList, Command::WidgetPress { .. }) => {
                let f = w
                    .function
                    .ok_or_else(|| CommandError::invalid("widget has no cue list"))?;
                self.cue_go(f)?;
            }
            (WidgetKind::Slider { mode }, Command::WidgetSetValue { value, .. }) => {
                let v = finite01(value, "value")?;
                match mode {
                    SliderMode::GrandMaster => self.rt.grand_master = v,
                    SliderMode::Playback | SliderMode::Submaster => {
                        let f = w
                            .function
                            .ok_or_else(|| CommandError::invalid("slider has no function"))?;
                        self.set_playback_level(f, v)?;
                    }
                }
            }
            (WidgetKind::Knob, Command::WidgetSetValue { value, .. }) => {
                let v = finite01(value, "value")?;
                if let Some(f) = w.function {
                    self.set_playback_level(f, v)?;
                }
            }
            (WidgetKind::SpeedDial, Command::WidgetPress { .. }) => self.rt.music.tap(self.now_us),
            (WidgetKind::SpeedDial, Command::WidgetSetValue { value, .. }) => {
                if !value.is_finite() || value <= 0.0 {
                    return Err(CommandError::invalid("BPM must be positive"));
                }
                self.rt.music.set_manual_bpm(self.now_us, f64::from(value));
            }
            _ => {}
        }
        Ok(Vec::new())
    }

    /// Slider semantics: > 0 starts the function at that level, 0 stops it.
    fn set_playback_level(&mut self, f: FunctionId, v: f32) -> Result<(), CommandError> {
        if v <= 0.0 {
            if self.is_active(f) {
                self.stop_function(f)?;
            }
            return Ok(());
        }
        if !self.is_active(f) {
            self.rt.pending_levels.insert(f, v);
            self.start_function(f)?;
        }
        self.with_playback(f, |p, _, _| p.level = f32_to16(v));
        Ok(())
    }

    fn stop_all(&mut self) {
        let ids: Vec<FunctionId> = self
            .rt
            .playbacks
            .iter()
            .filter(|p| !p.stopping)
            .map(|p| p.id)
            .collect();
        for id in ids {
            let _ = self.stop_function(id);
        }
    }

    fn cue_go(&mut self, id: FunctionId) -> Result<(), CommandError> {
        if !matches!(
            self.compiled.function(id),
            Some(CompiledFunction::CueList(_))
        ) {
            return Err(CommandError::not_found(format!("cue list {}", id.0)));
        }
        if !self.is_active(id) {
            return self.start_function(id);
        }
        self.with_playback(id, |p, pool, ctx| {
            if p.paused_since.is_some() {
                p.toggle_pause(ctx.now_us);
            } else {
                p.cue_go(pool, ctx);
            }
        });
        Ok(())
    }

    fn apply_runtime(&mut self, source: &CommandSource, cmd: Command) -> EditResult {
        match cmd {
            Command::StartFunction { id } => self.start_function(id)?,
            Command::StopFunction { id } => self.stop_function(id)?,
            Command::StopAll => self.stop_all(),
            Command::SetFunctionLevel { id, level } => {
                let level = finite01(level, "level")?;
                if self.compiled.function(id).is_none() {
                    return Err(CommandError::not_found(id));
                }
                if self
                    .with_playback(id, |p, _, _| p.level = f32_to16(level))
                    .is_none()
                {
                    self.rt.pending_levels.insert(id, level);
                }
            }
            Command::CueGo { id } => self.cue_go(id)?,
            Command::CueBack { id } => {
                self.with_playback(id, |p, pool, ctx| p.cue_back(pool, ctx))
                    .ok_or_else(|| CommandError::not_found(format!("active cue list {}", id.0)))?;
            }
            Command::CuePause { id } => {
                self.with_playback(id, |p, _, ctx| p.toggle_pause(ctx.now_us))
                    .ok_or_else(|| CommandError::not_found(format!("active cue list {}", id.0)))?;
            }
            Command::CueGoTo { id, index } => {
                if !self.is_active(id) {
                    self.start_function(id)?;
                }
                let ok = self
                    .with_playback(id, |p, pool, ctx| p.cue_goto(pool, ctx, index as usize))
                    .unwrap_or(false);
                if !ok {
                    return Err(CommandError::not_found(format!("cue {index}")));
                }
            }
            Command::ChaserGoTo { id, index } => {
                let is_chaser = matches!(
                    self.show.function(id).map(|f| &f.kind),
                    Some(klight_model::FunctionKind::Chaser(_))
                );
                if !is_chaser {
                    return Err(CommandError::not_found(format!("chaser {}", id.0)));
                }
                if !self.is_active(id) {
                    self.start_function(id)?;
                }
                let ok = self
                    .with_playback(id, |p, pool, ctx| p.chaser_goto(pool, ctx, index as usize))
                    .unwrap_or(false);
                if !ok {
                    return Err(CommandError::not_found(format!("chaser step {index}")));
                }
            }
            Command::ProgrammerSet { target, value } => {
                if value.targets_service_channel() {
                    return Err(CommandError::invalid(
                        "service channels are controlled by dedicated actions",
                    ));
                }
                if target == Target::Scene {
                    return Err(CommandError::invalid(
                        "only the values of a scene can mean the whole scene",
                    ));
                }
                if self.compiled.expand_target(&self.show, &target).is_empty() {
                    return Err(CommandError::not_found(format!("target {target:?}")));
                }
                self.rt
                    .programmer_entries
                    .retain(|e| !(e.target == target && e.value.same_slot(&value)));
                self.rt
                    .programmer_entries
                    .push(ProgrammerEntry { target, value });
                return Ok(self.rebuild_programmer());
            }
            Command::ProgrammerClear => {
                self.rt.programmer_entries.clear();
                self.rebuild_programmer();
                self.rt.raw.iter_mut().for_each(|r| r.fill(RAW_UNSET));
            }
            Command::SetChannel {
                universe,
                address,
                value,
            } => {
                let i = self
                    .compiled
                    .universe_index(universe)
                    .ok_or_else(|| CommandError::not_found(universe))?;
                if address == 0 || usize::from(address) > UNIVERSE_SIZE {
                    return Err(CommandError::invalid("address must be in 1..=512"));
                }
                if let Some(slot) = self
                    .rt
                    .raw
                    .get_mut(i)
                    .and_then(|r| r.get_mut(usize::from(address) - 1))
                {
                    *slot = value.map_or(RAW_UNSET, u16::from);
                }
            }
            Command::ClearChannels { universe } => match universe {
                Some(u) => {
                    let i = self
                        .compiled
                        .universe_index(u)
                        .ok_or_else(|| CommandError::not_found(u))?;
                    if let Some(r) = self.rt.raw.get_mut(i) {
                        r.fill(RAW_UNSET);
                    }
                }
                None => self.rt.raw.iter_mut().for_each(|r| r.fill(RAW_UNSET)),
            },
            Command::SetGrandMaster { level } => {
                self.rt.grand_master = finite01(level, "grand master")?
            }
            Command::SetBlackout { on } => self.rt.blackout = on,
            Command::ReleaseControl { control } => {
                self.rt.holds.remove(&control);
            }
            Command::TapTempo => self.rt.music.tap(self.now_us),
            Command::SetBpm { bpm } => {
                if !bpm.is_finite() || bpm <= 0.0 {
                    return Err(CommandError::invalid("BPM must be positive"));
                }
                self.rt.music.set_manual_bpm(self.now_us, bpm);
            }
            Command::ClockPulse { at_us } => self.rt.music.clock_pulse(at_us.min(self.now_us)),
            Command::ExternalTempo { bpm, beat, .. } => {
                if !bpm.is_finite() || bpm <= 0.0 {
                    return Err(CommandError::invalid("BPM must be positive"));
                }
                self.rt.music.external(self.now_us, bpm, beat);
            }
            cmd @ (Command::WidgetPress { .. }
            | Command::WidgetRelease { .. }
            | Command::WidgetSetValue { .. }) => {
                return self.apply_widget(cmd);
            }
            Command::SetMode { mode, pin } => {
                if mode == OperatingMode::Design && source.is_remote() {
                    if let Some(required) = &self.show.settings.design_pin {
                        if pin.as_deref() != Some(required.as_str()) {
                            return Err(CommandError::BadPin);
                        }
                    }
                }
                self.rt.mode = mode;
                if mode == OperatingMode::Show {
                    self.end_preview();
                }
            }
            Command::SetPreview { values, members } => {
                if self.rt.mode == OperatingMode::Show {
                    return Err(CommandError::ShowModeLocked);
                }
                self.rt.preview_values = values;
                self.rt.preview_members = members;
                self.rt.preview_until = Some(self.now_us.saturating_add(PREVIEW_HOLD_US));
                self.rebuild_preview();
            }
            Command::ClearPreview => self.end_preview(),
            _ => return Err(CommandError::invalid("unsupported command")),
        }
        Ok(Vec::new())
    }

    // ------------------------------------------------------------------- tick

    /// Advances time and computes the output frames. Allocation-free in the
    /// steady state (no commands, no discrete state change).
    pub fn tick(&mut self, now_us: u64) {
        self.now_us = self.now_us.max(now_us);
        let now = self.now_us;
        if self.rt.music.update(now) {
            self.dirty = true;
        }

        if self.rt.preview_until.is_some_and(|until| now >= until) {
            self.end_preview();
        }

        // 1. Advance playbacks.
        {
            let rt = &mut self.rt;
            let ctx = Ctx {
                now_us: now,
                music: &rt.music,
                base: &rt.base,
                channels: &self.compiled.channels,
            };
            for p in rt.playbacks.iter_mut() {
                if p.advance(&mut rt.pool, &ctx) {
                    self.dirty = true;
                }
            }
            let before = rt.playbacks.len();
            rt.playbacks.retain(|p| !p.is_done());
            if rt.playbacks.len() != before {
                self.dirty = true;
            }
        }

        // 2. Render layers: the scenes first (also those fading out), as a
        // relative movement adds to what they give.
        let channels = &self.compiled.channels;
        self.rt.base.clear();
        self.rt.effects.clear();
        for p in &self.rt.playbacks {
            p.render_base(now, channels, &mut self.rt.base, &mut self.rt.mix);
        }
        let bpm = self.rt.music.bpm();
        for p in &self.rt.playbacks {
            // Effects stop at once, unless they have a fade out of their own.
            if p.stopping && p.effect_until.is_none() {
                continue;
            }
            // An effect runs from the start of its function or chaser step.
            let (start_us, start_beat, t) = match &p.kind {
                PlaybackKind::Chaser(c) => (c.step_start_us, c.step_start_beat, p.render_time(now)),
                _ => (p.started_us, p.started_beat, now),
            };
            let turns = |period| cycle_turns(period, start_us, start_beat, t, &self.rt.music);
            let remaining = p.effect_until.map(|until| until.saturating_sub(now));
            let base = &self.rt.base;
            let movement = |m: &CompiledMovement, layer: &mut Layer| {
                let size = movement_size(&m.effect, bpm, t.saturating_sub(start_us), remaining);
                render_movement(m, turns(m.effect.period), size, base, channels, layer);
            };
            let matrix = |m: &CompiledMatrix, layer: &mut Layer| {
                let e = &m.effect;
                let size = effect_size(
                    e.fade_in,
                    e.fade_out,
                    bpm,
                    t.saturating_sub(start_us),
                    remaining,
                );
                let level = scale(p.level, f32_to16(size));
                render_matrix(m, turns(e.period), level, channels, layer);
            };
            let effects = &mut self.rt.effects;
            let collection = match &p.kind {
                PlaybackKind::Movement(m) => {
                    movement(m, effects);
                    None
                }
                PlaybackKind::Matrix(m) => {
                    matrix(m, effects);
                    None
                }
                PlaybackKind::Collection(c) => Some(c),
                PlaybackKind::Chaser(state) => {
                    match state.chaser.steps.get(state.step).map(|s| &s.content) {
                        Some(StepContent::Movement(m)) => {
                            movement(m, effects);
                            None
                        }
                        Some(StepContent::Matrix(m)) => {
                            matrix(m, effects);
                            None
                        }
                        Some(StepContent::Collection(c)) => Some(c),
                        Some(StepContent::Scene(_)) | None => None,
                    }
                }
                PlaybackKind::Scene(_) | PlaybackKind::CueList(_) => None,
            };
            if let Some(c) = collection {
                c.movements.iter().for_each(|m| movement(m, effects));
                c.matrices.iter().for_each(|m| matrix(m, effects));
            }
        }

        // 3. Merge and write frames.
        merge(
            channels,
            [&self.rt.base, &self.rt.effects, &self.rt.programmer],
            &self.rt.preview,
            f32_to16(self.rt.grand_master),
            self.rt.blackout,
            &mut self.rt.merged,
            &mut self.frames,
        );
        apply_raw_channels(
            &self.rt.raw,
            &self.compiled.intensity_addr,
            f32_to16(self.rt.grand_master),
            self.rt.blackout,
            &mut self.frames,
        );
        hold_position_limits(channels, &mut self.frames);
        for (frame, slot) in self.frames.iter().zip(&self.outputs) {
            slot.write(frame);
        }

        self.shared
            .beat
            .store(self.rt.music.beat_at(now), Ordering::Relaxed);
        if self.dirty {
            self.publish_snapshot();
        }
    }

    /// Restores a persisted runtime state (after a restart). Fades are
    /// skipped: the stage goes straight back to where it was.
    pub fn restore(&mut self, now_us: u64, state: &RuntimeState) -> Vec<String> {
        self.now_us = self.now_us.max(now_us);
        let mut warnings = Vec::new();
        self.rt.mode = state.mode;
        self.rt.grand_master = if state.grand_master.is_finite() {
            state.grand_master.clamp(0.0, 1.0)
        } else {
            1.0
        };
        self.rt.blackout = state.blackout;
        self.rt.music.restore(self.now_us, &state.tempo);
        self.rt.programmer_entries = state.programmer.clone();
        warnings.extend(self.rebuild_programmer());
        for c in &state.channels {
            let i = self.compiled.universe_index(c.universe);
            let slot = i
                .and_then(|i| self.rt.raw.get_mut(i))
                .and_then(|r| r.get_mut(usize::from(c.address).wrapping_sub(1)));
            match slot {
                Some(s) => *s = u16::from(c.value),
                None => warnings.push(format!(
                    "raw channel {}/{} not restored",
                    c.universe.0, c.address
                )),
            }
        }
        for a in &state.active {
            if let Err(e) = self.start_function(a.id) {
                warnings.push(format!("could not restore function {}: {e}", a.id.0));
                continue;
            }
            let level = if a.level.is_finite() {
                a.level.clamp(0.0, 1.0)
            } else {
                1.0
            };
            let now = self.now_us;
            let bpm = self.rt.music.bpm();
            let id = a.id;
            let position = a.position;
            let (ctx, playbacks, pool) = self.ctx_parts();
            if let Some(p) = playbacks.iter_mut().rev().find(|p| p.id == id) {
                p.level = f32_to16(level);
                match (&mut p.kind, position) {
                    (PlaybackKind::CueList(state), Some(pos)) => {
                        if let Some(cue) = state.list.cues.get(pos as usize) {
                            state.current = Some(pos as usize);
                            let scene = cue.scene.clone();
                            p.instances.clear();
                            let mut inst = SceneInstance::default();
                            inst.rebind(scene, now, bpm, ctx.base, ctx.channels);
                            p.instances.push(inst);
                        }
                    }
                    (PlaybackKind::Chaser(state), Some(pos)) => {
                        // `pos` is the step's place in the show's chaser.
                        let found = state
                            .chaser
                            .steps
                            .iter()
                            .enumerate()
                            .find(|(_, s)| s.index == pos as usize)
                            .map(|(i, s)| (i, s.content.clone()));
                        if let Some((i, content)) = found {
                            state.step = i;
                            p.show_now(pool, &content, &ctx);
                        }
                    }
                    _ => p.snap(&ctx),
                }
                if a.paused {
                    p.toggle_pause(now);
                }
            }
        }
        // Publish right away: nothing may observe (or persist) the
        // pre-restore state once restore() has returned.
        self.publish_snapshot();
        warnings
    }
}

fn check_palette(p: &Palette) -> Result<(), CommandError> {
    let all = p
        .generic
        .iter()
        .chain(p.per_fixture.iter().flat_map(|e| e.values.iter()));
    for v in all {
        if v.targets_service_channel() {
            return Err(CommandError::invalid(
                "service channels cannot be stored in palettes",
            ));
        }
        if !p.kind.accepts(v.kind()) {
            return Err(CommandError::invalid(format!(
                "value {v:?} does not belong in a {:?} palette",
                p.kind
            )));
        }
    }
    Ok(())
}

fn check_function(f: &Function) -> Result<(), CommandError> {
    let check_values = |values: &[SceneValue]| -> Result<(), CommandError> {
        for v in values {
            let service = match &v.value {
                ValueSource::Explicit { value } => value.targets_service_channel(),
                ValueSource::Channel { channel, .. } => {
                    if !matches!(v.target, Target::Fixture { .. }) {
                        return Err(CommandError::invalid(
                            "channel values need a single fixture as target",
                        ));
                    }
                    matches!(
                        channel,
                        ChannelRef::Dmx {
                            attribute: Attribute::Maintenance,
                            ..
                        }
                    )
                }
                ValueSource::Palette { .. } => false,
            };
            if service {
                return Err(CommandError::invalid(
                    "service channels cannot be stored in scenes or cues",
                ));
            }
        }
        Ok(())
    };
    match &f.kind {
        FunctionKind::Scene(s) => {
            let light = |t: &Target| matches!(t, Target::Fixture { .. } | Target::Group { .. });
            if !s.members.iter().all(light) {
                return Err(CommandError::invalid(
                    "the lights of a scene are groups and fixtures",
                ));
            }
            check_values(&s.values)
        }
        FunctionKind::CueList(c) => c.cues.iter().try_for_each(|cue| {
            if cue.values.iter().any(|v| v.target == Target::Scene) {
                return Err(CommandError::invalid(
                    "only the values of a scene can mean the whole scene",
                ));
            }
            check_values(&cue.values)
        }),
        FunctionKind::Chaser(c) => {
            if c.steps.iter().any(|s| s.scene == f.id) {
                return Err(CommandError::invalid("chaser cannot reference itself"));
            }
            Ok(())
        }
        // Members of the wrong type are dropped with a warning when compiled.
        FunctionKind::Collection(_)
        | FunctionKind::MovementEffect(_)
        | FunctionKind::MatrixEffect(_) => Ok(()),
    }
}

/// Takes the lights `gone` picks out of every scene.
fn leave_scenes(show: &mut Show, gone: impl Fn(&Target) -> bool) {
    for f in &mut show.functions {
        if let FunctionKind::Scene(s) = &mut f.kind {
            s.members.retain(|m| !gone(m));
        }
    }
}

/// Marker for "raw channel not set".
const RAW_UNSET: u16 = u16::MAX;

/// Simple Desk raw channels override the merged output; intensity
/// addresses stay subject to grand master and blackout.
pub fn apply_raw_channels(
    raw: &[[u16; UNIVERSE_SIZE]],
    intensity: &[[bool; UNIVERSE_SIZE]],
    grand_master: u16,
    blackout: bool,
    frames: &mut [DmxFrame],
) {
    for (u, (r, frame)) in raw.iter().zip(frames.iter_mut()).enumerate() {
        let class = intensity.get(u);
        for (i, (v, out)) in r.iter().zip(frame.iter_mut()).enumerate() {
            if *v == RAW_UNSET {
                continue;
            }
            let is_intensity = class.and_then(|c| c.get(i)).copied().unwrap_or(false);
            let v8 = u8::try_from(*v).unwrap_or(u8::MAX);
            *out = if !is_intensity {
                v8
            } else if blackout {
                0
            } else {
                u8::try_from((u32::from(v8) * u32::from(grand_master) + 32767) / 65535)
                    .unwrap_or(u8::MAX)
            };
        }
    }
}

/// Holds the pan and tilt of the fixtures with position limits inside them in
/// the frames about to be sent: from whatever layer they come, and from the
/// raw channels of the Simple Desk, which are written into the frames after
/// the merge. It is the last word on the lights. A channel of one byte has no finer step than 256
/// values: its bounds are rounded inwards.
pub fn hold_position_limits(channels: &[LogicalChannel], frames: &mut [DmxFrame]) {
    for lc in channels {
        if (lc.min, lc.max) == (0, u16::MAX) || lc.coarse == NO_ADDR {
            continue;
        }
        let Some(frame) = frames.get_mut(usize::from(lc.universe)) else {
            continue;
        };
        let coarse = frame.get(usize::from(lc.coarse)).copied().unwrap_or(0);
        if lc.fine == NO_ADDR {
            let high = (lc.max >> 8) as u8;
            let low = u8::try_from((u32::from(lc.min) + 255) >> 8)
                .unwrap_or(u8::MAX)
                .min(high);
            if let Some(b) = frame.get_mut(usize::from(lc.coarse)) {
                *b = coarse.clamp(low, high);
            }
            continue;
        }
        let fine = frame.get(usize::from(lc.fine)).copied().unwrap_or(0);
        let [hi, lo] = u16::from_be_bytes([coarse, fine])
            .clamp(lc.min, lc.max)
            .to_be_bytes();
        if let Some(b) = frame.get_mut(usize::from(lc.coarse)) {
            *b = hi;
        }
        if let Some(b) = frame.get_mut(usize::from(lc.fine)) {
            *b = lo;
        }
    }
}

/// Layer merge: HTP for intensity, LTP (top layer wins) for everything
/// else; what `over` sets replaces the result; then grand master and blackout;
/// then virtual dimmers.
pub fn merge(
    channels: &[LogicalChannel],
    layers: [&Layer; 3],
    over: &Layer,
    grand_master: u16,
    blackout: bool,
    merged: &mut [u16],
    frames: &mut [DmxFrame],
) {
    for (ch, (lc, out)) in channels.iter().zip(merged.iter_mut()).enumerate() {
        let v = match lc.merge {
            Merge::Htp => layers
                .iter()
                .filter_map(|l| l.get(ch))
                .max()
                .unwrap_or(lc.default),
            Merge::Ltp => layers
                .iter()
                .rev()
                .find_map(|l| l.get(ch))
                .unwrap_or(lc.default),
        };
        let v = over.get(ch).unwrap_or(v);
        *out = if lc.role.is_intensity() {
            if blackout {
                0
            } else {
                scale(v, grand_master)
            }
        } else {
            v
        };
    }
    for f in frames.iter_mut() {
        f.fill(0);
    }
    for (lc, &v) in channels.iter().zip(merged.iter()) {
        if lc.coarse == NO_ADDR {
            continue;
        }
        let v = match lc.role {
            Role::Color { vdim } if vdim != NO_CHANNEL => {
                scale(v, merged.get(vdim as usize).copied().unwrap_or(u16::MAX))
            }
            _ => v,
        };
        let Some(frame) = frames.get_mut(usize::from(lc.universe)) else {
            continue;
        };
        let [hi, lo] = v.to_be_bytes();
        if let Some(b) = frame.get_mut(usize::from(lc.coarse)) {
            *b = hi;
        }
        if lc.fine != NO_ADDR {
            if let Some(b) = frame.get_mut(usize::from(lc.fine)) {
                *b = lo;
            }
        }
    }
}
