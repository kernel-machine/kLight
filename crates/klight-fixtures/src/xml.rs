//! Minimal, bounded XML tree built on quick-xml. Malformed input yields an
//! error, never a panic; depth and size are limited so hostile files cannot
//! exhaust memory or the stack.

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::QxfError;

const MAX_DEPTH: usize = 32;
const MAX_ELEMENTS: usize = 200_000;
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Default, Clone)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Element>,
}

impl Element {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    pub fn child_text(&self, name: &str) -> Option<&str> {
        self.child(name).map(|c| c.text.trim())
    }
}

fn local_name(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    match s.rsplit_once(':') {
        Some((_, local)) => local.to_owned(),
        None => s.into_owned(),
    }
}

fn element_from(start: &BytesStart<'_>) -> Result<Element, QxfError> {
    let mut el = Element {
        name: local_name(start.name().as_ref()),
        ..Element::default()
    };
    for attr in start.attributes() {
        let attr = attr.map_err(|e| QxfError::Xml(e.to_string()))?;
        let key = local_name(attr.key.as_ref());
        let value = attr
            .unescape_value()
            .map_err(|e| QxfError::Xml(e.to_string()))?
            .into_owned();
        el.attrs.push((key, value));
    }
    Ok(el)
}

/// Parses a whole document and returns its root element.
pub fn parse(input: &str) -> Result<Element, QxfError> {
    if input.len() > MAX_INPUT_BYTES {
        return Err(QxfError::TooLarge);
    }
    let mut reader = Reader::from_str(input);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;
    let mut count = 0usize;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| QxfError::Xml(format!("at byte {}: {e}", reader.buffer_position())))?;
        match event {
            Event::Start(start) => {
                count += 1;
                if count > MAX_ELEMENTS {
                    return Err(QxfError::TooLarge);
                }
                if stack.len() >= MAX_DEPTH {
                    return Err(QxfError::Xml("document nested too deeply".into()));
                }
                stack.push(element_from(&start)?);
            }
            Event::Empty(start) => {
                count += 1;
                if count > MAX_ELEMENTS {
                    return Err(QxfError::TooLarge);
                }
                let el = element_from(&start)?;
                attach(&mut stack, &mut root, el)?;
            }
            Event::End(_) => {
                let el = stack
                    .pop()
                    .ok_or_else(|| QxfError::Xml("unbalanced end tag".into()))?;
                attach(&mut stack, &mut root, el)?;
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    let text = t.unescape().map_err(|e| QxfError::Xml(e.to_string()))?;
                    top.text.push_str(&text);
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(QxfError::Xml("unexpected end of document".into()));
    }
    root.ok_or_else(|| QxfError::Xml("empty document".into()))
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, el: Element) -> Result<(), QxfError> {
    match stack.last_mut() {
        Some(parent) => {
            parent.children.push(el);
            Ok(())
        }
        None => {
            if root.is_some() {
                return Err(QxfError::Xml("multiple root elements".into()));
            }
            *root = Some(el);
            Ok(())
        }
    }
}
