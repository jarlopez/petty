extern crate futures;
extern crate petty;
extern crate udt;

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

    use futures;
    use futures::Future;
    use futures::Stream;
    use std::thread;

    let (tx, rx) = futures::oneshot();

    let _handle = thread::spawn(move || {
        let selector = UdtSelector::new().expect("internal UDT err on creation");
        let (mut event_loop, events) = SelectorEventLoop::new(selector);

        tx.send(events)
            .expect("Unable to return events receiver to listener.");
        println!("spawning event loop.run");

        let localhost = std::net::Ipv4Addr::from_str("127.0.0.1").unwrap();

        let sock = UdtSocket::new(SocketFamily::AFInet, SocketType::Stream).unwrap();
        let target = SocketAddrV4::new(localhost, 8080);
        sock.connect(SocketAddr::V4(target));

        let ch = UdtChannel::new(sock, ChannelKind::Connector);
        let key = UdtKey::new(ch);

        let mut ops = Ops::empty();
        ops.apply(Ops::WRITE);
        ops.apply(Ops::CONNECT);
        event_loop.register(key, ops);
        event_loop.run();
    });
    let events = rx.wait().unwrap();
    events
        .for_each(|ev: Trigger| {
            // Process event outside of event loop
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
                }
            }
            Ok(())
        }).wait()
        .unwrap();
}

fn main() {
    client();
}
