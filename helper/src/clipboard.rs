use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, EventMask, PropMode, Property, SelectionNotifyEvent,
    SelectionRequestEvent, Window, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

use crate::x11::Display;
use crate::Result;

const OWNER_REPLY_TIMEOUT: Duration = Duration::from_secs(3);
const INCR_CHUNK_TIMEOUT: Duration = Duration::from_secs(5);
const PROPERTY_READ_WORDS: u32 = 1 << 20;
const TEXT_TARGETS: [&str; 5] = [
    "UTF8_STRING",
    "text/plain;charset=utf-8",
    "text/plain",
    "STRING",
    "TEXT",
];

fn selection_atom(display: &Display, selection: &str) -> Atom {
    match selection {
        "primary" | "p" => display.atoms.PRIMARY,
        "secondary" | "s" => display.atoms.SECONDARY,
        _ => display.atoms.CLIPBOARD,
    }
}

fn read_order<'a>(target: &'a str) -> impl Iterator<Item = &'a str> {
    let fallbacks: &[&'a str] = if TEXT_TARGETS.contains(&target) {
        &TEXT_TARGETS
    } else {
        &[]
    };
    std::iter::once(target).chain(
        fallbacks
            .iter()
            .copied()
            .filter(move |name| *name != target),
    )
}

fn read_whole_property(
    display: &Display,
    window: Window,
    property: Atom,
) -> Result<(Atom, Vec<u8>)> {
    let mut data = Vec::new();
    let mut offset = 0;
    loop {
        let reply = display
            .conn
            .get_property(
                false,
                window,
                property,
                AtomEnum::ANY,
                offset,
                PROPERTY_READ_WORDS,
            )?
            .reply()?;
        data.extend_from_slice(&reply.value);
        offset += (reply.value.len() / 4) as u32;
        if reply.bytes_after == 0 {
            display.conn.delete_property(window, property)?;
            display.conn.flush()?;
            return Ok((reply.type_, data));
        }
    }
}

fn read_incrementally(display: &Display, window: Window, property: Atom) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    loop {
        display.wait_for(Instant::now() + INCR_CHUNK_TIMEOUT, |event| match event {
            Event::PropertyNotify(e) if e.atom == property && e.state == Property::NEW_VALUE => {
                Some(())
            }
            _ => None,
        })?;
        let (_, chunk) = read_whole_property(display, window, property)?;
        if chunk.is_empty() {
            return Ok(data);
        }
        data.extend_from_slice(&chunk);
    }
}

fn convert(
    display: &Display,
    window: Window,
    selection: Atom,
    target: Atom,
) -> Result<Option<(Atom, Vec<u8>)>> {
    let property = display.atoms.CLAUDE_PASTE_HELPER;
    display
        .conn
        .convert_selection(window, selection, target, property, CURRENT_TIME)?;
    let notified_property =
        display.wait_for(Instant::now() + OWNER_REPLY_TIMEOUT, |event| match event {
            Event::SelectionNotify(e) if e.requestor == window => Some(e.property),
            _ => None,
        })?;
    if notified_property == NONE {
        return Ok(None);
    }
    let (kind, data) = read_whole_property(display, window, property)?;
    if kind == display.atoms.INCR {
        return Ok(Some((kind, read_incrementally(display, window, property)?)));
    }
    Ok(Some((kind, data)))
}

pub fn read(selection: &str, target: &str) -> Result<Vec<u8>> {
    let display = Display::connect()?;
    let selection = selection_atom(&display, selection);
    if display.conn.get_selection_owner(selection)?.reply()?.owner == NONE {
        return Err("the clipboard is empty".into());
    }
    let window = display.hidden_window(EventMask::PROPERTY_CHANGE)?;
    for name in read_order(target) {
        let target_atom = display.atom(name)?;
        let Some((kind, data)) = convert(&display, window, selection, target_atom)? else {
            continue;
        };
        if name == "TARGETS" && kind == u32::from(AtomEnum::ATOM) {
            return list_targets(&display, &data);
        }
        return Ok(data);
    }
    Err(format!("the clipboard holds no {target}").into())
}

fn list_targets(display: &Display, data: &[u8]) -> Result<Vec<u8>> {
    let mut names = String::new();
    for word in data.as_chunks::<4>().0 {
        names.push_str(&display.atom_name(u32::from_ne_bytes(*word))?);
        names.push('\n');
    }
    Ok(names.into_bytes())
}

struct Offer {
    targets: Vec<Atom>,
    data: Vec<u8>,
    kind: Atom,
}

fn answer(
    display: &Display,
    request: &SelectionRequestEvent,
    offer: &Offer,
    targets_atom: Atom,
) -> Result<()> {
    let property = if request.property == NONE {
        request.target
    } else {
        request.property
    };
    let fits = offer.data.len() + 64 < display.conn.maximum_request_bytes();
    let delivered = if request.target == targets_atom {
        let mut atoms = vec![targets_atom];
        atoms.extend(&offer.targets);
        display.conn.change_property32(
            PropMode::REPLACE,
            request.requestor,
            property,
            AtomEnum::ATOM,
            &atoms,
        )?;
        true
    } else if offer.targets.contains(&request.target) && fits {
        display.conn.change_property8(
            PropMode::REPLACE,
            request.requestor,
            property,
            offer.kind,
            &offer.data,
        )?;
        true
    } else {
        false
    };
    let event = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: request.time,
        requestor: request.requestor,
        selection: request.selection,
        target: request.target,
        property: if delivered { property } else { NONE },
    };
    display
        .conn
        .send_event(false, request.requestor, EventMask::NO_EVENT, event)?;
    display.conn.flush()?;
    Ok(())
}

pub fn serve(selection: &str, target: Option<&str>, data: Vec<u8>) -> Result<()> {
    let display = Display::connect()?;
    let selection = selection_atom(&display, selection);
    let targets_atom = display.atoms.TARGETS;
    let target_names: Vec<&str> = match target {
        Some(name) if !TEXT_TARGETS.contains(&name) => vec![name],
        _ => TEXT_TARGETS.to_vec(),
    };
    let targets = target_names
        .iter()
        .map(|name| display.atom(name))
        .collect::<Result<Vec<_>>>()?;
    let kind = if target_names.len() == 1 {
        targets[0]
    } else {
        display.atoms.UTF8_STRING
    };
    let offer = Offer {
        targets,
        data,
        kind,
    };

    let window = display.hidden_window(EventMask::NO_EVENT)?;
    display
        .conn
        .set_selection_owner(window, selection, CURRENT_TIME)?;
    if display.conn.get_selection_owner(selection)?.reply()?.owner != window {
        return Err("could not take ownership of the clipboard".into());
    }
    loop {
        match display.conn.wait_for_event()? {
            Event::SelectionRequest(request) => answer(&display, &request, &offer, targets_atom)?,
            Event::SelectionClear(_) => return Ok(()),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn falls_back_through_the_text_targets_for_text_only() {
        assert_eq!(
            read_order("image/png").collect::<Vec<_>>(),
            vec!["image/png"]
        );
        assert_eq!(read_order("TARGETS").collect::<Vec<_>>(), vec!["TARGETS"]);
        let text: Vec<_> = read_order("text/plain").collect();
        assert_eq!(text[0], "text/plain");
        assert_eq!(text.len(), TEXT_TARGETS.len());
        assert!(text.contains(&"UTF8_STRING"));
    }
}
