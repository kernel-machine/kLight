//! Compilation of a [`Show`] into flat, index-based structures used by the
//! tick loop. Compilation happens on show changes (command path), never in
//! the steady-state tick.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use klight_fixtures::{profile::to16, Converter, FixtureProfile, Resolved};
use klight_model::function::{Direction, RunMode};
use klight_model::patch::ColorCalibration;
use klight_model::UNIVERSE_SIZE;
use klight_model::{
    attributes::{kind_of_attribute, AttributeKind},
    validate, Attribute, AttributeValue, ChannelRef, Chaser, Collection, CueFollow, CueList,
    FixtureId, FunctionCategory, FunctionId, FunctionKind, MatrixEffect, MovementEffect,
    Orientation, Scene, SceneValue, Show, Spread, Target, TimeValue, Timing, UniverseId,
    ValueSource,
};

/// Marker for "no DMX address" (virtual channels).
pub const NO_ADDR: u16 = u16::MAX;
/// Marker for "no channel".
pub const NO_CHANNEL: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merge {
    Htp,
    Ltp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Real intensity channel (HTP, scaled by grand master, zeroed by blackout).
    Dimmer,
    /// Intensity of a head without dimmer: scales its color channels.
    VirtualDimmer,
    /// Additive color emitter, optionally governed by a virtual dimmer.
    Color {
        vdim: u32,
    },
    Other,
}

impl Role {
    pub fn is_intensity(self) -> bool {
        matches!(self, Role::Dimmer | Role::VirtualDimmer)
    }
}

/// One logical channel: a coarse DMX address plus optional fine byte, or a
/// virtual channel without address.
#[derive(Debug, Clone, PartialEq)]
pub struct LogicalChannel {
    pub universe: u16,
    pub coarse: u16,
    pub fine: u16,
    pub merge: Merge,
    pub role: Role,
    pub default: u16,
    pub fadeable: bool,
    /// Index into `CompiledShow::fixtures`.
    pub fixture: u32,
    pub attribute: Attribute,
}

#[derive(Debug, Clone)]
pub struct CompiledHead {
    pub vdim: u32,
}

#[derive(Debug, Clone)]
pub struct CompiledFixture {
    pub id: FixtureId,
    pub first_channel: u32,
    pub heads: Vec<CompiledHead>,
    pub profile: Arc<FixtureProfile>,
    pub definition: usize,
    pub orientation: Orientation,
    pub calibration: Option<ColorCalibration>,
}

/// Bit of the coarse byte in [`SceneEntry::set`] and [`SceneEntry::manual`].
pub const COARSE_BYTE: u8 = 1;
/// Bit of the fine byte in [`SceneEntry::set`] and [`SceneEntry::manual`].
pub const FINE_BYTE: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneEntry {
    pub channel: u32,
    pub value: u16,
    pub kind: AttributeKind,
    /// Position of the head in the target group (for spread timing).
    pub order: u16,
    pub group_len: u16,
    /// Extra delay for this entry (move in black presets).
    pub delay: TimeValue,
    /// Bytes the scene sets, by intention or by hand.
    pub set: u8,
    /// Bytes set by hand.
    pub manual: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledScene {
    pub entries: Vec<SceneEntry>,
    /// Timing per attribute kind, indexed by [`kind_index`].
    pub timing: [Timing; 4],
    pub spread: Spread,
}

pub fn kind_index(k: AttributeKind) -> usize {
    match k {
        AttributeKind::Intensity => 0,
        AttributeKind::Color => 1,
        AttributeKind::Position => 2,
        AttributeKind::Beam => 3,
    }
}

impl CompiledScene {
    pub fn timing_for(&self, k: AttributeKind) -> Timing {
        self.timing.get(kind_index(k)).copied().unwrap_or_default()
    }
}

/// What a chaser step plays.
#[derive(Debug, Clone)]
pub enum StepContent {
    /// A copy of the scene with the step's crossfade as its timing.
    Scene(Arc<CompiledScene>),
    Movement(Arc<CompiledMovement>),
    Matrix(Arc<CompiledMatrix>),
    /// Its scenes take the step's crossfade as their timing.
    Collection(Arc<CompiledCollection>),
}

impl StepContent {
    /// The scenes the step plays.
    pub fn scenes(&self) -> &[Arc<CompiledScene>] {
        match self {
            StepContent::Scene(s) => std::slice::from_ref(s),
            StepContent::Collection(c) => &c.scenes,
            StepContent::Movement(_) | StepContent::Matrix(_) => &[],
        }
    }
}

#[derive(Debug, Clone)]
pub struct CompiledStep {
    /// Position of the step in the show's chaser: invalid steps are dropped,
    /// so it can differ from the position among the compiled steps.
    pub index: usize,
    /// Function the step plays.
    pub source: FunctionId,
    pub content: StepContent,
    pub hold: TimeValue,
    pub fade: TimeValue,
}

#[derive(Debug, Clone)]
pub struct CompiledChaser {
    pub steps: Vec<CompiledStep>,
    pub direction: Direction,
    pub run_mode: RunMode,
}

#[derive(Debug, Clone)]
pub struct CompiledCue {
    pub scene: Arc<CompiledScene>,
    pub follow: CueFollow,
    pub number: f32,
}

#[derive(Debug, Clone)]
pub struct CompiledCueList {
    pub cues: Vec<CompiledCue>,
}

#[derive(Debug, Clone, Copy)]
pub struct MoveHead {
    pub pan: u32,
    pub tilt: u32,
    pub pan_range: f32,
    pub tilt_range: f32,
    pub orientation: Orientation,
    /// Phase offset of this head, in turns (0..1).
    pub phase: f32,
    /// It draws the shape mirrored left to right.
    pub mirrored: bool,
}

#[derive(Debug, Clone)]
pub struct CompiledMovement {
    pub heads: Vec<MoveHead>,
    pub effect: MovementEffect,
}

#[derive(Debug, Clone, Copy)]
pub struct MatrixCell {
    pub x: u16,
    pub y: u16,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
    pub white: u32,
    /// Dimmer or virtual dimmer of the head.
    pub intensity: u32,
}

#[derive(Debug, Clone)]
pub struct CompiledMatrix {
    pub cells: Vec<MatrixCell>,
    pub width: u16,
    pub height: u16,
    pub effect: MatrixEffect,
}

/// What a collection plays, all together.
#[derive(Debug, Clone, Default)]
pub struct CompiledCollection {
    /// The functions played, each once.
    pub members: Vec<FunctionId>,
    pub scenes: Vec<Arc<CompiledScene>>,
    pub movements: Vec<Arc<CompiledMovement>>,
    pub matrices: Vec<Arc<CompiledMatrix>>,
}

#[derive(Debug, Clone)]
pub enum CompiledFunction {
    Scene(Arc<CompiledScene>),
    Chaser(Arc<CompiledChaser>),
    CueList(Arc<CompiledCueList>),
    Movement(Arc<CompiledMovement>),
    Matrix(Arc<CompiledMatrix>),
    Collection(Arc<CompiledCollection>),
}

#[derive(Debug, Clone, Default)]
pub struct CompiledShow {
    pub universes: Vec<UniverseId>,
    pub channels: Vec<LogicalChannel>,
    pub fixtures: Vec<CompiledFixture>,
    pub fixture_index: BTreeMap<FixtureId, u32>,
    pub functions: BTreeMap<FunctionId, CompiledFunction>,
    pub warnings: Vec<String>,
    /// Per universe: addresses carrying intensity (dimmers, and color
    /// channels governed by a virtual dimmer). Used for raw desk channels.
    pub intensity_addr: Vec<[bool; UNIVERSE_SIZE]>,
}

const MAX_WARNINGS: usize = 200;

impl CompiledShow {
    pub fn warn(&mut self, w: String) {
        if self.warnings.len() < MAX_WARNINGS {
            self.warnings.push(w);
        }
    }

    pub fn channel_count(&self) -> usize {
        self.channels.len()
    }

    pub fn universe_index(&self, id: UniverseId) -> Option<usize> {
        self.universes.iter().position(|u| *u == id)
    }

    pub fn function(&self, id: FunctionId) -> Option<&CompiledFunction> {
        self.functions.get(&id)
    }

    /// Builds everything that does not depend on functions.
    pub fn build(show: &Show) -> Self {
        let mut c = CompiledShow {
            universes: show.universes.iter().map(|u| u.id).collect(),
            ..Default::default()
        };
        let mut profiles: BTreeMap<(usize, String), Arc<FixtureProfile>> = BTreeMap::new();

        for f in &show.fixtures {
            if let Err(e) = validate::check_fixture(show, f) {
                c.warn(format!("fixture {} '{}' skipped: {e}", f.id.0, f.name));
                continue;
            }
            let Some(def_idx) = show
                .definitions
                .iter()
                .position(|d| d.key() == f.definition)
            else {
                continue;
            };
            let Some(def) = show.definitions.get(def_idx) else {
                continue;
            };
            let Some(mode) = def.mode(&f.mode) else {
                continue;
            };
            let Some(universe) = c.universe_index(f.universe) else {
                continue;
            };
            let profile = profiles
                .entry((def_idx, f.mode.clone()))
                .or_insert_with(|| Arc::new(FixtureProfile::build(def, mode)))
                .clone();

            let fixture_idx = u32::try_from(c.fixtures.len()).unwrap_or(u32::MAX);
            let first_channel = u32::try_from(c.channels.len()).unwrap_or(u32::MAX);
            let base = f.address.saturating_sub(1);
            let universe = u16::try_from(universe).unwrap_or(u16::MAX);

            for slot in &profile.slots {
                let role = if slot.attribute == Attribute::Dimmer {
                    Role::Dimmer
                } else if slot.attribute.is_additive_color() {
                    Role::Color { vdim: NO_CHANNEL }
                } else {
                    Role::Other
                };
                c.channels.push(LogicalChannel {
                    universe,
                    coarse: base.saturating_add(slot.offset),
                    fine: slot.fine.map_or(NO_ADDR, |o| base.saturating_add(o)),
                    merge: if role == Role::Dimmer {
                        Merge::Htp
                    } else {
                        Merge::Ltp
                    },
                    role,
                    default: slot.default,
                    fadeable: slot.attribute.is_fadeable(),
                    fixture: fixture_idx,
                    attribute: slot.attribute,
                });
            }

            let mut heads = Vec::with_capacity(profile.heads.len());
            for (h, head) in profile.heads.iter().enumerate() {
                let h16 = u16::try_from(h).unwrap_or(u16::MAX);
                let has_color = head.slots.iter().any(|&s| {
                    profile
                        .slots
                        .get(usize::from(s))
                        .is_some_and(|s| s.attribute.is_additive_color())
                });
                let mut vdim = NO_CHANNEL;
                if !profile.has(h16, Attribute::Dimmer) && has_color {
                    vdim = u32::try_from(c.channels.len()).unwrap_or(NO_CHANNEL);
                    c.channels.push(LogicalChannel {
                        universe,
                        coarse: NO_ADDR,
                        fine: NO_ADDR,
                        merge: Merge::Htp,
                        role: Role::VirtualDimmer,
                        default: u16::MAX,
                        fadeable: true,
                        fixture: fixture_idx,
                        attribute: Attribute::Dimmer,
                    });
                    for &s in &head.slots {
                        let ch = first_channel.saturating_add(u32::from(s));
                        if let Some(lc) = c.channels.get_mut(ch as usize) {
                            if let Role::Color { vdim: v } = &mut lc.role {
                                if *v == NO_CHANNEL {
                                    *v = vdim;
                                }
                            }
                        }
                    }
                }
                heads.push(CompiledHead { vdim });
            }

            c.fixture_index.insert(f.id, fixture_idx);
            c.fixtures.push(CompiledFixture {
                id: f.id,
                first_channel,
                heads,
                profile,
                definition: def_idx,
                orientation: f.orientation,
                calibration: f.calibration,
            });
        }

        c.intensity_addr = vec![[false; UNIVERSE_SIZE]; c.universes.len()];
        for lc in &c.channels {
            let intensity = lc.role == Role::Dimmer
                || matches!(lc.role, Role::Color { vdim } if vdim != NO_CHANNEL);
            if !intensity {
                continue;
            }
            if let Some(map) = c.intensity_addr.get_mut(usize::from(lc.universe)) {
                for a in [lc.coarse, lc.fine] {
                    if let Some(slot) = map.get_mut(usize::from(a)) {
                        *slot = true;
                    }
                }
            }
        }

        for conflict in validate::address_conflicts(show) {
            c.warn(format!(
                "address conflict in universe {} at {}: fixtures {} and {}",
                conflict.universe.0, conflict.address, conflict.first.0, conflict.second.0
            ));
        }

        // Containers after what they contain, which they reuse: collections
        // after scenes and effects, chasers after collections.
        let mut functions: Vec<_> = show.functions.iter().collect();
        functions.sort_by_key(|f| match f.kind.category() {
            FunctionCategory::Chaser => 2,
            FunctionCategory::Collection => 1,
            _ => 0,
        });
        for func in functions {
            match compile_function(show, &mut c, &func.kind) {
                Some(cf) => {
                    c.functions.insert(func.id, cf);
                }
                None => c.warn(format!(
                    "function {} '{}' could not be compiled",
                    func.id.0, func.name
                )),
            }
        }
        c
    }

    /// (fixture index, head, order, group length) for each head of a target.
    pub fn expand_target(&self, show: &Show, target: &Target) -> Vec<(u32, u16, u16, u16)> {
        match *target {
            Target::Fixture { id } => match self.fixture_index.get(&id) {
                Some(&fi) => {
                    let heads = self.fixtures.get(fi as usize).map_or(0, |f| f.heads.len());
                    (0..heads)
                        .filter_map(|h| u16::try_from(h).ok())
                        .map(|h| (fi, h, 0, 1))
                        .collect()
                }
                None => Vec::new(),
            },
            Target::Head { id, head } => self
                .fixture_index
                .get(&id)
                .map(|&fi| vec![(fi, head, 0, 1)])
                .unwrap_or_default(),
            Target::Group { id } => {
                let Some(g) = show.group(id) else {
                    return Vec::new();
                };
                let len = u16::try_from(g.members.len()).unwrap_or(u16::MAX);
                g.members
                    .iter()
                    .enumerate()
                    .filter_map(|(i, m)| {
                        let fi = *self.fixture_index.get(&m.fixture)?;
                        Some((fi, m.head, u16::try_from(i).ok()?, len))
                    })
                    .collect()
            }
            // Resolved by `resolve_values`, which knows the scene.
            Target::Scene => Vec::new(),
        }
    }

    /// The heads of all the lights of a scene, each once, in member order:
    /// what [`Target::Scene`] means.
    fn expand_members(&self, show: &Show, members: &[Target]) -> Vec<(u32, u16, u16, u16)> {
        let mut seen = BTreeSet::new();
        let mut heads = Vec::new();
        for m in members {
            for (fi, head, _, _) in self.expand_target(show, m) {
                if seen.insert((fi, head)) {
                    heads.push((fi, head));
                }
            }
        }
        let len = u16::try_from(heads.len()).unwrap_or(u16::MAX);
        heads
            .into_iter()
            .zip(0u16..)
            .map(|((fi, head), order)| (fi, head, order, len))
            .collect()
    }

    /// Resolves one intention for one head into (channel, value) pairs.
    pub fn resolve_head(
        &self,
        show: &Show,
        fixture: u32,
        head: u16,
        value: &AttributeValue,
        scratch: &mut Vec<Resolved>,
        out: &mut Vec<(u32, u16)>,
    ) -> Result<(), String> {
        let f = self
            .fixtures
            .get(fixture as usize)
            .ok_or_else(|| "unknown fixture".to_owned())?;
        let def = show
            .definitions
            .get(f.definition)
            .ok_or_else(|| "missing definition".to_owned())?;
        let conv = Converter {
            def,
            profile: &f.profile,
            orientation: f.orientation,
            calibration: f.calibration.as_ref(),
        };
        scratch.clear();
        conv.resolve(head, value, scratch)
            .map_err(|e| format!("fixture {}: {e}", f.id.0))?;
        for r in scratch.iter() {
            match *r {
                Resolved::Slot { slot, value } => {
                    out.push((f.first_channel.saturating_add(u32::from(slot)), value));
                }
                Resolved::VirtualDimmer { value } => {
                    if let Some(h) = f.heads.get(usize::from(head)) {
                        if h.vdim != NO_CHANNEL {
                            out.push((h.vdim, value));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Logical channel of a fixture (by index into `fixtures`) that `channel`
    /// names, and whether it names its fine byte.
    pub fn locate_channel(&self, fixture: u32, channel: ChannelRef) -> Option<(u32, bool)> {
        let f = self.fixtures.get(fixture as usize)?;
        match channel {
            ChannelRef::Dmx {
                attribute,
                index,
                fine,
            } => {
                let (i, slot) = f
                    .profile
                    .slots
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.attribute == attribute)
                    .nth(usize::from(index))?;
                if fine && slot.fine.is_none() {
                    return None;
                }
                let channel = f.first_channel.saturating_add(u32::try_from(i).ok()?);
                Some((channel, fine))
            }
            ChannelRef::VirtualDimmer { head } => {
                let vdim = f.heads.get(usize::from(head))?.vdim;
                (vdim != NO_CHANNEL).then_some((vdim, false))
            }
        }
    }

    /// Applies one channel set by hand on top of the entries resolved so far.
    fn resolve_channel(
        &self,
        target: &Target,
        channel: ChannelRef,
        value: u8,
        entries: &mut Vec<SceneEntry>,
        index: &mut BTreeMap<u32, usize>,
    ) -> Result<(), String> {
        let Target::Fixture { id } = *target else {
            return Err("channel values need a single fixture as target".into());
        };
        let fixture = *self
            .fixture_index
            .get(&id)
            .ok_or_else(|| format!("target {target:?} resolves to no fixture"))?;
        let (logical, fine) = self
            .locate_channel(fixture, channel)
            .ok_or_else(|| format!("fixture {}: no channel {channel:?}", id.0))?;
        let lc = self
            .channels
            .get(logical as usize)
            .ok_or_else(|| "unknown channel".to_owned())?;
        if lc.attribute == Attribute::Maintenance {
            return Err("service channels cannot be stored in scenes".into());
        }
        let byte = if fine { FINE_BYTE } else { COARSE_BYTE };
        let existing = index.get(&logical).and_then(|&i| entries.get_mut(i));
        let current = existing.as_ref().map_or(lc.default, |e| e.value);
        let value = if lc.fine == NO_ADDR {
            to16(value)
        } else {
            let [hi, lo] = current.to_be_bytes();
            u16::from_be_bytes(if fine { [hi, value] } else { [value, lo] })
        };
        match existing {
            // Order and group are kept: the spread timing does not change.
            Some(e) => {
                e.value = value;
                e.set |= byte;
                e.manual |= byte;
            }
            None => {
                index.insert(logical, entries.len());
                entries.push(SceneEntry {
                    channel: logical,
                    value,
                    kind: kind_of_attribute(lc.attribute),
                    order: 0,
                    group_len: 1,
                    delay: TimeValue::ZERO,
                    set: byte,
                    manual: byte,
                });
            }
        }
        Ok(())
    }

    /// Resolves scene-like values (explicit, palette references or channels
    /// set by hand), in order: the last one wins on every channel. `members`
    /// are the lights of the scene (none for cues). Returns the entries and the
    /// warnings.
    pub fn resolve_values(
        &self,
        show: &Show,
        values: &[SceneValue],
        members: &[Target],
    ) -> (Vec<SceneEntry>, Vec<String>) {
        let mut entries: Vec<SceneEntry> = Vec::new();
        let mut index: BTreeMap<u32, usize> = BTreeMap::new();
        let mut scratch = Vec::new();
        let mut out = Vec::new();
        let mut warnings = Vec::new();

        for sv in values {
            if let ValueSource::Channel { channel, value } = sv.value {
                if let Err(w) =
                    self.resolve_channel(&sv.target, channel, value, &mut entries, &mut index)
                {
                    warnings.push(w);
                }
                continue;
            }
            let whole = sv.target == Target::Scene;
            let heads = if whole {
                self.expand_members(show, members)
            } else {
                self.expand_target(show, &sv.target)
            };
            // A value of the whole scene may be waiting for its lights.
            if heads.is_empty() && !whole {
                warnings.push(format!("target {:?} resolves to no fixture", sv.target));
            }
            for (fi, head, order, len) in heads {
                let fixture_id = self.fixtures.get(fi as usize).map(|f| f.id);
                let resolved_values: Vec<&AttributeValue> = match &sv.value {
                    ValueSource::Explicit { value } => vec![value],
                    ValueSource::Palette { id } => match show.palette(*id) {
                        Some(p) => fixture_id
                            .map(|fid| {
                                p.values_for(fid)
                                    .iter()
                                    .filter(|v| p.kind.accepts(v.kind()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        None => {
                            warnings.push(format!("unknown palette {}", id.0));
                            Vec::new()
                        }
                    },
                    // Handled above.
                    ValueSource::Channel { .. } => Vec::new(),
                };
                for value in resolved_values {
                    if value.targets_service_channel() {
                        warnings.push("service channels cannot be stored in scenes".into());
                        continue;
                    }
                    out.clear();
                    let member_value = value.for_member(order, len);
                    if let Err(w) =
                        self.resolve_head(show, fi, head, &member_value, &mut scratch, &mut out)
                    {
                        warnings.push(w);
                        continue;
                    }
                    for &(channel, v) in &out {
                        let e = SceneEntry {
                            channel,
                            value: v,
                            kind: value.kind(),
                            order,
                            group_len: len,
                            delay: TimeValue::ZERO,
                            set: COARSE_BYTE | FINE_BYTE,
                            manual: 0,
                        };
                        match index.get(&channel).and_then(|&i| entries.get_mut(i)) {
                            Some(existing) => *existing = e,
                            None => {
                                index.insert(channel, entries.len());
                                entries.push(e);
                            }
                        }
                    }
                }
            }
        }
        warnings.sort();
        warnings.dedup();
        (entries, warnings)
    }

    pub fn compile_scene(&mut self, show: &Show, scene: &Scene) -> CompiledScene {
        let (entries, warnings) = self.resolve_values(show, &scene.values, &scene.members);
        for w in warnings {
            self.warn(w);
        }
        CompiledScene {
            entries,
            timing: [
                scene.timing_for(AttributeKind::Intensity),
                scene.timing_for(AttributeKind::Color),
                scene.timing_for(AttributeKind::Position),
                scene.timing_for(AttributeKind::Beam),
            ],
            spread: scene.spread,
        }
    }
}

fn compile_function(
    show: &Show,
    c: &mut CompiledShow,
    kind: &FunctionKind,
) -> Option<CompiledFunction> {
    Some(match kind {
        FunctionKind::Scene(s) => CompiledFunction::Scene(Arc::new(c.compile_scene(show, s))),
        FunctionKind::Chaser(ch) => CompiledFunction::Chaser(Arc::new(compile_chaser(c, ch))),
        FunctionKind::CueList(cl) => {
            CompiledFunction::CueList(Arc::new(compile_cue_list(show, c, cl)))
        }
        FunctionKind::MovementEffect(m) => {
            CompiledFunction::Movement(Arc::new(compile_movement(show, c, m)))
        }
        FunctionKind::MatrixEffect(m) => {
            CompiledFunction::Matrix(Arc::new(compile_matrix(show, c, m)?))
        }
        FunctionKind::Collection(col) => {
            CompiledFunction::Collection(Arc::new(compile_collection(c, col)))
        }
    })
}

fn compile_collection(c: &mut CompiledShow, col: &Collection) -> CompiledCollection {
    let mut out = CompiledCollection::default();
    for &id in &col.functions {
        // Listed twice, it plays once.
        if out.members.contains(&id) {
            continue;
        }
        match c.functions.get(&id) {
            Some(CompiledFunction::Scene(s)) => out.scenes.push(s.clone()),
            Some(CompiledFunction::Movement(m)) => out.movements.push(m.clone()),
            Some(CompiledFunction::Matrix(m)) => out.matrices.push(m.clone()),
            Some(
                CompiledFunction::Chaser(_)
                | CompiledFunction::CueList(_)
                | CompiledFunction::Collection(_),
            )
            | None => {
                c.warn(format!(
                    "collection references {} which a collection cannot play",
                    id.0
                ));
                continue;
            }
        }
        out.members.push(id);
    }
    out
}

/// A scene as a chaser step plays it: the step's crossfade replaces the
/// scene's own times.
fn step_scene(scene: &CompiledScene, fade: TimeValue) -> Arc<CompiledScene> {
    Arc::new(CompiledScene {
        entries: scene.entries.clone(),
        timing: [Timing {
            delay: TimeValue::ZERO,
            fade_in: fade,
            fade_out: fade,
        }; 4],
        spread: Spread::None,
    })
}

fn compile_chaser(c: &mut CompiledShow, ch: &Chaser) -> CompiledChaser {
    let mut steps = Vec::new();
    for (index, step) in ch.steps.iter().enumerate() {
        let fade = step.fade.unwrap_or(ch.fade);
        let content = match c.functions.get(&step.scene) {
            Some(CompiledFunction::Scene(s)) => Some(StepContent::Scene(step_scene(s, fade))),
            Some(CompiledFunction::Movement(m)) => Some(StepContent::Movement(m.clone())),
            Some(CompiledFunction::Matrix(m)) => Some(StepContent::Matrix(m.clone())),
            Some(CompiledFunction::Collection(col)) => {
                let mut col = (**col).clone();
                for s in &mut col.scenes {
                    *s = step_scene(s, fade);
                }
                Some(StepContent::Collection(Arc::new(col)))
            }
            Some(CompiledFunction::Chaser(_) | CompiledFunction::CueList(_)) | None => None,
        };
        match content {
            Some(content) => steps.push(CompiledStep {
                index,
                source: step.scene,
                content,
                hold: step.hold.unwrap_or(ch.hold),
                fade,
            }),
            None => c.warn(format!(
                "chaser step references {} which a chaser cannot play",
                step.scene.0
            )),
        }
    }
    CompiledChaser {
        steps,
        direction: ch.direction,
        run_mode: ch.run_mode,
    }
}

fn compile_cue_list(show: &Show, c: &mut CompiledShow, cl: &CueList) -> CompiledCueList {
    let mut cues: Vec<CompiledCue> = Vec::new();
    let mut scenes: Vec<CompiledScene> = cl
        .cues
        .iter()
        .map(|cue| {
            let scene = Scene {
                values: cue.values.clone(),
                timing: cue.timing,
                ..Scene::default()
            };
            c.compile_scene(show, &scene)
        })
        .collect();

    // Move in black: a head dark in cue i is preset to the non-intensity
    // values of the next cue where it lights up, after cue i's fade.
    let lit = |scene: &CompiledScene, fixture: u32, chans: &[LogicalChannel]| -> bool {
        scene.entries.iter().any(|e| {
            chans
                .get(e.channel as usize)
                .is_some_and(|lc| lc.fixture == fixture && lc.role.is_intensity() && e.value > 0)
        })
    };
    let snapshot: Vec<CompiledScene> = scenes.clone();
    for (i, cue) in cl.cues.iter().enumerate() {
        let enabled = cue.move_in_black.unwrap_or(cl.move_in_black);
        if !enabled {
            continue;
        }
        let Some(current) = snapshot.get(i) else {
            continue;
        };
        // The preset starts once the head is dark: after the cue's intensity fades.
        let t = current.timing_for(AttributeKind::Intensity);
        let wait = longest(longest(t.fade_in, t.fade_out), t.delay);
        let mut extra: Vec<SceneEntry> = Vec::new();
        for fixture in 0..u32::try_from(c.fixtures.len()).unwrap_or(0) {
            if lit(current, fixture, &c.channels) {
                continue;
            }
            let next = snapshot
                .iter()
                .skip(i + 1)
                .find(|s| lit(s, fixture, &c.channels));
            if let Some(next) = next {
                for e in &next.entries {
                    let Some(lc) = c.channels.get(e.channel as usize) else {
                        continue;
                    };
                    if lc.fixture == fixture && !lc.role.is_intensity() {
                        extra.push(SceneEntry { delay: wait, ..*e });
                    }
                }
            }
        }
        if let Some(scene) = scenes.get_mut(i) {
            for e in extra {
                scene.entries.retain(|x| x.channel != e.channel);
                scene.entries.push(e);
            }
        }
    }

    for (cue, scene) in cl.cues.iter().zip(scenes) {
        cues.push(CompiledCue {
            scene: Arc::new(scene),
            follow: cue.follow,
            number: cue.number,
        });
    }
    CompiledCueList { cues }
}

/// The longer of two durations (beats compared at 120 BPM as a heuristic
/// only when units differ).
fn longest(a: TimeValue, b: TimeValue) -> TimeValue {
    match (a, b) {
        (TimeValue::Ms(x), TimeValue::Ms(y)) => TimeValue::Ms(x.max(y)),
        (TimeValue::Beats(x), TimeValue::Beats(y)) => TimeValue::Beats(x.max(y)),
        _ => {
            if a.to_micros(120.0) >= b.to_micros(120.0) {
                a
            } else {
                b
            }
        }
    }
}

fn compile_movement(show: &Show, c: &mut CompiledShow, m: &MovementEffect) -> CompiledMovement {
    let heads: Vec<MoveHead> = c
        .movement_members(show, m)
        .into_iter()
        .filter_map(|(_, _, head)| head)
        .collect();
    if heads.is_empty() {
        c.warn(format!(
            "movement effect on group {} has no pan/tilt heads",
            m.group.0
        ));
    }
    CompiledMovement {
        heads,
        effect: m.clone(),
    }
}

impl CompiledShow {
    /// Every member of the group of a movement effect, in group order: its
    /// fixture index, head and, if it has pan or tilt, how the effect moves
    /// it. The engine plays the heads; the editor's reach reads the same list.
    pub fn movement_members(
        &self,
        show: &Show,
        m: &MovementEffect,
    ) -> Vec<(u32, u16, Option<MoveHead>)> {
        let members = self.expand_target(show, &Target::Group { id: m.group });
        let n = members.len().max(1);
        members
            .into_iter()
            .map(|(fi, head, order, _)| {
                let moving = self.fixtures.get(fi as usize).and_then(|f| {
                    let find = |a| {
                        f.profile.find(head, a).map_or(NO_CHANNEL, |(s, _)| {
                            f.first_channel.saturating_add(u32::from(s))
                        })
                    };
                    let (pan, tilt) = (find(Attribute::Pan), find(Attribute::Tilt));
                    (pan != NO_CHANNEL || tilt != NO_CHANNEL).then(|| MoveHead {
                        pan,
                        tilt,
                        pan_range: f.profile.physical.pan_max_deg,
                        tilt_range: f.profile.physical.tilt_max_deg,
                        orientation: f.orientation,
                        phase: crate::effects::head_phase(m.phase_spread, usize::from(order), n),
                        mirrored: m.mirror.mirrors(usize::from(order), n),
                    })
                });
                (fi, head, moving)
            })
            .collect()
    }
}

fn compile_matrix(
    show: &Show,
    c: &mut CompiledShow,
    m: &klight_model::MatrixEffect,
) -> Option<CompiledMatrix> {
    let group = show.group(m.group)?;
    let mut cells = Vec::new();
    // Same layout function the UI preview uses (klight-wasm).
    let layout = group.layout();
    let (width, height) = (layout.width, layout.height);
    for (member, pos) in group.members.iter().zip(layout.positions.iter().copied()) {
        let Some(&fi) = c.fixture_index.get(&member.fixture) else {
            continue;
        };
        let Some(f) = c.fixtures.get(fi as usize) else {
            continue;
        };
        let find = |a| {
            f.profile.find(member.head, a).map_or(NO_CHANNEL, |(s, _)| {
                f.first_channel.saturating_add(u32::from(s))
            })
        };
        let dimmer = find(Attribute::Dimmer);
        let intensity = if dimmer != NO_CHANNEL {
            dimmer
        } else {
            f.heads
                .get(usize::from(member.head))
                .map_or(NO_CHANNEL, |h| h.vdim)
        };
        cells.push(MatrixCell {
            x: pos.x,
            y: pos.y,
            red: find(Attribute::Red),
            green: find(Attribute::Green),
            blue: find(Attribute::Blue),
            white: find(Attribute::White),
            intensity,
        });
    }
    Some(CompiledMatrix {
        cells,
        width: width.max(1),
        height,
        effect: m.clone(),
    })
}
