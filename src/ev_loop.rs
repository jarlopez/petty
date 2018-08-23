use channel::RWEvent;
use channel::ReadEvent;
use futures::sync::mpsc;
use ops::Ops;
use selector::Selector;
use selector::SelectorKey;
use std::marker::PhantomData;

pub struct SelectorEventLoop<S, K>
where
    S: Selector<K>,
    K: SelectorKey,
{
    selector: S,
    // Outgoing events from this event loop
    events: mpsc::UnboundedSender<Trigger>,
    key: PhantomData<K>,
    events_buf: Vec<RWEvent<K>>,
}

impl<S, K> SelectorEventLoop<S, K>
where
    S: Selector<K>,
    K: SelectorKey,
{
    pub fn new(selector: S) -> (Self, mpsc::UnboundedReceiver<Trigger>) {
        let (tx, rx) = mpsc::unbounded();

        let event_loop = SelectorEventLoop {
            selector,
            events: tx,
            key: PhantomData,
            events_buf: Vec::new(),
        };
        (event_loop, rx)
    }

    pub fn register(&mut self, key: K, ops: Ops) {
        self.selector.register(key, ops);
    }

    pub fn run(&mut self) {
        loop {
            self.selector.select(S::DEFAULT_TIMEOUT_MS);
            self.process_selected();
            self.run_io_tasks();
        }
    }

    fn process_selected(&mut self) {
        use channel::{Read, Write};

        self.selector
            .on_selected(&mut self.events_buf, |ev, key: &mut K| {
                let ready_ops = key.ready_ops();
                let io = key.io();

                if ready_ops.has_read() || ready_ops.has_accept() {
                    io.read(ev);
                }

                if ready_ops.has_write() {
                    io.flush(ev);
                }
            });
        // TODO the event loop shouldn't really be driving this logic
        // TODO but instead something like Netty's Unsafe abstractions
        for ev in self.events_buf.drain(..) {
            match ev {
                RWEvent::Read(ReadEvent::NewPeer(key, addr)) => {
                    let mut ops = Ops::empty();
                    ops.apply(Ops::READ);
                    ops.apply(Ops::WRITE);
                    ops.apply(Ops::ERROR);
                    self.selector.register(key, ops);
                    self.events
                        .unbounded_send(Trigger::State(events::StateEvent::Connected(addr)))
                        .expect("Dropped unbounded events receiver");
                }
                RWEvent::Read(ReadEvent::Data(bytes)) => {
                    self.events
                        .unbounded_send(Trigger::Read(events::ReadEvent::Data(bytes)))
                        .expect("Dropped unbounded events receiver");
                }
                other => {
                    println!("Unhandled event {:?}", other);
                }
            }
        }
    }

    fn run_io_tasks(&mut self) {}
}

#[derive(Debug)]
pub enum Trigger {
    State(events::StateEvent),
    Read(events::ReadEvent),
    Write(events::WriteEvent),
    Error(events::ErrorEvent),
}
mod events {
    use bytes::Bytes;
    use std::net::SocketAddr;

    #[derive(Debug)]
    pub enum StateEvent {
        Connected(SocketAddr),
        Disconnected(SocketAddr),
    }

    #[derive(Debug)]
    pub enum ReadEvent {
        Data(Bytes),
    }
    #[derive(Debug)]
    pub struct WriteEvent;
    #[derive(Debug)]
    pub struct ErrorEvent;
}
