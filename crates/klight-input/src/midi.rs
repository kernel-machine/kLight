//! MIDI message parsing (tolerant: unknown or truncated bytes are skipped).

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MidiEvent {
    NoteOn {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    NoteOff {
        channel: u8,
        note: u8,
        velocity: u8,
    },
    ControlChange {
        channel: u8,
        controller: u8,
        value: u8,
    },
    ProgramChange {
        channel: u8,
        program: u8,
    },
    PitchBend {
        channel: u8,
        value: u16,
    },
    Clock,
    Start,
    Continue,
    Stop,
    SysEx(Vec<u8>),
}

/// Maximum SysEx length accepted.
pub const MAX_SYSEX: usize = 64 * 1024;

/// Parses one buffer (as delivered by a MIDI driver) into events. Supports
/// running status. Never panics.
pub fn parse(bytes: &[u8]) -> Vec<MidiEvent> {
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut running: Option<u8> = None;
    let data = |i: usize| bytes.get(i).copied().filter(|b| *b < 0x80);
    while let Some(&b) = bytes.get(i) {
        // System real-time messages can appear anywhere.
        match b {
            0xF8 => {
                out.push(MidiEvent::Clock);
                i += 1;
                continue;
            }
            0xFA => {
                out.push(MidiEvent::Start);
                i += 1;
                continue;
            }
            0xFB => {
                out.push(MidiEvent::Continue);
                i += 1;
                continue;
            }
            0xFC => {
                out.push(MidiEvent::Stop);
                i += 1;
                continue;
            }
            0xF0 => {
                let end = bytes.iter().skip(i).position(|&x| x == 0xF7).map(|p| i + p);
                match end {
                    Some(e) if e - i <= MAX_SYSEX => {
                        out.push(MidiEvent::SysEx(
                            bytes.get(i..=e).map(<[u8]>::to_vec).unwrap_or_default(),
                        ));
                        i = e + 1;
                    }
                    _ => break,
                }
                running = None;
                continue;
            }
            _ => {}
        }
        let (status, first) = if b >= 0x80 {
            if b >= 0xF0 {
                // Other system common messages: skip.
                running = None;
                i += 1;
                continue;
            }
            running = Some(b);
            (b, i + 1)
        } else if let Some(r) = running {
            (r, i)
        } else {
            i += 1;
            continue;
        };
        let channel = status & 0x0F;
        let needs_two = !matches!(status & 0xF0, 0xC0 | 0xD0);
        let Some(d1) = data(first) else {
            i = first.max(i + 1);
            continue;
        };
        if needs_two {
            let Some(d2) = data(first + 1) else {
                i = first + 1;
                continue;
            };
            out.push(match status & 0xF0 {
                0x90 if d2 > 0 => MidiEvent::NoteOn {
                    channel,
                    note: d1,
                    velocity: d2,
                },
                0x90 | 0x80 => MidiEvent::NoteOff {
                    channel,
                    note: d1,
                    velocity: d2,
                },
                0xB0 => MidiEvent::ControlChange {
                    channel,
                    controller: d1,
                    value: d2,
                },
                0xE0 => MidiEvent::PitchBend {
                    channel,
                    value: u16::from(d1) | (u16::from(d2) << 7),
                },
                _ => {
                    i = first + 2;
                    continue;
                }
            });
            i = first + 2;
        } else {
            if status & 0xF0 == 0xC0 {
                out.push(MidiEvent::ProgramChange {
                    channel,
                    program: d1,
                });
            }
            i = first + 1;
        }
    }
    out
}

pub fn note_on(channel: u8, note: u8, velocity: u8) -> [u8; 3] {
    [0x90 | (channel & 0x0F), note & 0x7F, velocity & 0x7F]
}

pub fn control_change(channel: u8, controller: u8, value: u8) -> [u8; 3] {
    [0xB0 | (channel & 0x0F), controller & 0x7F, value & 0x7F]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_status_and_realtime() {
        let ev = parse(&[0x90, 60, 100, 61, 0, 0xF8, 0xB1, 7, 64]);
        assert_eq!(
            ev,
            vec![
                MidiEvent::NoteOn {
                    channel: 0,
                    note: 60,
                    velocity: 100
                },
                MidiEvent::NoteOff {
                    channel: 0,
                    note: 61,
                    velocity: 0
                },
                MidiEvent::Clock,
                MidiEvent::ControlChange {
                    channel: 1,
                    controller: 7,
                    value: 64
                },
            ]
        );
    }

    #[test]
    fn sysex_and_truncated() {
        assert_eq!(
            parse(&[0xF0, 1, 2, 0xF7]),
            vec![MidiEvent::SysEx(vec![0xF0, 1, 2, 0xF7])]
        );
        assert!(parse(&[0x90, 60]).is_empty());
        assert!(parse(&[0xF0, 1, 2]).is_empty());
    }
}
