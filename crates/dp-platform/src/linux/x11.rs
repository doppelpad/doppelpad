//! A connection to the X server with the EWMH lookups the foreground watcher and the window
//! commands share.

use x11rb::connection::Connection;
use x11rb::errors::ReplyError;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

pub struct X11 {
    pub conn: RustConnection,
    /// The root window of the default screen.
    pub root: Window,
    net_active_window: Atom,
}

impl X11 {
    /// Connect to `$DISPLAY`.
    pub fn connect() -> Result<X11, String> {
        if std::env::var_os("DISPLAY").is_none() {
            return Err("no X display".into());
        }
        let (conn, screen) = x11rb::connect(None).map_err(|e| e.to_string())?;
        let root = conn
            .setup()
            .roots
            .get(screen)
            .ok_or_else(|| format!("the X server has no screen {screen}"))?
            .root;
        let mut x11 = X11 {
            conn,
            root,
            net_active_window: 0,
        };
        x11.net_active_window = x11.atom(b"_NET_ACTIVE_WINDOW").map_err(|e| e.to_string())?;
        Ok(x11)
    }

    /// The atom called `name`.
    pub fn atom(&self, name: &[u8]) -> Result<Atom, ReplyError> {
        Ok(self.conn.intern_atom(false, name)?.reply()?.atom)
    }

    /// The first 32-bit value of `property` (of type `ty`) on `window`.
    pub fn card32(
        &self,
        window: Window,
        property: Atom,
        ty: AtomEnum,
    ) -> Result<Option<u32>, ReplyError> {
        let reply = self
            .conn
            .get_property(false, window, property, ty, 0, 1)?
            .reply()?;
        Ok(reply.value32().and_then(|mut v| v.next()))
    }

    /// The window `_NET_ACTIVE_WINDOW` names; `None` when no window has the focus.
    pub fn active_window(&self) -> Result<Option<Window>, ReplyError> {
        Ok(self
            .card32(self.root, self.net_active_window, AtomEnum::WINDOW)?
            .filter(|&w| w != 0))
    }
}
