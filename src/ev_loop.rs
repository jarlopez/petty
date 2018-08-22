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
    read_buf: Vec<RWEvent<K>>,
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
            read_buf: Vec::new(),
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
        use channel::Read;

        self.selector
            .on_selected(&mut self.read_buf, |ev, key: &mut K| {
                let ready_ops = key.ready_ops();
                if ready_ops.has_read() || ready_ops.has_accept() {
                    let io = key.io();
                    io.read(ev);
                }
            });
        for ev in self.read_buf.drain(..) {
            match ev {
                RWEvent::Read(ReadEvent::NewPeer(key, _addr)) => {
                    let mut ops = Ops::empty();
                    ops.apply(Ops::READ);
                    ops.apply(Ops::WRITE);
                    ops.apply(Ops::ERROR);
                    self.selector.register(key, ops);
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
    Read(events::ReadEvent),
    Write(events::WriteEvent),
    Error(events::ErrorEvent),
}
mod events {
    use bytes::Bytes;

    #[derive(Debug)]
    pub enum ReadEvent {
        Data(Bytes),
    }
    #[derive(Debug)]
    pub struct WriteEvent;
    #[derive(Debug)]
    pub struct ErrorEvent;
}
