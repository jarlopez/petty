use bytes::BytesMut;
use channel;
use channel::RWEvent;
use channel::ReadEvent;
use ops::Ops;
use selector::Selector;
use selector::SelectorKey;
use std::collections::{HashMap, HashSet};
use std::hash::Hash;
use std::hash::Hasher;
use std::io::Error;
use std::io::Read;
use std::io::Write;
use std::mem;
use udt::UdtOpts;
use udt::{self, Epoll, EpollEvents, UdtError, UdtSocket, UdtStatus};

const DEFAULT_UDT_BUF_CAPACITY: usize = 10000;

#[derive(Debug, Eq, PartialEq)]
pub enum ChannelKind {
    Acceptor,
    Connector,
}

#[derive(Debug, Eq, PartialEq)]
pub enum ChannelState {
    Idle,
    Connected,
    Connecting,
}

#[derive(Debug)]
pub struct UdtSelector {
    poller: Epoll,
    selected: HashSet<UdtSocket>,
    registered: HashMap<UdtSocket, UdtKey>,
}

#[derive(Debug, Eq)]
pub struct UdtKey {
    ch: UdtChannel,
    readiness: Ops,
    interest: Ops,
}

#[derive(Debug, Eq)]
pub struct UdtChannel {
    io: SocketIo,
    kind: ChannelKind,
    state: ChannelState,
}

#[derive(Debug, Eq, PartialEq)]
pub struct SocketIo {
    socket: UdtSocket,
}

// impl UdtSelector
impl UdtSelector {
    pub fn new() -> Result<Self, UdtError> {
        udt::init();
        let poller = Epoll::create()?;
        let selector = UdtSelector {
            poller,
            selected: HashSet::new(),
            registered: HashMap::new(),
        };
        Ok(selector)
    }
}

impl Selector<UdtKey> for UdtSelector {
    const DEFAULT_TIMEOUT_MS: i64 = 50;

    fn register(&mut self, mut key: UdtKey, interest: Ops) {
        key.interest = interest;
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

        key.ch
            .io
            .socket
            .setsockopt(UdtOpts::UDT_SNDSYN, false)
            .unwrap();

        self.poller
            .add_usock(&key.socket_clone(), Some(events))
            .expect("add_usock err");
        self.registered.insert(key.socket_clone(), key);
    }

    fn select(&mut self, timeout: i64) {
        // TODO modify poller to re-use fixed length vectors
        let (readers, _writers) = self.poller.wait(timeout as i64, true).unwrap();

        for socket in readers {
            let mut key = {
                match self.registered.get_mut(&socket) {
                    None => {
                        println!("[WARN] unregistered read for {:?}", socket);
                        continue;
                    }
                    Some(key) => key,
                }
            };
            if key.apply_read() {
                self.selected.insert(socket);
            } else {
                println!("Reader {:?} didn't wanna read", socket);
            }
        }
        // TODO process writers
    }

    fn on_selected<F>(&mut self, coll: &mut Vec<RWEvent<UdtKey>>, f: F)
    where
        F: Fn(&mut Vec<RWEvent<UdtKey>>, &mut UdtKey) -> (),
    {
        let mut selected = mem::replace(&mut self.selected, HashSet::new());
        selected.drain().for_each(|r| {
            if let Some(key) = self.registered.get_mut(&r) {
                f(coll, key);
            } else {
                println!("not found!");
            }
        });
    }
}

// impl Key
impl UdtKey {
    pub fn new(ch: UdtChannel) -> Self {
        let mut interest = Ops::empty();
        let readiness = Ops::empty();

        match ch.kind {
            ChannelKind::Acceptor => {
                interest.apply(Ops::ACCEPT);
            }
            ChannelKind::Connector => {
                interest.apply(Ops::CONNECT);
            }
        }

        UdtKey {
            ch,
            readiness,
            interest,
        }
    }

    pub fn socket_clone(&self) -> UdtSocket {
        self.ch.io.socket.clone()
    }
}

impl SelectorKey for UdtKey {
    type Io = UdtChannel;
    type Resource = UdtSocket;

    fn ready_ops(&self) -> Ops {
        self.readiness
    }

    fn io(&mut self) -> &mut Self::Io {
        &mut self.ch
    }

    fn apply_read(&mut self) -> bool {
        match self.ch.kind {
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
        match self.ch.kind {
            ChannelKind::Acceptor => {
                return false;
            }
            ChannelKind::Connector => {
                if self.ch.io.socket.getstate() == UdtStatus::CONNECTED {
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

impl Hash for UdtKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.ch.io.socket.hash(state)
    }
}

impl PartialEq for UdtKey {
    fn eq(&self, other: &UdtKey) -> bool {
        self.ch.io.socket == other.ch.io.socket
    }
}

// impl UdtChannel
impl UdtChannel {
    pub fn new(socket: UdtSocket, kind: ChannelKind) -> Self {
        let io = SocketIo::new(socket);
        UdtChannel {
            io,
            kind,
            state: ChannelState::Idle,
        }
    }
}

impl PartialEq for UdtChannel {
    fn eq(&self, other: &UdtChannel) -> bool {
        self.io.socket == other.io.socket
    }
}

impl channel::Read<UdtKey> for UdtChannel {
    fn read(&mut self, collector: &mut Vec<RWEvent<UdtKey>>) {
        match self.kind {
            ChannelKind::Acceptor => {
                let (peer, addr) = self
                    .io
                    .socket
                    .accept()
                    .expect("told to accept but experienced err!");
                let ch = UdtChannel::new(peer, ChannelKind::Connector);
                let key = UdtKey::new(ch);
                let ev = ReadEvent::NewPeer(key, addr);
                // TODO figure out Netty-like pipeline for funneling read events
                collector.push(RWEvent::Read(ev));
            }
            ChannelKind::Connector => {
                // TODO buffer allocator
                let mut buf = {
                    let mut buf = BytesMut::with_capacity(DEFAULT_UDT_BUF_CAPACITY);
                    buf.resize(DEFAULT_UDT_BUF_CAPACITY, 0u8);
                    buf
                };

                if let Ok(len) = self.io.read(&mut buf) {
                    buf.truncate(len);
                    let ev = ReadEvent::Data(buf.freeze());
                    // TODO figure out Netty-like pipeline for funneling read events
                    collector.push(RWEvent::Read(ev));
                }
            }
        }
    }
}

impl channel::Write for UdtChannel {
    fn write(&mut self) {
        unimplemented!()
    }

    fn flush(&mut self) {
        unimplemented!()
    }
}

// impl SocketIo
impl SocketIo {
    pub fn new(socket: UdtSocket) -> Self {
        SocketIo { socket }
    }
}

impl Read for SocketIo {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        let len = buf.len();
        let len = self.socket.recv(buf, len).expect("TODO UDT error on recv");
        Ok(len as usize)
    }
}

impl Write for SocketIo {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Error> {
        let len = self.socket.send(buf).expect("TODO udt error on send");
        Ok(len as usize)
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
