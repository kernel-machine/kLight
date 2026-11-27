//! Layers and playback of functions.

use std::sync::Arc;

use klight_model::function::{Direction, RunMode};
use klight_model::{CueFollow, FunctionId, TimeValue};

use crate::compile::{
    CompiledChaser, CompiledCollection, CompiledCueList, CompiledMatrix, CompiledMovement,
    CompiledScene, LogicalChannel, Merge, StepContent, NO_CHANNEL,
};
use crate::music::{beats_to_fixed, MusicClock};

/// Values set by one layer, indexed by logical channel.
#[derive(Debug, Clone, Default)]
pub struct Layer {
    pub values: Vec<u16>,
    pub set: Vec<bool>,
}

impl Layer {
    pub fn resize(&mut self, n: usize) {
        self.values.resize(n, 0);
        self.set.resize(n, false);
    }

    pub fn clear(&mut self) {
        self.set.iter_mut().for_each(|s| *s = false);
    }

    pub fn get(&self, ch: usize) -> Option<u16> {
        match self.set.get(ch) {
            Some(true) => self.values.get(ch).copied(),
            _ => None,
        }
    }

    /// Writes with the merge rule of the channel inside the layer.
    pub fn write(&mut self, ch: usize, merge: Merge, v: u16) {
        let (Some(slot), Some(set)) = (self.values.get_mut(ch), self.set.get_mut(ch)) else {
            return;
        };
        if *set && merge == Merge::Htp {
            *slot = (*slot).max(v);
        } else {
            *slot = v;
        }
        *set = true;
    }

    /// Writes another layer on top, each channel with its merge rule.
    pub fn write_layer(&mut self, other: &Layer, channels: &[LogicalChannel]) {
        for (ch, lc) in channels.iter().enumerate() {
            if let Some(v) = other.get(ch) {
                self.write(ch, lc.merge, v);
            }
        }
    }
}

/// Linear interpolation in integer arithmetic: exact at both ends and
/// monotonic in `num`.
pub fn lerp_u16(from: u16, to: u16, num: u64, den: u64) -> u16 {
    if den == 0 || num >= den {
        return to;
    }
    let from_i = i64::from(from);
    let delta = i64::from(to) - from_i;
    let num = i64::try_from(num).unwrap_or(i64::MAX);
    let den = i64::try_from(den).unwrap_or(i64::MAX);
    let v = from_i + (i128::from(delta) * i128::from(num) / i128::from(den)) as i64;
    u16::try_from(v.clamp(0, i64::from(u16::MAX))).unwrap_or(to)
}

/// Scales `v` by `level / 65535`, rounding to nearest.
pub fn scale(v: u16, level: u16) -> u16 {
    let r = (u32::from(v) * u32::from(level) + 32767) / 65535;
    u16::try_from(r).unwrap_or(u16::MAX)
}

/// One running copy of a compiled scene, with per-entry timing.
#[derive(Debug, Clone, Default)]
pub struct SceneInstance {
    pub scene: Option<Arc<CompiledScene>>,
    pub started_us: u64,
    pub released_at: Option<u64>,
    /// Scenes of one collection started together share this: among them every
    /// channel is HTP.
    together: Option<u64>,
    start: Vec<u16>,
    released: Vec<u16>,
    delay_us: Vec<u64>,
    fade_in_us: Vec<u64>,
    fade_out_us: Vec<u64>,
}

impl SceneInstance {
    /// (Re)initialises the instance, reusing its buffers.
    pub fn begin(
        &mut self,
        scene: Arc<CompiledScene>,
        now_us: u64,
        bpm: f64,
        current: &Layer,
        channels: &[LogicalChannel],
    ) {
        self.start.clear();
        self.released.clear();
        self.delay_us.clear();
        self.fade_in_us.clear();
        self.fade_out_us.clear();
        for e in &scene.entries {
            let ch = e.channel as usize;
            let default = channels.get(ch).map_or(0, |c| c.default);
            self.start.push(current.get(ch).unwrap_or(default));
            let t = scene.timing_for(e.kind);
            let spread_step = match scene.spread {
                klight_model::Spread::None => 0,
                klight_model::Spread::Linear { step } => step.to_micros(bpm) * u64::from(e.order),
                klight_model::Spread::FromCenter { step } => {
                    // |2*order - (n-1)| / 2 steps from the center.
                    let n = i64::from(e.group_len.max(1));
                    let d = (2 * i64::from(e.order) - (n - 1)).unsigned_abs();
                    step.to_micros(bpm) * d / 2
                }
            };
            self.delay_us.push(
                t.delay
                    .to_micros(bpm)
                    .saturating_add(spread_step)
                    .saturating_add(e.delay.to_micros(bpm)),
            );
            self.fade_in_us.push(t.fade_in.to_micros(bpm));
            self.fade_out_us.push(t.fade_out.to_micros(bpm));
            self.released.push(0);
        }
        self.scene = Some(scene);
        self.started_us = now_us;
        self.released_at = None;
    }

    /// Rebinds to a recompiled scene and skips its fades (show edited live).
    pub fn rebind(
        &mut self,
        scene: Arc<CompiledScene>,
        now_us: u64,
        bpm: f64,
        current: &Layer,
        channels: &[LogicalChannel],
    ) {
        self.begin(scene, now_us, bpm, current, channels);
        self.delay_us.iter_mut().for_each(|d| *d = 0);
        self.fade_in_us.iter_mut().for_each(|d| *d = 0);
    }

    fn value_at(&self, i: usize, now_us: u64, merge: Merge, fadeable: bool) -> Option<u16> {
        let scene = self.scene.as_ref()?;
        let target = scene.entries.get(i)?.value;
        let start = *self.start.get(i)?;
        if let Some(rel) = self.released_at {
            let held = *self.released.get(i)?;
            let fade = *self.fade_out_us.get(i)?;
            let t = now_us.saturating_sub(rel);
            return match merge {
                Merge::Htp => Some(lerp_u16(held, 0, t, fade)),
                Merge::Ltp => (t < fade).then_some(held),
            };
        }
        let delay = *self.delay_us.get(i)?;
        let fade = *self.fade_in_us.get(i)?;
        let since = now_us.saturating_sub(self.started_us);
        if since < delay {
            return match merge {
                Merge::Htp => Some(0),
                Merge::Ltp => Some(start),
            };
        }
        let t = since - delay;
        Some(match merge {
            Merge::Htp => lerp_u16(0, target, t, fade),
            Merge::Ltp if fadeable => lerp_u16(start, target, t, fade),
            Merge::Ltp => target,
        })
    }

    pub fn render(&self, now_us: u64, level: u16, channels: &[LogicalChannel], layer: &mut Layer) {
        self.render_merging(now_us, level, channels, layer, None);
    }

    /// [`Self::render`], merging by `forced` instead of each channel's rule.
    fn render_merging(
        &self,
        now_us: u64,
        level: u16,
        channels: &[LogicalChannel],
        layer: &mut Layer,
        forced: Option<Merge>,
    ) {
        let Some(scene) = self.scene.as_ref() else {
            return;
        };
        for (i, e) in scene.entries.iter().enumerate() {
            let ch = e.channel as usize;
            let Some(lc) = channels.get(ch) else { continue };
            if let Some(v) = self.value_at(i, now_us, lc.merge, lc.fadeable) {
                let v = if lc.merge == Merge::Htp {
                    scale(v, level)
                } else {
                    v
                };
                layer.write(ch, forced.unwrap_or(lc.merge), v);
            }
        }
    }

    /// Starts the fade out from the current values.
    pub fn release(&mut self, now_us: u64, fade_out_us: Option<u64>, channels: &[LogicalChannel]) {
        if self.released_at.is_some() {
            return;
        }
        let Some(scene) = self.scene.clone() else {
            return;
        };
        for (i, e) in scene.entries.iter().enumerate() {
            let (merge, fadeable) = channels
                .get(e.channel as usize)
                .map_or((Merge::Ltp, false), |c| (c.merge, c.fadeable));
            let v = self.value_at(i, now_us, merge, fadeable).unwrap_or(0);
            if let Some(r) = self.released.get_mut(i) {
                *r = v;
            }
            if let (Some(f), Some(fo)) = (self.fade_out_us.get_mut(i), fade_out_us) {
                *f = fo;
            }
        }
        self.released_at = Some(now_us);
    }

    pub fn finished(&self, now_us: u64) -> bool {
        match self.released_at {
            Some(rel) => {
                let longest = self.fade_out_us.iter().copied().max().unwrap_or(0);
                now_us.saturating_sub(rel) >= longest
            }
            None => false,
        }
    }

    /// True when every fade-in has completed.
    pub fn settled(&self, now_us: u64) -> bool {
        let since = now_us.saturating_sub(self.started_us);
        self.delay_us
            .iter()
            .zip(&self.fade_in_us)
            .all(|(d, f)| since >= d.saturating_add(*f))
    }
}

#[derive(Debug, Clone)]
pub struct ChaserState {
    pub chaser: Arc<CompiledChaser>,
    pub step: usize,
    pub step_start_us: u64,
    pub step_start_beat: u64,
    pub forward: bool,
}

#[derive(Debug, Clone)]
pub struct CueState {
    pub list: Arc<CompiledCueList>,
    pub current: Option<usize>,
    pub cue_started_us: u64,
}

#[derive(Debug, Clone)]
pub enum PlaybackKind {
    Scene(Arc<CompiledScene>),
    Chaser(ChaserState),
    CueList(CueState),
    Movement(Arc<CompiledMovement>),
    Matrix(Arc<CompiledMatrix>),
    Collection(Arc<CompiledCollection>),
}

#[derive(Debug, Clone)]
pub struct Playback {
    pub id: FunctionId,
    pub level: u16,
    /// Activation order: later playbacks win LTP.
    pub order: u64,
    pub kind: PlaybackKind,
    pub instances: Vec<SceneInstance>,
    pub stopping: bool,
    pub started_us: u64,
    pub started_beat: u64,
    pub paused_since: Option<u64>,
    /// Times scenes of a collection were started together.
    pub groups: u64,
}

/// Context handed to playback updates.
pub struct Ctx<'a> {
    pub now_us: u64,
    pub music: &'a MusicClock,
    pub base: &'a Layer,
    pub channels: &'a [LogicalChannel],
}

impl Playback {
    pub fn position(&self) -> Option<u32> {
        match &self.kind {
            // The step's place in the show, as the screens list it.
            PlaybackKind::Chaser(c) => c
                .chaser
                .steps
                .get(c.step)
                .and_then(|s| u32::try_from(s.index).ok()),
            PlaybackKind::CueList(c) => c.current.and_then(|i| u32::try_from(i).ok()),
            _ => None,
        }
    }

    /// Adds the functions played inside this one: the current step of a
    /// chaser, the members of a collection, itself or as a step.
    pub fn playing(&self, out: &mut Vec<FunctionId>) {
        match &self.kind {
            PlaybackKind::Chaser(c) => {
                if let Some(step) = c.chaser.steps.get(c.step) {
                    out.push(step.source);
                    if let StepContent::Collection(c) = &step.content {
                        out.extend_from_slice(&c.members);
                    }
                }
            }
            PlaybackKind::Collection(c) => out.extend_from_slice(&c.members),
            _ => {}
        }
    }

    /// Time used to render: frozen while paused.
    pub fn render_time(&self, now_us: u64) -> u64 {
        self.paused_since.unwrap_or(now_us)
    }

    fn spawn(&mut self, pool: &mut Vec<SceneInstance>, scene: Arc<CompiledScene>, ctx: &Ctx<'_>) {
        let mut inst = pool.pop().unwrap_or_default();
        inst.begin(scene, ctx.now_us, ctx.music.bpm(), ctx.base, ctx.channels);
        inst.together = None;
        self.instances.push(inst);
    }

    /// Starts the scenes of a collection, merged HTP among themselves.
    fn spawn_together(
        &mut self,
        pool: &mut Vec<SceneInstance>,
        scenes: &[Arc<CompiledScene>],
        ctx: &Ctx<'_>,
    ) {
        self.groups += 1;
        for scene in scenes {
            self.spawn(pool, scene.clone(), ctx);
            if let Some(inst) = self.instances.last_mut() {
                inst.together = Some(self.groups);
            }
        }
    }

    /// Starts what a chaser step plays. Effects need nothing: they are
    /// rendered from the current step.
    fn enter_step(&mut self, pool: &mut Vec<SceneInstance>, content: &StepContent, ctx: &Ctx<'_>) {
        match content {
            StepContent::Scene(s) => self.spawn(pool, s.clone(), ctx),
            StepContent::Collection(c) => self.spawn_together(pool, &c.scenes, ctx),
            StepContent::Movement(_) | StepContent::Matrix(_) => {}
        }
    }

    /// Replaces what is playing with `content`, without fading it in (show
    /// edited live, or restored).
    pub fn show_now(
        &mut self,
        pool: &mut Vec<SceneInstance>,
        content: &StepContent,
        ctx: &Ctx<'_>,
    ) {
        pool.append(&mut self.instances);
        self.enter_step(pool, content, ctx);
        self.snap(ctx);
    }

    /// Skips the fades of everything playing.
    pub fn snap(&mut self, ctx: &Ctx<'_>) {
        for inst in &mut self.instances {
            if let Some(scene) = inst.scene.clone() {
                inst.rebind(scene, ctx.now_us, ctx.music.bpm(), ctx.base, ctx.channels);
            }
        }
    }

    fn release_all(&mut self, ctx: &Ctx<'_>, fade_out: Option<u64>) {
        for i in &mut self.instances {
            i.release(ctx.now_us, fade_out, ctx.channels);
        }
    }

    /// Called right after activation.
    pub fn start(&mut self, pool: &mut Vec<SceneInstance>, ctx: &Ctx<'_>) {
        match &mut self.kind {
            PlaybackKind::Scene(s) => {
                let s = s.clone();
                self.spawn(pool, s, ctx);
            }
            PlaybackKind::Chaser(state) => {
                let first = match state.chaser.direction {
                    Direction::Forward => 0,
                    Direction::Backward => state.chaser.steps.len().saturating_sub(1),
                };
                state.step = first;
                state.forward = state.chaser.direction == Direction::Forward;
                state.step_start_us = ctx.now_us;
                state.step_start_beat = ctx.music.beat_at(ctx.now_us);
                if let Some(content) = state.chaser.steps.get(first).map(|s| s.content.clone()) {
                    self.enter_step(pool, &content, ctx);
                }
            }
            PlaybackKind::CueList(_) => {
                self.cue_go(pool, ctx);
            }
            PlaybackKind::Movement(_) | PlaybackKind::Matrix(_) => {}
            PlaybackKind::Collection(c) => {
                let c = c.clone();
                self.spawn_together(pool, &c.scenes, ctx);
            }
        }
    }

    /// Stops with fades; returns true if the playback can be removed immediately.
    pub fn stop(&mut self, ctx: &Ctx<'_>) -> bool {
        self.stopping = true;
        self.release_all(ctx, None);
        // Effects have nothing to fade out.
        matches!(
            self.kind,
            PlaybackKind::Movement(_) | PlaybackKind::Matrix(_)
        ) || (matches!(self.kind, PlaybackKind::Collection(_)) && self.instances.is_empty())
    }

    pub fn cue_go(&mut self, pool: &mut Vec<SceneInstance>, ctx: &Ctx<'_>) {
        let PlaybackKind::CueList(state) = &mut self.kind else {
            return;
        };
        let next = state.current.map_or(0, |c| c + 1);
        let Some(cue) = state.list.cues.get(next) else {
            return;
        };
        let scene = cue.scene.clone();
        state.current = Some(next);
        state.cue_started_us = ctx.now_us;
        self.go_to_scene(pool, scene, ctx);
    }

    pub fn cue_back(&mut self, pool: &mut Vec<SceneInstance>, ctx: &Ctx<'_>) {
        let PlaybackKind::CueList(state) = &mut self.kind else {
            return;
        };
        let Some(prev) = state.current.and_then(|c| c.checked_sub(1)) else {
            return;
        };
        let Some(cue) = state.list.cues.get(prev) else {
            return;
        };
        let scene = cue.scene.clone();
        state.current = Some(prev);
        state.cue_started_us = ctx.now_us;
        self.go_to_scene(pool, scene, ctx);
    }

    pub fn cue_goto(&mut self, pool: &mut Vec<SceneInstance>, ctx: &Ctx<'_>, index: usize) -> bool {
        let PlaybackKind::CueList(state) = &mut self.kind else {
            return false;
        };
        let Some(cue) = state.list.cues.get(index) else {
            return false;
        };
        let scene = cue.scene.clone();
        state.current = Some(index);
        state.cue_started_us = ctx.now_us;
        self.go_to_scene(pool, scene, ctx);
        true
    }

    /// Jumps a chaser to the step at `index` in the show's list (a step whose
    /// function cannot be played is not there). It fades in with its own fade
    /// and lasts its full hold from now. False if there is no such step.
    pub fn chaser_goto(
        &mut self,
        pool: &mut Vec<SceneInstance>,
        ctx: &Ctx<'_>,
        index: usize,
    ) -> bool {
        let PlaybackKind::Chaser(state) = &mut self.kind else {
            return false;
        };
        let Some(pos) = state.chaser.steps.iter().position(|s| s.index == index) else {
            return false;
        };
        let Some(step) = state.chaser.steps.get(pos) else {
            return false;
        };
        let content = step.content.clone();
        let fade = step.fade.to_micros(ctx.music.bpm());
        state.step = pos;
        state.step_start_us = ctx.now_us;
        state.step_start_beat = ctx.music.beat_at(ctx.now_us);
        self.paused_since = None;
        self.release_all(ctx, Some(fade));
        self.enter_step(pool, &content, ctx);
        true
    }

    fn go_to_scene(
        &mut self,
        pool: &mut Vec<SceneInstance>,
        scene: Arc<CompiledScene>,
        ctx: &Ctx<'_>,
    ) {
        self.paused_since = None;
        // The incoming cue's fade-out time is the down time of the outgoing one.
        let down = scene
            .timing_for(klight_model::AttributeKind::Intensity)
            .fade_out
            .to_micros(ctx.music.bpm());
        self.release_all(ctx, Some(down));
        self.spawn(pool, scene, ctx);
    }

    pub fn toggle_pause(&mut self, now_us: u64) {
        match self.paused_since.take() {
            Some(since) => {
                let d = now_us.saturating_sub(since);
                for i in &mut self.instances {
                    i.started_us = i.started_us.saturating_add(d);
                    if let Some(r) = &mut i.released_at {
                        *r = r.saturating_add(d);
                    }
                }
                if let PlaybackKind::CueList(s) = &mut self.kind {
                    s.cue_started_us = s.cue_started_us.saturating_add(d);
                }
            }
            None => self.paused_since = Some(now_us),
        }
    }

    /// Advances time-based behaviour. Returns true when the discrete state
    /// changed (step/cue change, end).
    pub fn advance(&mut self, pool: &mut Vec<SceneInstance>, ctx: &Ctx<'_>) -> bool {
        let mut changed = false;
        if self.paused_since.is_none() && !self.stopping {
            changed |= match &self.kind {
                PlaybackKind::Chaser(_) => self.advance_chaser(pool, ctx),
                PlaybackKind::CueList(state) => {
                    let auto = state
                        .current
                        .and_then(|c| state.list.cues.get(c))
                        .and_then(|cue| match cue.follow {
                            CueFollow::Auto { after } => Some(after.to_micros(ctx.music.bpm())),
                            CueFollow::Manual => None,
                        });
                    let has_next = state.current.map_or(0, |c| c + 1) < state.list.cues.len();
                    match auto {
                        Some(after)
                            if has_next
                                && ctx.now_us.saturating_sub(state.cue_started_us) >= after =>
                        {
                            self.cue_go(pool, ctx);
                            true
                        }
                        _ => false,
                    }
                }
                _ => false,
            };
        }
        // Retire finished instances (buffers go back to the pool).
        let now = self.render_time(ctx.now_us);
        let mut i = 0;
        while i < self.instances.len() {
            if self.instances.get(i).is_some_and(|inst| inst.finished(now)) {
                // `remove` keeps the order: newer instances stay last (LTP).
                let inst = self.instances.remove(i);
                pool.push(inst);
            } else {
                i += 1;
            }
        }
        changed
    }

    fn advance_chaser(&mut self, pool: &mut Vec<SceneInstance>, ctx: &Ctx<'_>) -> bool {
        let PlaybackKind::Chaser(state) = &mut self.kind else {
            return false;
        };
        let n = state.chaser.steps.len();
        let Some(step) = state.chaser.steps.get(state.step) else {
            return false;
        };
        let (due, beat_based) = match step.hold {
            TimeValue::Ms(ms) => (
                ctx.now_us >= state.step_start_us.saturating_add(u64::from(ms) * 1000),
                false,
            ),
            TimeValue::Beats(b) => {
                let len = beats_to_fixed(b);
                (
                    ctx.music.beat_at(ctx.now_us) >= state.step_start_beat.saturating_add(len),
                    true,
                )
            }
        };
        if !due {
            return false;
        }
        let next = match (state.chaser.run_mode, state.forward) {
            (RunMode::Loop, true) => Some((state.step + 1) % n.max(1)),
            (RunMode::Loop, false) => {
                Some(state.step.checked_sub(1).unwrap_or(n.saturating_sub(1)))
            }
            (RunMode::SingleShot, true) => (state.step + 1 < n).then_some(state.step + 1),
            (RunMode::SingleShot, false) => state.step.checked_sub(1),
            (RunMode::PingPong, fwd) => {
                if n <= 1 {
                    Some(0)
                } else if fwd && state.step + 1 >= n {
                    state.forward = false;
                    Some(state.step - 1)
                } else if !fwd && state.step == 0 {
                    state.forward = true;
                    Some(1)
                } else if fwd {
                    Some(state.step + 1)
                } else {
                    Some(state.step - 1)
                }
            }
        };
        // Next boundary is computed from the previous one: no drift.
        if beat_based {
            if let TimeValue::Beats(b) = step.hold {
                let len = beats_to_fixed(b).max(1);
                let late = ctx
                    .music
                    .beat_at(ctx.now_us)
                    .saturating_sub(state.step_start_beat);
                // Whole steps gone by: normally one. More when the step was
                // shortened while running (or is shorter than a tick): they
                // are skipped, staying on the grid.
                let gone = (late / len).max(1).saturating_mul(len);
                state.step_start_beat = state.step_start_beat.saturating_add(gone);
            }
            state.step_start_us = ctx.now_us;
        } else {
            let hold = step.hold.to_micros(ctx.music.bpm()).max(1);
            state.step_start_us = state.step_start_us.saturating_add(hold);
            if ctx.now_us.saturating_sub(state.step_start_us) > hold {
                state.step_start_us = ctx.now_us;
            }
            // The next step may count in beats: from the same boundary.
            state.step_start_beat = ctx.music.beat_at(state.step_start_us);
        }
        match next {
            Some(nx) => {
                state.step = nx;
                let Some(next_step) = state.chaser.steps.get(nx) else {
                    return true;
                };
                let content = next_step.content.clone();
                let fade = next_step.fade.to_micros(ctx.music.bpm());
                self.release_all(ctx, Some(fade));
                self.enter_step(pool, &content, ctx);
            }
            None => {
                self.stopping = true;
                self.release_all(ctx, None);
            }
        }
        true
    }

    /// Renders scene instances into the base layer. `mix` is a work layer to
    /// merge the scenes of a collection.
    pub fn render_base(
        &self,
        now_us: u64,
        channels: &[LogicalChannel],
        layer: &mut Layer,
        mix: &mut Layer,
    ) {
        let t = self.render_time(now_us);
        let mut insts = self.instances.iter().peekable();
        while let Some(inst) = insts.next() {
            let Some(group) = inst.together else {
                inst.render(t, self.level, channels, layer);
                continue;
            };
            // The highest value of the group, then merged like any scene.
            mix.clear();
            let mut next = Some(inst);
            while let Some(i) = next {
                i.render_merging(t, self.level, channels, mix, Some(Merge::Htp));
                next = insts.next_if(|i| i.together == Some(group));
            }
            layer.write_layer(mix, channels);
        }
    }

    pub fn is_done(&self) -> bool {
        self.stopping && self.instances.is_empty()
    }
}

/// Unused-channel guard used by effects.
pub fn valid(ch: u32) -> bool {
    ch != NO_CHANNEL
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_ends_exact() {
        assert_eq!(lerp_u16(10, 200, 0, 100), 10);
        assert_eq!(lerp_u16(10, 200, 100, 100), 200);
        assert_eq!(lerp_u16(200, 10, 50, 100), 105);
        assert_eq!(lerp_u16(0, 65535, 5, 0), 65535);
    }

    #[test]
    fn scale_bounds() {
        assert_eq!(scale(65535, 65535), 65535);
        assert_eq!(scale(65535, 0), 0);
        assert_eq!(scale(0, 65535), 0);
    }
}
