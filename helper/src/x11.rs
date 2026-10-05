use std::time::{Duration, Instant};

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, ConnectionExt as _, CreateWindowAux, EventMask, Screen, Window, WindowClass,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::COPY_DEPTH_FROM_PARENT;

use crate::Result;

const EVENT_POLL_INTERVAL: Duration = Duration::from_millis(5);

x11rb::atom_manager! {
    pub Atoms: AtomsCookie {
        CLIPBOARD,
        PRIMARY,
        SECONDARY,
        TARGETS,
        INCR,
        UTF8_STRING,
        CLAUDE_PASTE_HELPER,
    }
}

pub struct Display {
    pub conn: RustConnection,
    pub atoms: Atoms,
    screen: usize,
}

impl Display {
    pub fn connect() -> Result<Self> {
        let (conn, screen) =
            x11rb::connect(None).map_err(|error| format!("cannot open display: {error}"))?;
        let atoms = Atoms::new(&conn)?.reply()?;
        Ok(Self {
            conn,
            atoms,
            screen,
        })
    }

    pub fn screen(&self) -> &Screen {
        &self.conn.setup().roots[self.screen]
    }

    pub fn atom(&self, name: &str) -> Result<Atom> {
        Ok(self.conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
    }

    pub fn atom_name(&self, atom: Atom) -> Result<String> {
        Ok(String::from_utf8_lossy(&self.conn.get_atom_name(atom)?.reply()?.name).into_owned())
    }

    pub fn hidden_window(&self, events: EventMask) -> Result<Window> {
        let window = self.conn.generate_id()?;
        self.conn.create_window(
            COPY_DEPTH_FROM_PARENT,
            window,
            self.screen().root,
            0,
            0,
            1,
            1,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(events),
        )?;
        Ok(window)
    }

    pub fn wait_for<T>(
        &self,
        deadline: Instant,
        mut select: impl FnMut(Event) -> Option<T>,
    ) -> Result<T> {
        self.conn.flush()?;
        loop {
            while let Some(event) = self.conn.poll_for_event()? {
                if let Some(selected) = select(event) {
                    return Ok(selected);
                }
            }
            if Instant::now() >= deadline {
                return Err("timed out waiting for the X server".into());
            }
            std::thread::sleep(EVENT_POLL_INTERVAL);
        }
    }
}
