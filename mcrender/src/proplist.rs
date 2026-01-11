use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

use bytes::BytesMut;

struct BytesPool {
    // Heap-allocated buffer for new data
    unallocated: BytesMut,
    // How many bytes have been allocated to buffer slices
    allocated: usize,
    // How many bytes of the buffer slices are in use
    used: usize,
    // Buffer slices that can be re-used, sorted by capacity
    free: Vec<BytesMut>,
}

impl BytesPool {
    fn with_capacity(cap: usize) -> Self {
        Self {
            unallocated: BytesMut::with_capacity(cap),
            allocated: 0,
            used: 0,
            free: Vec::new(),
        }
    }

    /// Store `data` into the buffer, either by re-using a free buffer slice or creating a new one.
    fn store(&mut self, data: &[u8]) -> BytesMut {
        // Try to find the first free buffer slice that's large enough to hold the data
        let maybe_i = self
            .free
            .iter()
            .enumerate()
            .find_map(|(i, free)| (free.capacity() >= data.len()).then_some(i));
        if let Some(i) = maybe_i {
            // If we can reuse a buffer, take it, fill it and remove it
            let mut buf = self.free.remove(i);
            buf.extend_from_slice(data);
            self.used += data.len();
            buf
        } else {
            // Otherwise, allocate the data into a new buffer
            self.append(data)
        }
    }

    /// Store `data` by allocating a new slice of the buffer.
    fn append(&mut self, data: &[u8]) -> BytesMut {
        self.unallocated.extend_from_slice(data);
        self.allocated += data.len();
        self.used += data.len();
        self.unallocated.split_to(data.len())
    }

    /// Store `data` in `existing`. If `existing` is too small, it will be added to the free-list
    /// and a new buffer slice will be allocated in its place.
    fn reuse_or_store(&mut self, data: &[u8], existing: &mut BytesMut) {
        if existing.capacity() >= data.len() {
            // If the existing buffer is large enough, reuse it
            self.used -= existing.len();
            existing.truncate(0);
            existing.extend_from_slice(data);
            self.used += data.len();
        } else {
            // Otherwise, allocate to a new buffer ...
            let mut buf = self.store(data);
            // ... return the new buffer in-place ...
            std::mem::swap(&mut buf, existing);
            // ... and keep the old buffer for later re-use
            self.free(buf);
        }
    }

    /// Add `buf` to the free list.
    fn free(&mut self, mut buf: BytesMut) {
        self.used -= buf.len();
        buf.truncate(0);
        // Keep free-list ordered by capacity
        match self
            .free
            .binary_search_by_key(&buf.capacity(), |free| free.capacity())
        {
            Ok(i) => self.free.insert(i, buf),
            Err(i) => self.free.insert(i, buf),
        }
    }

    /// Add each of `bufs` to the free list. Only sorts the free-list once, compared to repeated
    /// calls to `free()`.
    fn free_many(&mut self, bufs: impl IntoIterator<Item = BytesMut>) {
        self.free.extend(bufs.into_iter().map(|mut buf| {
            self.used -= buf.len();
            buf.truncate(0);
            buf
        }));
        self.free.sort_by_key(|buf| buf.capacity());
    }

    /// Try to reclaim the entire buffer as unallocated space, on the assumption that all references
    /// to the buffer have been dropped.
    fn try_reclaim(&mut self) -> bool {
        self.free.truncate(0);
        let expected_capacity = self.allocated + self.unallocated.capacity();
        if self.unallocated.try_reclaim(expected_capacity) {
            self.allocated = 0;
            self.used = 0;
            true
        } else {
            false
        }
    }
}

#[derive(Eq, PartialEq)]
struct Item {
    key: BytesMut,
    value: BytesMut,
}

impl Item {
    fn key(&self) -> &str {
        // Safety is provided by the caller only ever storing &str into the buffer.
        unsafe { str::from_utf8_unchecked(&self.key) }
    }

    fn value(&self) -> &str {
        // Safety is provided by the caller only ever storing &str into the buffer.
        unsafe { str::from_utf8_unchecked(&self.value) }
    }
}

/// An ordered string map that minimizes memory allocations, compared to `BTreeMap<String, String>`.
///
/// Allows updates and removals, but optimized for append-only operations.
///
/// Internally uses a single `BytesMut` buffer to store keys and values, with a free-list to re-use
/// buffer slices.
pub struct PropList {
    pool: BytesPool,
    items: Vec<Item>,
}

impl PropList {
    pub fn new() -> Self {
        Self::with_capacity(64, 8)
    }

    pub fn with_capacity(data_cap: usize, item_cap: usize) -> Self {
        Self {
            pool: BytesPool::with_capacity(data_cap),
            items: Vec::with_capacity(item_cap),
        }
    }

    pub fn from_iter<I, K, V>(iter: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut new = Self::new();
        for (key, value) in iter.into_iter() {
            new.insert(key.as_ref(), value.as_ref());
        }
        new
    }

    /// Create new `PropList` from an iterator known to have no duplicates, with a known required
    /// data size, and optionally already sorted (e.g. from `BTreeMap::items()`).
    fn from_iter_unchecked<I, K, V>(iter: I, data_cap: usize, item_cap: usize, sorted: bool) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut new = Self::with_capacity(data_cap, item_cap);
        for (key, value) in iter.into_iter() {
            let key = new.pool.append(key.as_ref().as_bytes());
            let value = new.pool.append(value.as_ref().as_bytes());
            new.items.push(Item { key, value });
        }
        if !sorted {
            new.items.sort_by(|item, other| item.key().cmp(other.key()));
        }
        new
    }

    /// Checks if the `PropList` contains `key` with `value`. Convenience method.
    pub fn contains(&self, key: &str, value: &str) -> bool {
        self.get_item(key)
            .map(|(_i, item)| item.value() == value)
            .unwrap_or(false)
    }

    // Standard HashMap-like methods

    fn clear(&mut self) {
        self.pool
            .free_many(self.items.drain(..).flat_map(|item| [item.key, item.value]));
        assert!(self.pool.try_reclaim());
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.get_item(key).is_some()
    }

    // pub fn drain(...)
    // pub fn entry(&mut self, key: &str) -> ...
    // pub fn extract_if(...)

    pub fn get(&self, key: &str) -> Option<&str> {
        self.get_item(key).map(|(_i, item)| item.value())
    }

    pub fn get_key_value(&self, key: &str) -> Option<(&str, &str)> {
        self.get_item(key)
            .map(|(_i, item)| (item.key(), item.value()))
    }

    // pub fn get_mut(...)

    pub fn insert(&mut self, key: &str, value: &str) -> &mut Self {
        match self.get_item_index(key) {
            Ok(i) => {
                // Existing item, update it, in-place if possible
                let mut item = &mut self.items[i];
                self.pool.reuse_or_store(value.as_bytes(), &mut item.value);
            }
            Err(i) => {
                // No existing item, insert a new one, in the correct position
                self.items.insert(
                    i,
                    Item {
                        key: self.pool.store(key.as_bytes()),
                        value: self.pool.store(value.as_bytes()),
                    },
                );
            }
        }
        self
    }

    // pub fn into_keys(...)
    // pub fn into_values(...)

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.items.iter().map(|item| (item.key(), item.value()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|item| item.key())
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn remove(&mut self, key: &str) -> bool {
        match self.get_item_index(key) {
            Ok(i) => {
                let Item { key, value } = self.items.remove(i);
                self.pool.free(key);
                self.pool.free(value);
                true
            }
            Err(_) => false,
        }
    }

    // pub fn remove_entry(...)
    // pub fn retain(...)

    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|item| item.value())
    }

    // pub fn values_mut(...)

    // Helpers

    #[inline]
    fn get_item_index(&self, key: &str) -> Result<usize, usize> {
        self.items.binary_search_by(|item| item.key().cmp(key))
    }

    fn get_item(&self, key: &str) -> Option<(usize, &Item)> {
        self.get_item_index(key).ok().map(|i| (i, &self.items[i]))
    }

    fn get_item_mut(&mut self, key: &str) -> Option<(usize, &mut Item)> {
        self.get_item_index(key)
            .ok()
            .map(|i| (i, &mut self.items[i]))
    }
}

impl Clone for PropList {
    fn clone(&self) -> Self {
        let mut pool = BytesPool::with_capacity(self.pool.used);
        let mut items = Vec::with_capacity(self.items.len());
        for item in self.items.iter() {
            let key = pool.append(item.key.as_ref());
            let value = pool.append(item.value.as_ref());
            items.push(Item { key, value });
        }
        Self { pool, items }
    }
}

impl<K: AsRef<str>, V: AsRef<str>> From<&HashMap<K, V>> for PropList {
    fn from(other: &HashMap<K, V>) -> Self {
        let data_cap: usize = other
            .iter()
            .map(|(k, v)| k.as_ref().len() + v.as_ref().len())
            .sum();
        let item_cap = other.len();
        Self::from_iter_unchecked(other.iter(), data_cap, item_cap, false)
    }
}

impl From<&BTreeMap<String, String>> for PropList {
    fn from(other: &BTreeMap<String, String>) -> Self {
        let data_cap: usize = other.iter().map(|(k, v)| k.len() + v.len()).sum();
        let item_cap = other.len();
        Self::from_iter_unchecked(other.iter(), data_cap, item_cap, true)
    }
}

impl<'a> From<&BTreeMap<&'a str, &'a str>> for PropList {
    fn from(other: &BTreeMap<&'a str, &'a str>) -> Self {
        let data_cap: usize = other.iter().map(|(k, v)| k.len() + v.len()).sum();
        let item_cap = other.len();
        Self::from_iter_unchecked(other.iter(), data_cap, item_cap, true)
    }
}

impl<'a> From<&BTreeMap<Cow<'a, str>, Cow<'a, str>>> for PropList {
    fn from(other: &BTreeMap<Cow<'a, str>, Cow<'a, str>>) -> Self {
        let data_cap: usize = other.iter().map(|(k, v)| k.len() + v.len()).sum();
        let item_cap = other.len();
        Self::from_iter_unchecked(other.iter(), data_cap, item_cap, true)
    }
}

impl<K: AsRef<str>, V: AsRef<str>> FromIterator<(K, V)> for PropList {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self::from_iter(iter)
    }
}

impl PartialEq for PropList {
    fn eq(&self, other: &Self) -> bool {
        self.items.eq(&other.items)
    }
}

impl Eq for PropList {}

impl Ord for PropList {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl Hash for PropList {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for (k, v) in self.iter() {
            k.hash(state);
            v.hash(state);
        }
    }
}

impl PartialOrd for PropList {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Debug for PropList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl std::fmt::Display for PropList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.items.is_empty() {
            f.write_str("<empty>")
        } else {
            let item = &self.items[0];
            f.write_str(item.key())?;
            f.write_str("=")?;
            f.write_str(item.value())?;
            for item in &self.items[1..] {
                f.write_str(";")?;
                f.write_str(item.key())?;
                f.write_str("=")?;
                f.write_str(item.value())?;
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytespool() {
        let mut pool = BytesPool::with_capacity(64);
        assert_eq!(pool.unallocated.capacity(), 64);

        // Simple allocations
        let mut a = pool.store(b"hello");
        assert_eq!(a.as_ref(), b"hello");
        assert_eq!(pool.allocated, 5);
        assert_eq!(pool.used, 5);
        assert_eq!(pool.unallocated.capacity(), 59);
        let b = pool.store(b" world");
        assert_eq!(b.as_ref(), b" world");
        assert_eq!(pool.allocated, 11);
        assert_eq!(pool.used, 11);
        assert_eq!(pool.unallocated.capacity(), 53);

        // Buffer slice should be reused when new data fits
        pool.reuse_or_store(b"'sup", &mut a);
        assert_eq!(a.as_ref(), b"'sup", "incorrect data after reuse");
        assert_eq!(b.as_ref(), b" world");
        assert_eq!(pool.allocated, 11);
        assert_eq!(pool.used, 10);
        assert_eq!(pool.unallocated.capacity(), 53);

        // Buffer slice should be swapped when new data doesn't fit
        pool.reuse_or_store(b"goodbye", &mut a);
        assert_eq!(a.as_ref(), b"goodbye", "incorrect data after reuse");
        assert_eq!(b.as_ref(), b" world", "reuse overran into next buffer");
        assert_eq!(pool.allocated, 18);
        assert_eq!(pool.used, 13);
        assert_eq!(pool.unallocated.capacity(), 46);

        // New allocation shouldn't use the free'd buffer if data won't fit
        assert_eq!(pool.free.len(), 1);
        let c = pool.store(b"foobarbaz");
        assert_eq!(pool.free.len(), 1);
        assert_eq!(pool.allocated, 27);
        assert_eq!(pool.used, 22);
        assert_eq!(pool.unallocated.capacity(), 37);

        // New allocation should use the free'd buffer if it will fit
        assert_eq!(pool.free.len(), 1);
        let d = pool.store(b"foo");
        assert_eq!(pool.free.len(), 0);
        assert_eq!(pool.allocated, 27);
        assert_eq!(pool.used, 25);
        assert_eq!(pool.unallocated.capacity(), 37);

        // Smallest suitable free buffer should be used
        pool.free(d);
        pool.free(c);
        pool.free(b);
        pool.free(a);
        assert_eq!(
            pool.free
                .iter()
                .map(|buf| buf.capacity())
                .collect::<Vec<usize>>(),
            vec![5, 6, 7, 9]
        );
        assert_eq!(pool.used, 0);
        let e = pool.store(b"foobar");
        assert_eq!(
            pool.free
                .iter()
                .map(|buf| buf.capacity())
                .collect::<Vec<usize>>(),
            vec![5, 7, 9]
        );
        assert_eq!(pool.allocated, 27);
        assert_eq!(pool.used, 6);
        assert_eq!(pool.unallocated.capacity(), 37);

        // Should be able to recover entire buffer by freeing all buffers
        pool.free(e);
        assert_eq!(pool.used, 0);
        assert!(pool.try_reclaim(), "unable to reclaim buffer");
        assert_eq!(pool.allocated, 0);
        assert_eq!(pool.used, 0);
        assert_eq!(pool.unallocated.capacity(), 64);
    }

    #[test]
    fn test_proplist_crud() {
        let mut a = PropList::new();
        assert_eq!(a.pool.unallocated.capacity(), 64);
        assert_eq!(a.items.capacity(), 8);
        assert!(a.is_empty());
        assert_eq!(a.len(), 0);
        assert_eq!(format!("{a:?}"), "{}");
        assert_eq!(format!("{a}"), "<empty>");

        // Insertion and retrieval
        a.insert("b", "123");
        a.insert("a", "456");
        assert!(!a.is_empty());
        assert_eq!(a.len(), 2);
        assert!(a.contains_key("a"));
        assert!(!a.contains_key("c"));
        assert!(a.contains("a", "456"));
        assert!(!a.contains("a", "123"));
        assert_eq!(a.get("b"), Some("123"));
        assert_eq!(a.get("c"), None);
        assert_eq!(Vec::from_iter(a.keys()), vec!["a", "b"]);
        assert_eq!(Vec::from_iter(a.values()), vec!["456", "123"]);
        assert_eq!(Vec::from_iter(a.iter()), vec![("a", "456"), ("b", "123")]);
        assert_eq!(format!("{a:?}"), "{\"a\": \"456\", \"b\": \"123\"}");
        assert_eq!(format!("{a}"), "a=456;b=123");

        // Update
        a.insert("a", "hello");
        assert_eq!(format!("{a:?}"), "{\"a\": \"hello\", \"b\": \"123\"}");
        assert_eq!(format!("{a}"), "a=hello;b=123");

        // Remove
        assert!(a.remove("a"));
        assert!(!a.remove("a")); // Only returns true if there was an item to remove
        assert_eq!(format!("{a:?}"), "{\"b\": \"123\"}");
        assert_eq!(format!("{a}"), "b=123");
    }

    #[test]
    fn test_proplist_traits() {
        let mut a = PropList::new();
        a.insert("foo", "hello");
        a.insert("bar", " world");
        let b = a.clone();
        assert_eq!(format!("{a}"), "bar= world;foo=hello");
        assert_eq!(format!("{b}"), "bar= world;foo=hello");

        // Ensure clones are independent of each other
        let mut c = b.clone();
        c.remove("foo");
        c.insert("a", "123");
        c.insert("b", "456");
        assert_eq!(format!("{b}"), "bar= world;foo=hello");
        assert_eq!(format!("{c}"), "a=123;b=456;bar= world");

        // Equality
        assert_eq!(a, b);
        assert_ne!(b, c);

        // Ordering
        assert!(PropList::new() < *PropList::new().insert("a", "a"));
        assert!(*PropList::new().insert("a", "a") > PropList::new());
        assert!(*PropList::new().insert("a", "a") < *PropList::new().insert("a", "b"));
        assert!(
            *PropList::new().insert("a", "a").insert("b", "a")
                < *PropList::new().insert("a", "a").insert("c", "a")
        );
    }
}
