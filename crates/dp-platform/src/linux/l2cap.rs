//! An L2CAP socket on a Bluetooth LE device's ATT channel, which needs no privileges: how
//! Doppelpad connects a Switch 2 controller and speaks ATT with it (see [`super::ble`]).

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Instant;

use super::att;
use super::dev::{poll_until, pollfd};
use crate::bdaddr::{BdAddr, LeAddrType};
use crate::switch2gatt::WRITE_TIMEOUT;

/// `struct sockaddr_l2` of `<bluetooth/l2cap.h>`.
#[repr(C)]
struct SockaddrL2 {
    family: libc::sa_family_t,
    psm: u16,
    /// Least significant byte first.
    bdaddr: [u8; 6],
    cid: u16,
    bdaddr_type: u8,
}

const BTPROTO_L2CAP: libc::c_int = 0;
const BDADDR_LE_PUBLIC: u8 = 1;
const BDADDR_LE_RANDOM: u8 = 2;

impl SockaddrL2 {
    /// The ATT channel of the Bluetooth LE device at `address`.
    fn att((address, kind): (BdAddr, LeAddrType)) -> SockaddrL2 {
        SockaddrL2 {
            family: libc::AF_BLUETOOTH as libc::sa_family_t,
            psm: 0,
            bdaddr: address.le_bytes(),
            cid: att::CID.to_le(),
            bdaddr_type: match kind {
                LeAddrType::Public => BDADDR_LE_PUBLIC,
                LeAddrType::Random => BDADDR_LE_RANDOM,
            },
        }
    }
}

/// A Bluetooth LE device's address and its kind.
pub type LeAddr = (BdAddr, LeAddrType);

/// The socket. It keeps PDUs apart; it is non-blocking, and every wait has a deadline.
pub struct AttSocket(OwnedFd);

impl AttSocket {
    /// Start connecting from the adapter `local` to the device `remote`;
    /// [`AttSocket::connected`] tells when it is done.
    pub fn connect(local: LeAddr, remote: LeAddr) -> io::Result<Self> {
        // SAFETY: plain socket creation; the descriptor is owned at once.
        let fd = unsafe {
            libc::socket(
                libc::AF_BLUETOOTH,
                libc::SOCK_SEQPACKET | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
                BTPROTO_L2CAP,
            )
        };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: a descriptor that was just created and is owned by nothing else.
        let socket = AttSocket(unsafe { OwnedFd::from_raw_fd(fd) });
        let len = std::mem::size_of::<SockaddrL2>() as libc::socklen_t;
        let (local, remote) = (SockaddrL2::att(local), SockaddrL2::att(remote));
        // SAFETY: a valid address of the given length, for a socket of its family.
        if unsafe { libc::bind(fd, (&raw const local).cast(), len) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: as above.
        if unsafe { libc::connect(fd, (&raw const remote).cast(), len) } < 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() != Some(libc::EINPROGRESS) {
                return Err(e);
            }
        }
        Ok(socket)
    }

    /// Whether the connection is made, waiting until `until`; an error if it failed.
    pub fn connected(&self, until: Instant) -> io::Result<bool> {
        if !self.wait(libc::POLLOUT, until)? {
            return Ok(false);
        }
        let mut error: libc::c_int = 0;
        let mut size = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
        // SAFETY: an int-sized buffer for SO_ERROR.
        let read = unsafe {
            libc::getsockopt(
                self.0.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&raw mut error).cast(),
                &raw mut size,
            )
        };
        if read < 0 {
            return Err(io::Error::last_os_error());
        }
        if error != 0 {
            return Err(io::Error::from_raw_os_error(error));
        }
        Ok(true)
    }

    /// Wait until the socket is ready for `events`; `false` once `until` passed. An error or
    /// hang-up shows in the call that follows.
    fn wait(&self, events: libc::c_short, until: Instant) -> io::Result<bool> {
        Ok(poll_until(&mut [pollfd(self.0.as_raw_fd(), events)], until)? > 0)
    }

    /// Send a PDU, waiting until `until` for room.
    pub fn send(&self, pdu: &[u8], until: Instant) -> io::Result<()> {
        loop {
            if self.try_send(pdu)? {
                return Ok(());
            }
            if !self.wait(libc::POLLOUT, until)? {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "no room to send to the controller",
                ));
            }
        }
    }

    /// Send a PDU now, or not at all (`false`: no room).
    pub fn try_send(&self, pdu: &[u8]) -> io::Result<bool> {
        loop {
            // SAFETY: sending from a buffer of this length.
            let n = unsafe {
                libc::send(
                    self.0.as_raw_fd(),
                    pdu.as_ptr().cast(),
                    pdu.len(),
                    libc::MSG_DONTWAIT | libc::MSG_NOSIGNAL,
                )
            };
            if n >= 0 {
                return Ok(true);
            }
            let e = io::Error::last_os_error();
            match e.kind() {
                io::ErrorKind::WouldBlock => return Ok(false),
                io::ErrorKind::Interrupted => {}
                _ => return Err(e),
            }
        }
    }

    /// The next PDU, or `None` once `until` passed. An error means the link is gone.
    pub fn recv(&self, until: Instant) -> io::Result<Option<Vec<u8>>> {
        let mut buf = vec![0u8; usize::from(att::MTU)];
        loop {
            // SAFETY: receiving into a buffer of this length.
            let n = unsafe {
                libc::recv(
                    self.0.as_raw_fd(),
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                    libc::MSG_DONTWAIT,
                )
            };
            if n > 0 {
                buf.truncate(n as usize);
                return Ok(Some(buf));
            }
            if n == 0 {
                return Err(io::ErrorKind::UnexpectedEof.into());
            }
            let e = io::Error::last_os_error();
            match e.kind() {
                io::ErrorKind::WouldBlock => {
                    if !self.wait(libc::POLLIN, until)? {
                        return Ok(None);
                    }
                }
                io::ErrorKind::Interrupted => {}
                _ => return Err(e),
            }
        }
    }

    /// End the link, also for whoever else holds the socket.
    pub fn shutdown(&self) {
        // SAFETY: shutting down a socket this value owns.
        unsafe { libc::shutdown(self.0.as_raw_fd(), libc::SHUT_RDWR) };
    }
}

impl att::Bearer for AttSocket {
    fn send(&mut self, pdu: &[u8]) -> io::Result<()> {
        AttSocket::send(self, pdu, Instant::now() + WRITE_TIMEOUT)
    }

    fn recv(&mut self, until: Instant) -> io::Result<Option<Vec<u8>>> {
        AttSocket::recv(self, until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_addresses_are_the_kernels() {
        assert_eq!(std::mem::size_of::<SockaddrL2>(), 14);
        let a = SockaddrL2::att((BdAddr::new(0x0605_0403_0201).unwrap(), LeAddrType::Random));
        assert_eq!(a.cid, 4u16.to_le());
        assert_eq!(a.bdaddr, [1, 2, 3, 4, 5, 6]);
        assert_eq!(a.bdaddr_type, BDADDR_LE_RANDOM);
    }
}
