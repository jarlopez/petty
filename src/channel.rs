use ev_loop::SelectorEventLoop;
use selector::Selector;
use selector::SelectorKey;

pub trait Read {
    fn read(&mut self);
}
pub trait Write {
    fn write(&mut self);
    fn flush(&mut self);
}
