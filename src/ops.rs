use std::fmt::Binary;
use std::fmt::Error;
use std::fmt::Formatter;

#[derive(Debug, Copy, PartialEq, Eq, Clone, PartialOrd, Ord)]
pub struct Ops(usize);

impl Ops {
    pub const ACCEPT: usize = 0b0000_0001;
    pub const CONNECT: usize = 0b0000_0010;
    pub const READ: usize = 0b0000_0100;
    pub const WRITE: usize = 0b0000_1000;
    pub const ERROR: usize = 0b0001_0000;

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

impl Binary for Ops {
    fn fmt(&self, f: &mut Formatter) -> Result<(), Error> {
        self.0.fmt(f)
    }
}
