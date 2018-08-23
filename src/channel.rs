use bytes::Bytes;
use selector::SelectorKey;
use std::net::SocketAddr;

pub trait Read<K: SelectorKey> {
    //    type KeyType: SelectorKey;
    fn read(&mut self, collector: &mut Vec<RWEvent<K>>);
}

pub trait Write<K: SelectorKey> {
    fn write(&mut self, collector: &mut Vec<RWEvent<K>>);
    fn flush(&mut self, collector: &mut Vec<RWEvent<K>>);
}

#[derive(Debug)]
pub enum RWEvent<K: SelectorKey> {
    Read(ReadEvent<K>),
}

#[derive(Debug)]
pub enum ReadEvent<K: SelectorKey> {
    NewPeer(K, SocketAddr),
    Error(),
    Data(Bytes),
}

#[derive(Debug)]
pub struct AcceptedPeer<K: SelectorKey> {
    key: K,
    peer: SocketAddr,
}

impl<K: SelectorKey> AcceptedPeer<K> {
    pub fn new(key: K, peer: SocketAddr) -> Self {
        AcceptedPeer { key, peer }
    }
}
