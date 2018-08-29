use bytes::Bytes;
use bytes::BytesMut;
use channel;
use channel::ChWrite;
use channel::{RWEvent, ReadEvent, StateEvent};
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
use std::net::SocketAddr;
use udt::UdtOpts;
use udt::{self, Epoll, EpollEvents, UdtError, UdtSocket, UdtStatus};

const DEFAULT_UDT_BUF_CAPACITY: usize = 10000;

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ChannelKind {
    Acceptor,
    Connector { remote: SocketAddr },
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub enum ChannelState {
    Idle,
    Connected,
    Connecting,
}

#[derive(Debug)]
pub struct UdtSelector {
    poller: Epoll,
    selected: HashSet<UdtSocket>,
    pub registered: HashMap<UdtSocket, UdtKey>,
}

#[derive(Debug, Eq)]
pub struct UdtKey {
    pub ch: UdtChannel,
    pub readiness: Ops,
    pub interest: Ops,
}

#[derive(Debug, Eq, Copy, Clone)]
pub struct UdtChannel {
    pub io: SocketIo,
    pub kind: ChannelKind,
    pub state: ChannelState,
}

#[derive(Debug, Eq, PartialEq, Copy, Clone)]
pub struct SocketIo {
    socket: UdtSocket,

    // Debug info
    bytes_sent: u64,
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
    const DEFAULT_TIMEOUT_MS: i64 = 1000;

    fn register(&mut self, mut key: UdtKey, interest: Ops) {
        key.interest = interest;
        let mut events = EpollEvents::empty();
        if interest.has_read() || interest.has_accept() {
            events |= udt::UDT_EPOLL_IN;
        }
        if interest.has_write() || interest.has_connect() {
            events |= udt::UDT_EPOLL_OUT;
        }
        if interest.has_error() {
            events |= udt::UDT_EPOLL_ERR;
        }

        self.poller
            .add_usock(key.socket_ref(), Some(events))
            .expect("add_usock err");
        let status = key.socket_ref().getstate();
        println!("registered state: {:?}", status);
        self.registered.insert(key.socket_clone(), key);
    }

    fn update_registration(&mut self, key: UdtSocket, interest: Ops) {
        if let Some(k) = self.registered.get_mut(&key) {
            let mut events = EpollEvents::empty();
            if interest.has_read() || interest.has_accept() {
                events |= udt::UDT_EPOLL_IN;
            }
            if interest.has_write() || interest.has_connect() {
                events |= udt::UDT_EPOLL_OUT;
            }
            if interest.has_error() {
                events |= udt::UDT_EPOLL_ERR;
            }
            self.poller.remove_usock(&key).unwrap();
            self.poller.add_usock(&key, Some(events)).unwrap()
        }
    }

    fn select(&mut self, timeout: i64) {
        // TODO modify poller to re-use fixed length vectors
        let (readers, writers) = self.poller.wait(timeout as i64, true).unwrap();
        println!("#r: {:?}, #w: {:?}", readers.len(), writers.len());

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
        for socket in writers {
            let mut key = {
                match self.registered.get_mut(&socket) {
                    None => {
                        println!("[WARN] unregistered write for {:?}", socket);
                        continue;
                    }
                    Some(key) => key,
                }
            };
            if key.apply_write() {
                self.selected.insert(socket);
            } else {
                println!("Writer {:?} didn't wanna write", socket);
            }
        }
    }

    fn on_resource<F>(
        &mut self,
        resource: &<UdtKey as SelectorKey>::Resource,
        coll: &mut Vec<RWEvent<UdtKey>>,
        f: F,
    ) where
        F: Fn(&mut Vec<RWEvent<UdtKey>>, &mut UdtKey) -> (),
    {
        self.registered.get_mut(resource).map(|key| {
            f(coll, key);
        });
    }

    fn on_selected<F>(&mut self, coll: &mut Vec<RWEvent<UdtKey>>, f: F)
    where
        F: Fn(&mut Vec<RWEvent<UdtKey>>, &mut UdtKey) -> (),
    {
        let mut selected = mem::replace(&mut self.selected, HashSet::new());
        selected.drain().for_each(|s| {
            self.registered.get_mut(&s).map_or_else(
                || {
                    println!("Key not found!");
                },
                |key| f(coll, key),
            );
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
            ChannelKind::Connector { .. } => {
                interest.apply(Ops::CONNECT);
            }
        }

        UdtKey {
            ch,
            readiness,
            interest,
        }
    }

    pub fn socket_ref(&self) -> &UdtSocket {
        &self.ch.io.socket
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

    fn set_readiness(&mut self, ops: Ops) {
        self.readiness = ops;
    }

    fn set_interest(&mut self, ops: Ops) {
        self.interest = ops;
    }

    fn io(&mut self) -> &mut Self::Io {
        &mut self.ch
    }

    fn resource(&self) -> Self::Resource {
        self.socket_clone()
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
            ChannelKind::Connector { .. } => {
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
            ChannelKind::Connector { .. } => {
                if self.ch.state() == ChannelState::Connected {
                    // Connected; transition to writable
                    if self.interest.has_write() {
                        self.readiness.apply(Ops::WRITE);
                        return true;
                    } else {
                        return false;
                    }
                } else {
                    // Not connected, but ready to connect
                    if self.interest.has_connect() {
                        self.readiness.apply(Ops::CONNECT);
                        return true;
                    } else {
                        return false;
                    }
                }
            }
        }
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
        // Ensure non-blocking mode
        socket.setsockopt(UdtOpts::UDT_SNDSYN, false).unwrap();
        socket.setsockopt(UdtOpts::UDT_RCVSYN, false).unwrap();

        UdtChannel {
            io,
            kind,
            state: ChannelState::Idle,
        }
    }

    pub fn finish_connect(&mut self) -> ChannelState {
        if self.is_connected() {
            self.state = ChannelState::Connected;
        }
        self.state
    }

    pub fn state(&self) -> ChannelState {
        self.state
    }

    //  Status reporting
    pub fn is_opened(&self) -> bool {
        self.sockstate() == UdtStatus::OPENED
    }
    pub fn is_listening(&self) -> bool {
        self.sockstate() == UdtStatus::LISTENING
    }
    pub fn is_broken(&self) -> bool {
        self.sockstate() == UdtStatus::BROKEN
    }
    pub fn is_closing(&self) -> bool {
        self.sockstate() == UdtStatus::CLOSING
    }
    pub fn is_closed(&self) -> bool {
        self.sockstate() == UdtStatus::CLOSED
    }
    pub fn is_connecting(&self) -> bool {
        self.sockstate() == UdtStatus::CONNECTING
    }
    pub fn is_connected(&self) -> bool {
        self.sockstate() == UdtStatus::CONNECTED
    }

    pub fn sockstate(&self) -> UdtStatus {
        self.io.socket.getstate()
    }
}

impl PartialEq for UdtChannel {
    fn eq(&self, other: &UdtChannel) -> bool {
        self.io.socket == other.io.socket
    }
}
impl channel::ChExt<UdtKey> for UdtChannel {
    fn finish_connect(&mut self, collector: &mut Vec<RWEvent<UdtKey>>) {
        self.finish_connect();
        let addr: SocketAddr = self
            .io
            .socket
            .getpeername()
            .expect("no associated peer with socket in finish_connect");
        let ev: StateEvent<UdtKey> = StateEvent::ConnectedPeer(self.io.socket, addr);
        collector.push(RWEvent::State(ev));
    }
}

impl channel::ChRead<UdtKey> for UdtChannel {
    fn read(&mut self, collector: &mut Vec<RWEvent<UdtKey>>) {
        match self.kind {
            ChannelKind::Acceptor => {
                let (peer, addr) = self
                    .io
                    .socket
                    .accept()
                    .expect("told to accept but experienced err!");

                let ch = UdtChannel::new(
                    peer,
                    ChannelKind::Connector {
                        remote: addr.clone(),
                    },
                );
                let key = UdtKey::new(ch);
                let ev = ReadEvent::NewPeer(key, addr);
                // TODO figure out Netty-like pipeline for funneling read events
                collector.push(RWEvent::Read(ev));
            }
            ChannelKind::Connector { .. } => {
                // TODO buffer allocator
                let mut buf = {
                    let mut buf = BytesMut::with_capacity(DEFAULT_UDT_BUF_CAPACITY);
                    buf.resize(DEFAULT_UDT_BUF_CAPACITY, 0u8);
                    buf
                };

                if let Ok(len) = self.io.read(&mut buf) {
                    buf.truncate(len);
                    let ev = ReadEvent::Data(buf.freeze());
                    println!("Read {:?} bytes", len);
                    // TODO figure out Netty-like pipeline for funneling read events
                    collector.push(RWEvent::Read(ev));
                }
            }
        }
    }
}

impl channel::ChWrite<UdtKey> for UdtChannel {
    fn write(&mut self, data: &Bytes, _collector: &mut Vec<RWEvent<UdtKey>>) {
        // TODO write to buffer, not directly to wire (though UDT buffers internally, so this might be ok)
        let _num_bytes = self.io.socket.send(data.as_ref()).expect("send err");
    }

    fn flush(&mut self, _collector: &mut Vec<RWEvent<UdtKey>>) {
        ()
    }
}

// impl SocketIo
impl SocketIo {
    pub fn new(socket: UdtSocket) -> Self {
        SocketIo {
            socket,
            bytes_sent: 0,
        }
    }
}

impl ChWrite<UdtKey> for SocketIo {
    fn write(&mut self, data: &Bytes, _collector: &mut Vec<RWEvent<UdtKey>>) {
        use std;
        std::io::Write::write(self, data).expect("I/O err on UdtKey ChWrite");
    }

    fn flush(&mut self, _collector: &mut Vec<RWEvent<UdtKey>>) {
        unimplemented!()
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
        self.socket
            .send(buf)
            .map(|len| {
                assert!(len >= 0);
                self.bytes_sent += len as u64;
                len as usize
            }).map_err(|why| {
                println!(
                    "UDT error {:?} on send after {:?} bytes",
                    why, self.bytes_sent
                );
                panic!();
            })
    }

    fn flush(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
