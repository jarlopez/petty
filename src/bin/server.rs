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

    use std::sync::mpsc;
    use std::thread;

    let (tx, rx) = mpsc::channel();

    let _handle = thread::spawn(move || {
        let selector = UdtSelector::new().expect("internal UDT err on creation");
        let (mut event_loop, tasks, events) = SelectorEventLoop::new(selector);

        tx.send((events, tasks))
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

        let ops = Ops::with_accept();
        event_loop.register(key, ops);
        event_loop.run();
    });
    let (events, _tasks) = rx.recv().unwrap();
    loop {
        let ev: Trigger<UdtKey> = events.recv().unwrap();
        match ev {
            Trigger::Read(data) => {
                //                println!("received READ event: {:?}", data);
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
    }
}

fn main() {
    server();
}
