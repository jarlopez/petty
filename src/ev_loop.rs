use bytes::Bytes;
use channel::RWEvent;
use channel::ReadEvent;
use channel::RegistrationEvent;
use channel::StateEvent;
use futures;
use ops::Ops;
use selector::Selector;
use selector::SelectorKey;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::sync::mpsc;
use std::sync::mpsc::TryRecvError;

pub trait FnBox<S, K>
where
    K: SelectorKey,
    S: Selector<K>,
{
    fn call_box(self: Box<Self>, sys: &mut S, events: futures::sync::mpsc::UnboundedSender<Trigger<K>>);
}

impl<S, K, F> FnBox<S, K> for F
where
    F: FnOnce(&mut S, futures::sync::mpsc::UnboundedSender<Trigger<K>>),
    K: SelectorKey,
    S: Selector<K>,
{
    fn call_box(self: Box<F>, sys: &mut S, events: futures::sync::mpsc::UnboundedSender<Trigger<K>>) {
        (*self)(sys, events);
    }
}

pub type Work<'a, S, K> = Box<FnBox<S, K> + Send + 'a>;

pub struct SelectorEventLoop<S, K>
where
    S: Selector<K>,
    K: SelectorKey,
{
    selector: S,
    // Outgoing events from this event loop
    events: futures::sync::mpsc::UnboundedSender<Trigger<K>>,
    io_tasks: mpsc::Receiver<Work<'static, S, K>>,
    key: PhantomData<K>,
    events_buf: Vec<RWEvent<K>>,
}

impl<S, K> SelectorEventLoop<S, K>
where
    S: Selector<K>,
    K: SelectorKey,
{
    pub fn new(
        selector: S,
    ) -> (
        Self,
        mpsc::Sender<Work<'static, S, K>>,
        futures::sync::mpsc::UnboundedReceiver<Trigger<K>>,
    ) {
        let (ev_tx, ev_rx) = futures::sync::mpsc::unbounded();
        let (io_tx, io_rx) = mpsc::channel();

        let event_loop = SelectorEventLoop {
            selector,
            events: ev_tx,
            io_tasks: io_rx,
            key: PhantomData,
            events_buf: Vec::new(),
        };
        (event_loop, io_tx, ev_rx)
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
        use channel::{ChExt, ChRead, ChWrite};

        self.selector
            .on_selected(&mut self.events_buf, |ev, key: &mut K| {
                let ready_ops = key.ready_ops();
                println!("handling {:?}", ready_ops);

                if ready_ops.has_connect() {
                    let mut updated_ops = Ops::from(ready_ops);
                    updated_ops.remove(Ops::CONNECT);
                    key.set_readiness(updated_ops);
                    key.set_interest(updated_ops);
                    ev.push(RWEvent::Registration(RegistrationEvent::Update(
                        key.resource(),
                        updated_ops,
                    )));
                    let io = key.io();
                    io.finish_connect(ev);
                }

                if ready_ops.has_read() || ready_ops.has_accept() {
                    let io = key.io();
                    io.read(ev);
                }
                if ready_ops.has_write() {
                    let io = key.io();
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
                    ops.apply(Ops::ERROR);
                    let resource = key.resource();
                    self.selector.register(key, ops);
                    self.events
                        .unbounded_send(Trigger::State(events::StateEvent::Connected(
                            resource, addr,
                        ))).expect("Dropped unbounded events receiver");
                }
                RWEvent::Read(ReadEvent::Data(bytes)) => {
                    self.events
                        .unbounded_send(Trigger::Read(events::ReadEvent::Data(bytes)))
                        .expect("Dropped unbounded events receiver");
                }
                RWEvent::Registration(RegistrationEvent::Update(resource, ops)) => {
                    println!("Updating registration {:?}", ops);
                    self.selector.update_registration(resource, ops);
                }
                RWEvent::State(StateEvent::ConnectedPeer(resource, addr)) => {
                    self.events
                        .unbounded_send(Trigger::State(events::StateEvent::Connected(
                            resource, addr,
                        ))).expect("Dropped unbounded events receiver");
                }
                other => {
                    println!("Unhandled event {:?}", other);
                }
            }
        }
    }

    fn run_io_tasks(&mut self) {
        loop {
            match self.io_tasks.try_recv() {
                Ok(task) => self.handle_task(task),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    println!("Disconnected I/O task channel!");
                    break;
                }
            }
        }
    }

    fn handle_task(&mut self, task: Work<'static, S, K>) {
        task.call_box(&mut self.selector, self.events.clone());
    }
}

#[derive(Debug)]
pub enum Trigger<K: SelectorKey> {
    State(events::StateEvent<K>),
    Read(events::ReadEvent),
    Write(events::WriteEvent),
    Error(events::ErrorEvent),
}

#[derive(Debug)]
pub enum IoTask<K: SelectorKey> {
    Connect(SocketAddr),

    Write(K::Resource, Bytes),
    Flush(K::Resource),
    WriteAndFlush(K::Resource, Bytes),
}

pub mod events {
    use bytes::Bytes;
    use selector::SelectorKey;
    use std::net::SocketAddr;

    #[derive(Debug)]
    pub enum StateEvent<K: SelectorKey> {
        Connected(K::Resource, SocketAddr),
        ConnectionError(SocketAddr),
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
