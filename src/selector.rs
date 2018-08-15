use channel;
use std::collections::HashSet;
use std::hash::Hash;
use std::io::Read;
use Ops;

pub trait SelectorKey: Eq + Hash {
    type Io: channel::Read + channel::Write;
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
    fn selected_keys(&self) -> &HashSet<K::Resource>;
    fn on_selected<F>(&mut self, f: F)
    where
        F: Fn(&mut K) -> ();
}
