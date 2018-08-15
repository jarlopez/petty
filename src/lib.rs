#[macro_use]
extern crate futures;
extern crate udt;

use std::fmt::Error;
use std::fmt::Formatter;

pub mod channel;
pub mod ev_loop;
pub mod selector;
pub mod transport;

#[derive(Debug, Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ops(usize);

impl Ops {
    const ACCEPT: usize = 0b0000_0001;
    const CONNECT: usize = 0b0000_0010;
    const READ: usize = 0b0000_0100;
    const WRITE: usize = 0b0000_1000;
    const ERROR: usize = 0b0001_0000;

    pub fn empty() -> Self {
        Ops(0)
    }

    pub fn apply(&mut self, state: usize) {
        self.0 |= state;
    }

    pub fn remove(&mut self, state: usize) {
        self.0 &= !state;
    }

    pub fn with_read() -> Self {
        Ops(Ops::READ)
    }

    pub fn with_write() -> Self {
        Ops(Ops::WRITE)
    }

    pub fn with_accept() -> Self {
        Ops(Ops::ACCEPT)
    }

    pub fn with_connect() -> Self {
        Ops(Ops::CONNECT)
    }

    pub fn with_error() -> Self {
        Ops(Ops::CONNECT)
    }

    pub fn has_accept(&self) -> bool {
        (self.0 & Ops::ACCEPT) == Ops::ACCEPT
    }

    pub fn has_connect(&self) -> bool {
        (self.0 & Ops::CONNECT) == Ops::CONNECT
    }

    pub fn has_read(&self) -> bool {
        (self.0 & Ops::READ) == Ops::READ
    }

    pub fn has_write(&self) -> bool {
        (self.0 & Ops::WRITE) == Ops::WRITE
    }
    pub fn has_error(&self) -> bool {
        (self.0 & Ops::ERROR) == Ops::ERROR
    }
}

impl std::fmt::Binary for Ops {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn sanity() {
        assert_eq!(1, 1);
    }
}
