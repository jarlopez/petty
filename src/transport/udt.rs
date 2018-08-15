use channel;
use ev_loop::SelectorEventLoop;
use selector::Selector;
use selector::SelectorKey;
use std::cell::RefCell;
use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;
use std::hash::Hasher;
use std::io;
use std::io::Read;
use std::rc::Rc;
use udt;
use udt::UdtError;
use udt::UdtSocket;
use udt::UdtStatus;
use udt::{Epoll, EpollEvents};
use Ops;

#[derive(Copy, Clone, Eq, PartialEq, Hash)]
pub enum ChannelKind {
    Acceptor,
    Connector,
}

// TODO question: how to wrap UdtSelectorKey? 'registered' "owns" the keys, but 'selected' can point to them
pub struct UdtSelector {
    poller: Epoll,
    selected: HashSet<UdtSocket>,
    registered: HashMap<UdtSocket, UdtSelectorKey>,
}

#[derive(Eq, PartialEq)]
pub struct UdtSelectorKey {
    channel: UdtChannel,
    readiness: Ops,
    interest: Ops,
}

#[derive(Eq)]
pub struct UdtSocketIo {
    socket: UdtSocket,
}

impl UdtSocketIo {
    pub fn new(socket: UdtSocket) -> Self {
        UdtSocketIo { socket }
    }
}
impl Read for UdtSocketIo {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        let len = buf.len();
        let nbytes = self
            .socket
            .recv(buf, len)
            .map_err(|udt_err| {
                // TODO
                println!("err!!! {:?}", udt_err);
            }).expect("internal UDT err");
        Ok(nbytes as usize)
    }
}

impl Hash for UdtSocketIo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.socket.hash(state)
    }
}

impl PartialEq for UdtSocketIo {
    fn eq(&self, other: &UdtSocketIo) -> bool {
        self.socket == other.socket
    }
}

impl UdtSelector {
    pub fn new() -> Result<Self, UdtError> {
        let poller = Epoll::create()?;
        let selector = UdtSelector {
            poller,
            selected: HashSet::new(),
            registered: HashMap::new(),
        };
        Ok(selector)
    }
}

impl UdtSelectorKey {
    pub fn new(channel: UdtChannel) -> Self {
        ///  TODO poopulate interest based on channel kind
        let interest = match channel.kind {
            ChannelKind::Acceptor => Ops::with_accept(),
            ChannelKind::Connector => Ops::with_connect(),
        };

        UdtSelectorKey {
            channel,
            readiness: Ops::empty(),
            interest,
        }
    }

    pub fn with_state(channel: UdtChannel, state: Ops) -> Self {
        UdtSelectorKey {
            channel,
            readiness: state,
            interest: Ops::empty(),
        }
    }
}
impl Hash for UdtSelectorKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.channel.io.socket.hash(state)
    }
}

impl SelectorKey for UdtSelectorKey {
    type Io = UdtChannel;
    type Resource = UdtSocket;

    fn ready_ops(&self) -> Ops {
        self.readiness
    }

    fn io(&mut self) -> &mut Self::Io {
        &mut self.channel
    }

    fn apply_read(&mut self) -> bool {
        match self.channel.kind {
            ChannelKind::Acceptor => {
                if self.interest.has_accept() {
                    self.readiness.apply(Ops::ACCEPT);
                } else {
                    return false;
                }
            }
            ChannelKind::Connector => {
                if self.interest.has_read() {
                    self.readiness.apply(Ops::READ);
                } else {
                    return false;
                }
            }
        }
        true
    }

    fn apply_write(&mut self) -> bool {
        match self.channel.kind {
            ChannelKind::Acceptor => {
                return false;
            }
            ChannelKind::Connector => {
                if self.channel.io.socket.getstate() == UdtStatus::CONNECTED {
                    // Connected; transition to writable
                    if self.interest.has_write() {
                        self.readiness.apply(Ops::WRITE);
                    }
                } else {
                    // Not connected, but ready to connect
                    if self.interest.has_connect() {
                        self.readiness.apply(Ops::CONNECT);
                    } else {
                        return false;
                    }
                }
            }
        }
        true
    }
}

impl Selector<UdtSelectorKey> for UdtSelector {
    const DEFAULT_TIMEOUT_MS: i64 = 1000;

    fn register(&mut self, key: UdtSelectorKey, interest: Ops) {
        println!("[UdtSelector] registering key");
        let mut events = EpollEvents::empty();
        if interest.has_read() {
            events |= udt::UDT_EPOLL_IN;
        }
        if interest.has_write() {
            events |= udt::UDT_EPOLL_OUT;
        }
        if interest.has_error() {
            events |= udt::UDT_EPOLL_ERR;
        }

        self.poller
            .add_usock(&key.channel.io.socket, Some(events))
            .expect("add_usock err");
        self.registered.insert(key.channel.io.socket, key);
    }

    fn select(&mut self, timeout: i64) {
        println!("[UdtSelector] selecting with timeout {:?}", timeout);
        let (readers, writers) = self.poller.wait(timeout as i64, true).unwrap();
        println!("numreaders: {:?}", readers.len());

        // TODO process readers, make sure they want to be added to 'selected' based on entries in 'registered'
        // process readers
        for socket in readers {
            let key = {
                match self.registered.get_mut(&socket) {
                    None => {
                        println!("[WARN] unregistered read for {:?}", socket);
                        continue;
                    }
                    Some(key) => key,
                }
            };
            if key.apply_read() {
                // Ready to be processed as a newly selected key
                println!("Reader {:?} ready and selected", socket);
                self.selected.insert(socket);
            } else {
                println!("Reader {:?} didn't wanna read", socket);
            }
        }
        // process writers
    }

    fn selected_keys(&self) -> &HashSet<UdtSocket> {
        &self.selected
    }

    fn on_selected<F>(&mut self, f: F)
    where
        F: Fn(&mut UdtSelectorKey) -> (),
    {
        for resource in self.selected.drain() {
            self.registered.get_mut(&resource).map(&f);
        }
    }
}

#[derive(Eq)]
pub struct UdtChannel {
    io: UdtSocketIo,
    kind: ChannelKind,
}

impl UdtChannel {
    pub fn new(socket: UdtSocket, kind: ChannelKind) -> Self {
        UdtChannel {
            io: UdtSocketIo::new(socket),
            kind,
        }
    }
}

impl channel::Read for UdtChannel {
    fn read(&mut self) {
        match self.kind {
            ChannelKind::Acceptor => {
                println!("Accepting new channel");
                let peer = self
                    .io
                    .socket
                    .accept()
                    .expect("told to accept but experienced err!");
                // TODO register the new channel with our epoll selector
            }
            ChannelKind::Connector => {
                println!("TODO implement Connectors channel::Read");
            }
        }
    }
}

impl channel::Write for UdtChannel {
    fn write(&mut self) {
        match self.kind {
            ChannelKind::Acceptor => {
                unimplemented!();
            }
            ChannelKind::Connector => {
                unimplemented!();
            }
        }
    }

    fn flush(&mut self) {
        match self.kind {
            ChannelKind::Acceptor => {
                unimplemented!();
            }
            ChannelKind::Connector => {
                unimplemented!();
            }
        }
    }
}

impl PartialEq for UdtChannel {
    fn eq(&self, other: &UdtChannel) -> bool {
        self.io.socket == other.io.socket
    }
}

impl Hash for UdtChannel {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.io.socket.hash(state)
    }
}
