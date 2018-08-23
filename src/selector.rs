use channel;
use channel::RWEvent;
use ops::Ops;
use std::fmt::Debug;
use std::hash::Hash;

pub trait SelectorKey: Eq + Hash + Debug + Sized {
    type Io: channel::Read<Self> + channel::Write<Self>;
    type Resource: Hash + Eq;

    fn ready_ops(&self) -> Ops;
    fn io(&mut self) -> &mut Self::Io;

    fn apply_read(&mut self) -> bool;
    fn apply_write(&mut self) -> bool;
}

pub trait Selector<K: SelectorKey> {
    const DEFAULT_TIMEOUT_MS: i64;

    fn register(&mut self, key: K, interest: Ops);
    fn select(&mut self, timeout: i64);
    fn on_selected<F>(&mut self, coll: &mut Vec<RWEvent<K>>, f: F)
    where
        F: Fn(&mut Vec<RWEvent<K>>, &mut K) -> ();
}
