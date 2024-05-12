use std::{
    hash::Hash,
    marker::PhantomData,
};

use indexmap::IndexSet;

#[derive(Debug)]
pub struct UniqueArena<T> {
    set: IndexSet<T>,
}

impl<T> Default for UniqueArena<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> UniqueArena<T> {
    pub fn new() -> Self {
        Self {
            set: IndexSet::new(),
        }
    }
}

impl<T: Hash + Eq> UniqueArena<T> {
    pub fn insert(&mut self, value: T) -> Handle<T> {
        let (index, _) = self.set.insert_full(value);
        Handle {
            index,
            _ty: PhantomData,
        }
    }

    pub fn get(&self, handle: Handle<T>) -> Option<&T> {
        self.set.get_index(handle.index)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Handle<T> {
    index: usize,
    _ty: PhantomData<T>,
}
