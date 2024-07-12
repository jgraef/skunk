use std::{
    fmt::Debug,
    hash::{
        BuildHasher,
        BuildHasherDefault,
        Hash,
    },
    iter::FusedIterator,
    mem::ManuallyDrop,
    ops::DerefMut,
};

use hashbrown::raw::{
    Bucket,
    InsertSlot,
    RawTable,
};

type AHasher = BuildHasherDefault<ahash::AHasher>;

#[derive(Debug)]
pub struct Builder<H> {
    capacity: usize,
    build_hasher: H,
}

impl Default for Builder<AHasher> {
    fn default() -> Self {
        Self::new()
    }
}

impl Builder<AHasher> {
    pub fn new() -> Self {
        Self {
            capacity: 0,
            build_hasher: Default::default(),
        }
    }
}

impl<H> Builder<H> {
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        self.capacity = capacity;
        self
    }

    pub fn with_hasher<H2>(self, build_hasher: H2) -> Builder<H2> {
        Builder {
            capacity: self.capacity,
            build_hasher,
        }
    }

    pub fn build<K, V>(self) -> OrderedMultiMap<K, V, H> {
        OrderedMultiMap {
            pairs: Pairs {
                inner: Vec::with_capacity(self.capacity),
            },
            buckets: Buckets {
                inner: RawTable::with_capacity(self.capacity),
                build_hasher: self.build_hasher,
            },
        }
    }
}

#[derive(Clone)]
pub struct OrderedMultiMap<K, V, H = AHasher> {
    pairs: Pairs<K, V>,
    buckets: Buckets<H>,
}

impl<K, V> OrderedMultiMap<K, V, AHasher> {
    pub fn new() -> Self {
        Self::builder().build()
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::builder().with_capacity(capacity).build()
    }
}

impl<K, V, H> OrderedMultiMap<K, V, H> {
    pub fn builder() -> Builder<AHasher> {
        Builder::new()
    }

    pub fn clear(&mut self) {
        self.pairs.inner.clear();
        self.buckets.inner.clear();
    }

    pub fn len(&self) -> usize {
        self.pairs.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.inner.is_empty()
    }

    fn fix_links_after_removal(&mut self, removed_index: PairIndex) {
        let fix = |index: &mut PairIndex| {
            assert_ne!(*index, removed_index);
            if *index > removed_index {
                index.0 -= 1;
            }
        };

        unsafe {
            for bucket in self.buckets.inner.iter() {
                let list = bucket.as_mut();
                fix(&mut list.head);
                fix(&mut list.tail);
            }
        }

        for pair in &mut self.pairs.inner {
            pair.next.as_mut().map(fix);
            pair.prev.as_mut().map(fix);
        }
    }

    pub fn entry<'a, Q>(&'a self, key: &Q) -> Entry<'a, K, V, H>
    where
        Q: Hash + Eq + PartialEq<K>,
        K: Hash,
        H: BuildHasher,
    {
        if let Some(bucket) = self.buckets.find(key, |index| &self.pairs.get(index).key) {
            Entry::Occupied(OccupiedEntry { map: self, bucket })
        }
        else {
            Entry::Vacant(VacantEntry { map: self })
        }
    }

    pub fn entry_mut<'a, Q>(&'a mut self, key: &Q) -> EntryMut<'a, K, V, H>
    where
        Q: Hash + Eq + PartialEq<K>,
        K: Hash,
        H: BuildHasher,
    {
        match self
            .buckets
            .find_or_find_insert_slot(key, |index| &self.pairs.get(index).key)
        {
            Ok(bucket) => EntryMut::Occupied(OccupiedEntryMut { map: self, bucket }),
            Err(insert_slot) => {
                EntryMut::Vacant(VacantEntryMut {
                    map: self,
                    insert_slot,
                })
            }
        }
    }

    pub fn contains<Q>(&self, key: &Q) -> bool
    where
        Q: Hash + Eq + PartialEq<K>,
        K: Hash,
        H: BuildHasher,
    {
        match self.entry(key) {
            Entry::Occupied(_) => true,
            Entry::Vacant(_) => false,
        }
    }

    pub fn get_first<Q>(&self, key: &Q) -> Option<&V>
    where
        Q: Hash + Eq + PartialEq<K>,
        K: Hash,
        H: BuildHasher,
    {
        match self.entry(key) {
            Entry::Occupied(occupied) => Some(occupied.first()),
            Entry::Vacant(_) => None,
        }
    }

    pub fn get_first_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
    where
        Q: Hash + Eq + PartialEq<K>,
        K: Hash,
        H: BuildHasher,
    {
        match self.entry_mut(key) {
            EntryMut::Occupied(occupied) => Some(occupied.into_first_mut()),
            EntryMut::Vacant(_) => None,
        }
    }

    pub fn get<Q>(&self, key: &Q) -> EntryIter<K, V>
    where
        Q: Hash + Eq + PartialEq<K>,
        K: Hash,
        H: BuildHasher,
    {
        self.entry(key).iter()
    }

    pub fn insert(&mut self, key: K, value: V)
    where
        K: Hash + Eq,
        H: BuildHasher,
    {
        self.entry_mut(&key).append(key, value);
    }

    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter {
            pairs: self.pairs.inner.iter(),
        }
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, K, V> {
        IterMut {
            pairs: self.pairs.inner.iter_mut(),
        }
    }
}

impl<K, V, H> IntoIterator for OrderedMultiMap<K, V, H> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            pairs: self.pairs.inner.into_iter(),
        }
    }
}

impl<'a, K, V, H> IntoIterator for &'a OrderedMultiMap<K, V, H> {
    type Item = (&'a K, &'a V);
    type IntoIter = Iter<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V, H> IntoIterator for &'a mut OrderedMultiMap<K, V, H> {
    type Item = (&'a K, &'a mut V);
    type IntoIter = IterMut<'a, K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<K, V> FromIterator<(K, V)> for OrderedMultiMap<K, V, AHasher>
where
    K: Hash + Eq,
{
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let iter = iter.into_iter();

        let size_hint = iter.size_hint();
        let size_hint = size_hint.1.unwrap_or(size_hint.0);

        let mut map = OrderedMultiMap::with_capacity(size_hint);
        for (key, value) in iter {
            map.insert(key, value);
        }

        map
    }
}

impl<K, V, H> Extend<(K, V)> for OrderedMultiMap<K, V, H>
where
    K: Hash + Eq,
    H: BuildHasher,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<K: Debug, V: Debug, H> Debug for OrderedMultiMap<K, V, H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self).finish()
    }
}

impl<K: PartialEq, V: PartialEq, H> PartialEq for OrderedMultiMap<K, V, H> {
    fn eq(&self, other: &Self) -> bool {
        self.pairs.inner.len() == other.pairs.inner.len()
            && self
                .pairs
                .inner
                .iter()
                .zip(&other.pairs.inner)
                .all(|(a, b)| a.key == b.key && a.value == b.value)
    }
}

pub enum Entry<'a, K, V, H> {
    Occupied(OccupiedEntry<'a, K, V, H>),
    Vacant(VacantEntry<'a, K, V, H>),
}

impl<'a, K, V, H> Entry<'a, K, V, H> {
    pub fn len(&self) -> usize {
        match self {
            Entry::Occupied(occupied) => occupied.len(),
            Entry::Vacant(_) => 0,
        }
    }

    pub fn iter(&self) -> EntryIter<'a, K, V> {
        match self {
            Entry::Occupied(occupied) => occupied.iter(),
            Entry::Vacant(vacant) => {
                EntryIter {
                    pairs: &vacant.map.pairs,
                    state: Default::default(),
                }
            }
        }
    }
}

pub struct OccupiedEntry<'a, K, V, H> {
    map: &'a OrderedMultiMap<K, V, H>,
    bucket: Bucket<List>,
}

impl<'a, K, V, H> OccupiedEntry<'a, K, V, H> {
    #[inline]
    fn list(&self) -> &List {
        unsafe { self.bucket.as_ref() }
    }

    pub fn key(&self) -> &'a K {
        &self.map.pairs.get(self.list().head).key
    }

    pub fn first(&self) -> &'a V {
        &self.map.pairs.get(self.list().head).value
    }

    pub fn last(&self) -> &'a V {
        &self.map.pairs.get(self.list().tail).value
    }

    pub fn len(&self) -> usize {
        self.list().count
    }

    pub fn iter(&self) -> EntryIter<'a, K, V> {
        EntryIter {
            pairs: &self.map.pairs,
            state: self.list().entry_iter_state(),
        }
    }
}

pub struct VacantEntry<'a, K, V, H> {
    map: &'a OrderedMultiMap<K, V, H>,
}

pub enum EntryMut<'a, K, V, H> {
    Occupied(OccupiedEntryMut<'a, K, V, H>),
    Vacant(VacantEntryMut<'a, K, V, H>),
}

impl<'a, K, V, H> EntryMut<'a, K, V, H> {
    pub fn len(&self) -> usize {
        match self {
            EntryMut::Occupied(occupied) => occupied.len(),
            EntryMut::Vacant(_) => 0,
        }
    }

    pub fn or_insert(self, default_key: K, default_value: V) -> OccupiedEntryMut<'a, K, V, H>
    where
        K: Hash + Eq,
        H: BuildHasher,
    {
        self.or_insert_with(move || (default_key, default_value))
    }

    pub fn or_insert_with(self, default: impl FnOnce() -> (K, V)) -> OccupiedEntryMut<'a, K, V, H>
    where
        K: Hash + Eq,
        H: BuildHasher,
    {
        match self {
            EntryMut::Occupied(occupied) => occupied,
            EntryMut::Vacant(vacant) => {
                let (key, value) = default();
                vacant.insert(key, value)
            }
        }
    }

    pub fn append(self, key: K, value: V) -> OccupiedEntryMut<'a, K, V, H>
    where
        K: Hash + Eq,
        H: BuildHasher,
    {
        match self {
            EntryMut::Occupied(mut occupied) => {
                occupied.append(key, value);
                occupied
            }
            EntryMut::Vacant(vacant) => vacant.insert(key, value),
        }
    }

    pub fn drain(self) -> EntryDrain<'a, K, V, H> {
        match self {
            EntryMut::Occupied(occupied) => occupied.drain(),
            EntryMut::Vacant(vacant) => {
                EntryDrain {
                    inner: ManuallyDrop::new(EntryDrainInner {
                        map: vacant.map,
                        state: Default::default(),
                        insert_slot: vacant.insert_slot,
                    }),
                }
            }
        }
    }

    pub fn iter(&self) -> EntryIter<'_, K, V> {
        match self {
            EntryMut::Occupied(occupied) => occupied.iter(),
            EntryMut::Vacant(vacant) => {
                EntryIter {
                    pairs: &vacant.map.pairs,
                    state: Default::default(),
                }
            }
        }
    }

    pub fn iter_mut(&mut self) -> EntryIterMut<'_, K, V> {
        match self {
            EntryMut::Occupied(occupied) => occupied.iter_mut(),
            EntryMut::Vacant(vacant) => {
                EntryIterMut {
                    pairs: &mut vacant.map.pairs,
                    state: Default::default(),
                }
            }
        }
    }
}

pub struct OccupiedEntryMut<'a, K, V, H> {
    map: &'a mut OrderedMultiMap<K, V, H>,
    bucket: Bucket<List>,
}

impl<'a, K, V, H> OccupiedEntryMut<'a, K, V, H> {
    #[inline]
    fn list(&self) -> &List {
        unsafe { self.bucket.as_ref() }
    }

    #[inline]
    fn list_mut(&self) -> &mut List {
        unsafe { self.bucket.as_mut() }
    }

    pub fn key(&self) -> &K {
        &self.map.pairs.get(self.list().head).key
    }

    pub fn first(&self) -> &V {
        &self.map.pairs.get(self.list().head).value
    }

    pub fn first_mut(&mut self) -> &mut V {
        &mut self.map.pairs.get_mut(self.list().head).value
    }

    pub fn into_first_mut(self) -> &'a mut V {
        &mut self.map.pairs.get_mut(self.list().head).value
    }

    pub fn last(&self) -> &V {
        &self.map.pairs.get(self.list().tail).value
    }

    pub fn last_mut(&mut self) -> &mut V {
        &mut self.map.pairs.get_mut(self.list().tail).value
    }

    pub fn into_last_mut(self) -> &'a mut V {
        &mut self.map.pairs.get_mut(self.list().tail).value
    }

    pub fn len(&self) -> usize {
        self.list().count
    }

    pub fn append(&mut self, key: K, value: V) -> &mut V {
        let tail = self.list().tail;

        let index = self.map.pairs.push(Pair {
            key,
            value,
            next: None,
            prev: Some(tail),
        });

        self.map.pairs.get_mut(tail).next = Some(index);
        let list = self.list_mut();
        list.tail = index;
        list.count += 1;

        &mut self.map.pairs.get_mut(index).value
    }

    pub fn iter(&self) -> EntryIter<'_, K, V> {
        EntryIter {
            pairs: &self.map.pairs,
            state: self.list().entry_iter_state(),
        }
    }

    pub fn iter_mut(&mut self) -> EntryIterMut<'_, K, V> {
        let state = self.list().entry_iter_state();
        EntryIterMut {
            pairs: &mut self.map.pairs,
            state,
        }
    }

    pub fn drain(self) -> EntryDrain<'a, K, V, H> {
        let state = self.list().entry_iter_state();
        let (_list, insert_slot) = self.map.buckets.remove(self.bucket);
        EntryDrain {
            inner: ManuallyDrop::new(EntryDrainInner {
                map: self.map,
                state,
                insert_slot,
            }),
        }
    }
}

impl<'a, K, V, H> Extend<(K, V)> for OccupiedEntryMut<'a, K, V, H>
where
    K: Hash + Eq,
{
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (key, value) in iter {
            self.append(key, value);
        }
    }
}

pub struct VacantEntryMut<'a, K, V, H> {
    map: &'a mut OrderedMultiMap<K, V, H>,
    insert_slot: InsertSlot,
}

impl<'a, K, V, H> VacantEntryMut<'a, K, V, H> {
    pub fn insert(self, key: K, value: V) -> OccupiedEntryMut<'a, K, V, H>
    where
        K: Hash,
        H: BuildHasher,
    {
        let index = self.map.pairs.next_index();

        let bucket = self.map.buckets.insert(
            &key,
            self.insert_slot,
            List {
                head: index,
                tail: index,
                count: 1,
            },
        );

        let index2 = self.map.pairs.push(Pair {
            key,
            value,
            next: None,
            prev: None,
        });
        assert_eq!(index, index2);

        OccupiedEntryMut {
            map: self.map,
            bucket,
        }
    }
}

pub struct EntryIter<'a, K, V> {
    pairs: &'a Pairs<K, V>,
    state: EntryIterState,
}

impl<'a, K, V> Iterator for EntryIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        pair_iter_next(
            &mut self.state.forward,
            &mut self.state.reverse,
            &mut self.state.count,
            |index| self.pairs.get(index),
            |pair| pair.next,
        )
        .map(|pair| (&pair.key, &pair.value))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.state.count, Some(self.state.count))
    }
}

impl<'a, K, V> DoubleEndedIterator for EntryIter<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        pair_iter_next(
            &mut self.state.reverse,
            &mut self.state.forward,
            &mut self.state.count,
            |index| self.pairs.get(index),
            |pair| pair.prev,
        )
        .map(|pair| (&pair.key, &pair.value))
    }
}

impl<'a, K, V> FusedIterator for EntryIter<'a, K, V> {}
impl<'a, K, V> ExactSizeIterator for EntryIter<'a, K, V> {}

pub struct EntryIterMut<'a, K, V> {
    pairs: &'a mut Pairs<K, V>,
    state: EntryIterState,
}

impl<'a, K, V> Iterator for EntryIterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        pair_iter_next(
            &mut self.state.forward,
            &mut self.state.reverse,
            &mut self.state.count,
            |index| self.pairs.get_mut(index),
            |pair| pair.next,
        )
        .map(|pair| unsafe {
            // SAFETY: all yielded &mut V are disjunct
            (&*(&pair.key as *const _), &mut *(&mut pair.value as *mut _))
        })
    }
}

impl<'a, K, V> DoubleEndedIterator for EntryIterMut<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        pair_iter_next(
            &mut self.state.reverse,
            &mut self.state.forward,
            &mut self.state.count,
            |index| self.pairs.get_mut(index),
            |pair| pair.next,
        )
        .map(|pair| unsafe {
            // SAFETY: all yielded &mut V are disjunct
            (&*(&pair.key as *const _), &mut *(&mut pair.value as *mut _))
        })
    }
}

impl<'a, K, V> FusedIterator for EntryIterMut<'a, K, V> {}
impl<'a, K, V> ExactSizeIterator for EntryIterMut<'a, K, V> {}

struct EntryDrainInner<'a, K, V, H> {
    map: &'a mut OrderedMultiMap<K, V, H>,
    state: EntryIterState,
    insert_slot: InsertSlot,
}

impl<'a, K, V, H> EntryDrainInner<'a, K, V, H> {
    pub fn drain_remaining(&mut self) {
        while let Some((index, _)) = pair_iter_next(
            &mut self.state.forward,
            &mut self.state.reverse,
            &mut self.state.count,
            |index| {
                let pair = self.map.pairs.remove(index);
                (index, pair)
            },
            |(_, pair)| pair.next,
        ) {
            self.map.fix_links_after_removal(index);
        }
    }
}

pub struct EntryDrain<'a, K, V, H> {
    inner: ManuallyDrop<EntryDrainInner<'a, K, V, H>>,
}

impl<'a, K, V, H> EntryDrain<'a, K, V, H> {
    pub fn into_vacant(mut self) -> VacantEntryMut<'a, K, V, H> {
        // we take out the inner data and then forget self, such that the Drop impl is
        // never run
        let mut inner = unsafe { ManuallyDrop::take(&mut self.inner) };
        std::mem::forget(self);

        inner.drain_remaining();

        VacantEntryMut {
            map: inner.map,
            insert_slot: inner.insert_slot,
        }
    }
}

impl<'a, K, V, H> Drop for EntryDrain<'a, K, V, H> {
    fn drop(&mut self) {
        self.inner.drain_remaining();
        unsafe { ManuallyDrop::drop(&mut self.inner) };
    }
}

impl<'a, K, V, H> Iterator for EntryDrain<'a, K, V, H> {
    type Item = V;

    fn next(&mut self) -> Option<Self::Item> {
        let inner = self.inner.deref_mut();
        let (index, pair) = pair_iter_next(
            &mut inner.state.forward,
            &mut inner.state.reverse,
            &mut inner.state.count,
            |index| {
                let pair = inner.map.pairs.remove(index);
                (index, pair)
            },
            |(_, pair)| pair.next,
        )?;
        inner.map.fix_links_after_removal(index);
        Some(pair.value)
    }
}

impl<'a, K, V, H> DoubleEndedIterator for EntryDrain<'a, K, V, H> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let inner = self.inner.deref_mut();
        let (index, value) = pair_iter_next(
            &mut inner.state.reverse,
            &mut inner.state.forward,
            &mut inner.state.count,
            |index| {
                let pair = inner.map.pairs.remove(index);
                (index, pair)
            },
            |(_, pair)| pair.prev,
        )?;
        inner.map.fix_links_after_removal(index);
        Some(value.value)
    }
}

impl<'a, K, V, H> FusedIterator for EntryDrain<'a, K, V, H> {}
impl<'a, K, V, H> ExactSizeIterator for EntryDrain<'a, K, V, H> {}

#[derive(Debug, Default)]
struct EntryIterState {
    forward: EntryIterStateHalf,
    reverse: EntryIterStateHalf,
    count: usize,
}

#[derive(Debug, Default)]
struct EntryIterStateHalf {
    next: Option<PairIndex>,
    prev: Option<PairIndex>,
}

fn pair_iter_next<P>(
    forward: &mut EntryIterStateHalf,
    reverse: &mut EntryIterStateHalf,
    count: &mut usize,
    get_pair: impl FnOnce(PairIndex) -> P,
    get_next: impl FnOnce(&P) -> Option<PairIndex>,
) -> Option<P> {
    let next = forward.next?;
    if Some(next) == reverse.prev {
        forward.next = None;
        reverse.next = None;
        return None;
    }
    forward.prev = Some(next);
    let pair = get_pair(next);
    forward.next = get_next(&pair);
    *count -= 1;
    Some(pair)
}

pub struct Iter<'a, K, V> {
    pairs: std::slice::Iter<'a, Pair<K, V>>,
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let pair = self.pairs.next()?;
        Some((&pair.key, &pair.value))
    }
}

impl<'a, K, V> DoubleEndedIterator for Iter<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let pair = self.pairs.next_back()?;
        Some((&pair.key, &pair.value))
    }
}

impl<'a, K, V> FusedIterator for Iter<'a, K, V> {}
impl<'a, K, V> ExactSizeIterator for Iter<'a, K, V> {}

pub struct IterMut<'a, K, V> {
    pairs: std::slice::IterMut<'a, Pair<K, V>>,
}

impl<'a, K, V> Iterator for IterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        let pair = self.pairs.next()?;
        Some((&pair.key, &mut pair.value))
    }
}

impl<'a, K, V> DoubleEndedIterator for IterMut<'a, K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let pair = self.pairs.next_back()?;
        Some((&pair.key, &mut pair.value))
    }
}

impl<'a, K, V> FusedIterator for IterMut<'a, K, V> {}
impl<'a, K, V> ExactSizeIterator for IterMut<'a, K, V> {}

pub struct IntoIter<K, V> {
    pairs: std::vec::IntoIter<Pair<K, V>>,
}

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (K, V);

    fn next(&mut self) -> Option<Self::Item> {
        let pair = self.pairs.next()?;
        Some((pair.key, pair.value))
    }
}

impl<K, V> DoubleEndedIterator for IntoIter<K, V> {
    fn next_back(&mut self) -> Option<Self::Item> {
        let pair = self.pairs.next_back()?;
        Some((pair.key, pair.value))
    }
}

impl<K, V> FusedIterator for IntoIter<K, V> {}
impl<K, V> ExactSizeIterator for IntoIter<K, V> {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PairIndex(usize);

#[derive(Clone)]
struct Pair<K, V> {
    key: K,
    value: V,
    next: Option<PairIndex>,
    prev: Option<PairIndex>,
}

#[derive(Clone, Copy)]
struct List {
    head: PairIndex,
    tail: PairIndex,
    count: usize,
}

impl List {
    pub fn entry_iter_state(&self) -> EntryIterState {
        EntryIterState {
            forward: EntryIterStateHalf {
                next: Some(self.head),
                prev: None,
            },
            reverse: EntryIterStateHalf {
                next: Some(self.tail),
                prev: None,
            },
            count: self.count,
        }
    }
}

#[derive(Clone)]
struct Pairs<K, V> {
    inner: Vec<Pair<K, V>>,
}

impl<K, V> Pairs<K, V> {
    pub fn get(&self, index: PairIndex) -> &Pair<K, V> {
        self.inner.get(index.0).expect("invalid pair index")
    }

    pub fn get_mut(&mut self, index: PairIndex) -> &mut Pair<K, V> {
        self.inner.get_mut(index.0).expect("invalid pair index")
    }

    pub fn next_index(&mut self) -> PairIndex {
        PairIndex(self.inner.len())
    }

    pub fn push(&mut self, pair: Pair<K, V>) -> PairIndex {
        let index = self.next_index();
        self.inner.push(pair);
        index
    }

    pub fn remove(&mut self, index: PairIndex) -> Pair<K, V> {
        self.inner.remove(index.0)
    }
}

#[derive(Clone)]
struct Buckets<H> {
    inner: RawTable<List>,
    build_hasher: H,
}

impl<H: BuildHasher> Buckets<H> {
    pub fn find_or_find_insert_slot<'k, Q: Hash + Eq + PartialEq<K>, K: Hash + 'k>(
        &mut self,
        key: &Q,
        get_key: impl Fn(PairIndex) -> &'k K + 'k,
    ) -> Result<Bucket<List>, InsertSlot> {
        let hash = self.build_hasher.hash_one(key);
        self.inner.find_or_find_insert_slot(
            hash,
            |x| key == get_key(x.head),
            |x| self.build_hasher.hash_one(get_key(x.head)),
        )
    }

    pub fn find<'k, Q: Hash + Eq + PartialEq<K>, K: 'k>(
        &self,
        key: &Q,
        get_key: impl Fn(PairIndex) -> &'k K + 'k,
    ) -> Option<Bucket<List>> {
        let hash = self.build_hasher.hash_one(key);
        self.inner.find(hash, |x| key == get_key(x.head))
    }

    pub fn insert<K: Hash>(&mut self, key: &K, slot: InsertSlot, value: List) -> Bucket<List> {
        let hash = self.build_hasher.hash_one(key);
        unsafe { self.inner.insert_in_slot(hash, slot, value) }
    }
}

impl<H> Buckets<H> {
    pub fn remove(&mut self, bucket: Bucket<List>) -> (List, InsertSlot) {
        unsafe { self.inner.remove(bucket) }
    }
}

#[cfg(test)]
mod tests {
    use super::OrderedMultiMap;

    #[test]
    fn it_inserts_and_gets_values() {
        let mut map = OrderedMultiMap::new();

        map.insert("Hello", "World");
        map.insert("foo", "bar");
        assert_eq!(map.get_first(&"Hello"), Some(&"World"));
        assert_eq!(map.get_first(&"foo"), Some(&"bar"));
    }

    #[test]
    fn it_inserts_and_contains_values() {
        let mut map = OrderedMultiMap::new();

        map.insert("Hello", "World");
        assert!(map.contains(&"Hello"));
    }

    #[test]
    fn it_inserts_and_iters() {
        let mut map = OrderedMultiMap::new();

        map.insert("Hello", "World");
        map.insert("foo", "bar");
        assert_eq!(
            map.iter().collect::<Vec<_>>(),
            vec![(&"Hello", &"World"), (&"foo", &"bar")]
        );
    }

    #[test]
    fn it_inserts_and_gets_values_with_same_key() {
        let mut map = OrderedMultiMap::new();

        map.insert("Hello", "World");
        map.insert("Hello", "Rust");
        map.insert("not", "you");
        assert_eq!(
            map.get(&"Hello").collect::<Vec<_>>(),
            vec![(&"Hello", &"World"), (&"Hello", &"Rust")]
        );
    }
}
