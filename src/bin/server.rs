extern crate futures;
extern crate petty;
extern crate udt;

fn server() {
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
        sock.bind(SocketAddr::V4(SocketAddrV4::new(localhost, 8080)))
            .unwrap();
        let my_addr = sock.getsockname().unwrap();
        println!("Server bound to {:?}", my_addr);
        sock.listen(5).unwrap();

        let ch = UdtChannel::new(sock, ChannelKind::Acceptor);
        let key = UdtKey::new(ch);

        let mut ops = Ops::empty();
        ops.apply(Ops::READ);
        ops.apply(Ops::ACCEPT);
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
            }
            Ok(())
        }).wait()
        .unwrap();
}

fn main() {
    server();
}
