extern crate bytes;
extern crate petty;
extern crate udt;

use bytes::Bytes;
use petty::ev_loop::events;
use petty::ev_loop::events::StateEvent;

fn client() {
    use std;
    use std::net::{SocketAddr, SocketAddrV4};
    use std::str::FromStr;
    use udt::*;

    use petty::ev_loop::SelectorEventLoop;
    use petty::ev_loop::Trigger;
    use petty::ops::Ops;
    use petty::transport::udt::ChannelKind;
    use petty::transport::udt::UdtChannel;
    use petty::transport::udt::UdtKey;
    use petty::transport::udt::UdtSelector;
    use udt::UdtSocket;

    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel();

    let _handle = thread::spawn(move || {
        let selector = UdtSelector::new().expect("internal UDT err on creation");
        let (mut event_loop, tasks, events) = SelectorEventLoop::new(selector);

        tx.send((events, tasks))
            .expect("Unable to return events receiver to listener.");
        println!("spawning event loop.run");

        event_loop.run();
    });
    let (events, tasks) = rx.recv().unwrap();

    tasks
        .send(Box::new(
            |sys: &mut UdtSelector, events: mpsc::Sender<Trigger<UdtKey>>| {
                use petty::selector::Selector;
                let localhost = std::net::Ipv4Addr::from_str("127.0.0.1").unwrap();
                let target = SocketAddrV4::new(localhost, 8080);
                let remote = SocketAddr::V4(target);
                let sock = UdtSocket::new(SocketFamily::AFInet, SocketType::Stream).unwrap();
                let ch = UdtChannel::new(sock, ChannelKind::Connector { remote });
                {
                    let key = UdtKey::new(ch.clone());
                    sys.register(key, Ops::with_error());
                }
                let key = UdtKey::new(ch);

                let res = sock.connect(remote);
                println!("res: {:?}", res);
                if let Err(_) = res {
                    events
                        .send(Trigger::State(events::StateEvent::ConnectionError(
                            remote.clone(),
                        ))).expect("Err delivering ConnError");
                    // TODO handle cleanup of ch
                }
                if ch.is_connected() {
                    println!("Quickly connected, yaa");
                    use petty::selector::SelectorKey;
                    events
                        .send(Trigger::State(events::StateEvent::Connected(
                            key.resource(),
                            remote.clone(),
                        ))).expect("Err delivering Connected event");
                } else {
                    println!("not connected, yaa");
                    let mut ops = Ops::with_error();
                    ops.apply(Ops::CONNECT);
                    sys.register(key, ops);
                }
            },
        )).expect("Error sending Connect I/O tasks");

    loop {
        let ev: Trigger<UdtKey> = events.recv().unwrap();
        match ev {
            Trigger::Read(_) => {
                println!("received READ event");
            }
            Trigger::Write(_) => {
                println!("received WRITE event");
            }
            Trigger::Error(_) => {
                println!("received ERROR event");
            }
            Trigger::State(state) => {
                println!("received STATE event {:?}", state);

                match state {
                    StateEvent::ConnectionError(addr) => {
                        println!("Received conn error on {:?}", addr);
                    }
                    StateEvent::Connected(resource, peer) => {
                        println!("connected to {:?}", peer);
                        let mut count = 0;
                        loop {
                            count += 1;
                            tasks
                                .send(Box::new(move |sys: &mut UdtSelector, _| {
                                    let key = sys.registered.get_mut(&resource).unwrap();
                                    use petty::channel::ChWrite;
                                    let payload = format!("msg {:?}", count);
                                    key.ch.io.write(&Bytes::from(payload), &mut vec![]);
                                })).expect("Err dispatching Write task to event loop");
                        }
                    }
                    StateEvent::Disconnected(_) => {}
                }
            }
        }
    }
}

fn main() {
    client();
}
