use futures::sync::mpsc;
use selector::Selector;
use selector::SelectorKey;
use std::marker::PhantomData;

pub struct SelectorEventLoop<S, K> {
    selector: S,
    events: mpsc::UnboundedSender<Trigger>,
    key: PhantomData<K>,
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
        };
        (event_loop, rx)
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
        self.selector.on_selected(|key| {
            let ready_ops = key.ready_ops();
            if ready_ops.has_read() || ready_ops.has_accept() {
                println!("key has read or accept");
                // get unsafe access, perform read()
                // unsafe.read() delegates down into doReadMEssages, which in ACCEPTOR's case
                // calls UDT's accept
                // doReadMessages then adds new channels to the pipeline :p
                // triggering pipeline.fireChannelRead with each new channel
                let io = key.io();
                io.read();
                // what events are possible from a 'read'?
                // either a series of byte buffers that need forwarding to ... something
                // or a newly connected peer who needs to be registered
            }
        });
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
    #[derive(Debug)]
    pub struct ReadEvent;
    #[derive(Debug)]
    pub struct WriteEvent;
    #[derive(Debug)]
    pub struct ErrorEvent;
}


#[cfg(test)]
mod tests {
    #[test]
    fn socket_setup() {
        use Ops;

        use std;
        use std::net::{SocketAddr, SocketAddrV4};
        use std::str::FromStr;
        use udt::*;

        use super::*;
        use futures::Future;
        use futures::Stream;
        use std::thread;
        use transport::udt::*;

        let localhost = std::net::Ipv4Addr::from_str("127.0.0.1").unwrap();

        let sock = UdtSocket::new(SocketFamily::AFInet, SocketType::Stream).unwrap();
        sock.bind(SocketAddr::V4(SocketAddrV4::new(localhost, 8080)))
            .unwrap();
        let my_addr = sock.getsockname().unwrap();
        println!("Server bound to {:?}", my_addr);
        sock.listen(5).unwrap();

        let ch = UdtChannel::new(sock, ChannelKind::Acceptor);
        let key = UdtSelectorKey::new(ch);
        let mut selector = UdtSelector::new().expect("internal UDT err on creation");
        selector.register(key, Ops::with_read());
        let (mut event_loop, events) = SelectorEventLoop::new(selector);

        let _handle = thread::spawn(move || {
            println!("spawning event loop.run");
            event_loop.run();
        });
        events
            .for_each(|ev| {
                println!("received ev: {:?}", ev);
                Ok(())
            }).wait()
            .unwrap();
    }
}
