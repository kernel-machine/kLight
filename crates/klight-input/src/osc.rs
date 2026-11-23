//! OSC decoding (never panics on hostile packets).

use rosc::{OscPacket, OscType};

use crate::mapping::InputEvent;

const MAX_BUNDLE_DEPTH: usize = 8;

/// Decodes a UDP datagram into input events (first numeric argument, if any).
pub fn decode(bytes: &[u8]) -> Vec<InputEvent> {
    let mut out = Vec::new();
    if let Ok((_, packet)) = rosc::decoder::decode_udp(bytes) {
        walk(&packet, 0, &mut out);
    }
    out
}

fn walk(p: &OscPacket, depth: usize, out: &mut Vec<InputEvent>) {
    if depth > MAX_BUNDLE_DEPTH || out.len() > 1024 {
        return;
    }
    match p {
        OscPacket::Message(m) => {
            let value = m.args.iter().find_map(|a| match a {
                OscType::Float(f) => Some(*f),
                OscType::Double(d) => Some(*d as f32),
                OscType::Int(i) => Some(*i as f32),
                OscType::Long(l) => Some(*l as f32),
                OscType::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
                _ => None,
            });
            out.push(InputEvent::Osc {
                path: m.addr.clone(),
                value,
            });
        }
        OscPacket::Bundle(b) => {
            for c in &b.content {
                walk(c, depth + 1, out);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rosc::{encoder, OscMessage};

    #[test]
    fn decodes_message() {
        let bytes = encoder::encode(&OscPacket::Message(OscMessage {
            addr: "/klight/gm".into(),
            args: vec![OscType::Float(0.25)],
        }))
        .unwrap();
        assert_eq!(
            decode(&bytes),
            vec![InputEvent::Osc {
                path: "/klight/gm".into(),
                value: Some(0.25)
            }]
        );
        assert!(decode(&[1, 2, 3]).is_empty());
    }
}
