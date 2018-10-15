use bytes::Bytes;
use ops::Ops;
use selector::SelectorKey;
use std::net::SocketAddr;

// TODO should really be implemented for whatever's inside SelectorKey's Resource
pub trait ChRead<K: SelectorKey> {
    fn read(&mut self, collector: &mut Vec<RWEvent<K>>);
}

pub trait ChWrite<K: SelectorKey> {
    fn write(&mut self, data: &Bytes, collector: &mut Vec<RWEvent<K>>);
    fn flush(&mut self, collector: &mut Vec<RWEvent<K>>);
}

pub trait ChExt<K: SelectorKey> {
    fn finish_connect(&mut self, collector: &mut Vec<RWEvent<K>>);
}

#[derive(Debug)]
pub enum RWEvent<K: SelectorKey> {
    Registration(RegistrationEvent<K>),
    Read(ReadEvent<K>),
    State(StateEvent<K>),
    Error(/*TODO*/),
}

#[derive(Debug)]
pub enum ReadEvent<K: SelectorKey> {
    NewPeer(K, SocketAddr),
    Data(Bytes),
}

#[derive(Debug)]
pub enum RegistrationEvent<K: SelectorKey> {
    Update(K::Resource, Ops),
}

#[derive(Debug)]
pub enum StateEvent<K: SelectorKey> {
    ConnectedPeer(K::Resource, SocketAddr),
}
