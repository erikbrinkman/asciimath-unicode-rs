//! complex iterators

use std::iter::FusedIterator;
use std::vec;

#[derive(Debug, Clone)]
pub(crate) struct Interleave<I> {
    queue: vec::IntoIter<I>,
    iter: I,
    sep: char,
}

impl<I> Interleave<I> {
    pub fn new(iters: Vec<I>, sep: char) -> Self {
        let mut queue = iters.into_iter();
        Interleave {
            iter: queue.next().unwrap(),
            queue,
            sep,
        }
    }
}

impl<I> Iterator for Interleave<I>
where
    I: Iterator<Item = char>,
{
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(res) = self.iter.next() {
            Some(res)
        } else if let Some(next) = self.queue.next() {
            self.iter = next;
            Some(self.sep)
        } else {
            None
        }
    }
}

impl<I> FusedIterator for Interleave<I> where I: Iterator<Item = char> {}

#[derive(Debug, Clone)]
pub(crate) struct Modified<I> {
    iter: I,
    modif: char,
    tog: bool,
}

impl<I> Modified<I> {
    pub fn new(iter: I, modif: char) -> Self {
        Modified {
            iter,
            modif,
            tog: false,
        }
    }
}

impl<I: Iterator<Item = char>> Iterator for Modified<I> {
    type Item = char;

    fn next(&mut self) -> Option<Self::Item> {
        if self.tog {
            self.tog = false;
            Some(self.modif)
        } else if let Some(res) = self.iter.next() {
            self.tog = true;
            Some(res)
        } else {
            None
        }
    }
}

impl<I: Iterator<Item = char>> FusedIterator for Modified<I> {}
