use bytes::BytesMut;
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};

use std::cmp::{Ordering, max};
use std::hash::Hash;
use std::marker::PhantomData;

struct Pool(Option<BytesMut>);

impl Pool {
    #[inline]
    fn new() -> Self {
        Self(None)
    }

    #[inline]
    fn with_capacity(capacity: usize) -> Self {
        Self(Some(BytesMut::with_capacity(capacity)))
    }

    #[inline]
    fn store<'d, I: IntoIterator<Item = &'d [u8]>>(
        &mut self,
        iter: I,
        size_hint: Option<usize>,
    ) -> BytesMut {
        let unallocated = self.0.get_or_insert_with(|| {
            BytesMut::with_capacity(max(DEFAULT_POOL_SIZE, size_hint.unwrap_or(0)))
        });
        for data in iter.into_iter() {
            unallocated.extend_from_slice(data);
        }
        unallocated.split()
    }
}

enum Item<const N: usize> {
    Inline { buf: [u8; N], key_len: u8, len: u8 },
    Allocated { buf: BytesMut, key_len: u32 },
}

impl<const N: usize> Item<N> {
    /// Create a new `Item` from `key` and `value`, allocating to `pool` if too large to store inline.
    #[inline]
    fn new(key: &str, value: &str, pool: &mut Pool) -> Item<N> {
        Self::try_new_inline(key, value).unwrap_or_else(|| Self::new_allocated(key, value, pool))
    }

    /// Attempt to create a new `Item` from `key` and `value` without allocating.
    #[inline]
    fn try_new_inline(key: &str, value: &str) -> Option<Item<N>> {
        if key.len() + value.len() <= N {
            let key_len = key.len();
            let len = key_len + value.len();
            let mut buf = [0u8; N];
            buf[..key_len].copy_from_slice(key.as_bytes());
            buf[key_len..len].copy_from_slice(value.as_bytes());
            Some(Self::Inline {
                buf,
                key_len: key_len as u8,
                len: len as u8,
            })
        } else {
            None
        }
    }

    /// Create a new `Item` from `key` and `value` by allocating to `pool`.
    #[inline]
    fn new_allocated(key: &str, value: &str, pool: &mut Pool) -> Item<N> {
        let key_len = key.len();
        let len = key_len + value.len();
        let buf = pool.store([key.as_bytes(), value.as_bytes()], Some(len));
        Self::Allocated {
            buf,
            key_len: key_len as u32,
        }
    }

    /// Create a copy of this `Item`, allocating to `pool` if it's too large to store inline.
    #[inline]
    fn clone(&self, pool: &mut Pool) -> Item<N> {
        self.try_clone_inline()
            .unwrap_or_else(|| self.clone_allocated(pool))
    }

    /// Attempt to create a copy of this `Item` without allocating.
    #[inline]
    fn try_clone_inline(&self) -> Option<Item<N>> {
        match self {
            Self::Inline { buf, key_len, len } => {
                // Previously Inline: just clone it
                Some(Self::Inline {
                    buf: buf.clone(),
                    key_len: *key_len,
                    len: *len,
                })
            }
            Self::Allocated { buf, key_len } => {
                // Previously Allocated: copy to inline if small enough
                if buf.len() > N {
                    None
                } else {
                    let mut new_buf = [0u8; N];
                    new_buf[..buf.len()].copy_from_slice(buf);
                    Some(Self::Inline {
                        buf: new_buf,
                        key_len: *key_len as u8,
                        len: buf.len() as u8,
                    })
                }
            }
        }
    }

    /// Create a copy of this `Item` by allocating to `pool`.
    #[inline]
    fn clone_allocated(&self, pool: &mut Pool) -> Item<N> {
        let (split, buf) = self.get_split_and_buffer();
        let buf = pool.store([buf], Some(buf.len()));
        Self::Allocated {
            buf,
            key_len: split as u32,
        }
    }

    /// Get the `(key, value)` strings. This is faster than `.key()` and `.value()` if both are needed.
    #[inline]
    fn key_value(&self) -> (&str, &str) {
        let (split, buf) = self.get_split_and_buffer();
        unsafe {
            // SAFETY: buffer is only ever populated from &str or copied
            (
                str::from_utf8_unchecked(&buf[..split]),
                str::from_utf8_unchecked(&buf[split..]),
            )
        }
    }

    #[inline]
    fn key(&self) -> &str {
        self.key_value().0
    }

    #[inline]
    fn value(&self) -> &str {
        self.key_value().1
    }

    /// Attempt to update the `value` of this `Item` in-place, if the new value will fit in the existing
    /// buffer (whether inline or allocated). Returns `true` if the update was performed, otherwise
    /// no changes will have been made.
    fn try_update(&mut self, value: &str) -> bool {
        match self {
            Self::Inline { buf, key_len, len } => {
                let key_len = *key_len as usize;
                let new_len = key_len + value.len();
                if new_len <= N {
                    buf[key_len..new_len].copy_from_slice(value.as_bytes());
                    *len = new_len as u8;
                    true
                } else {
                    false
                }
            }
            Self::Allocated { buf, key_len } => {
                let key_len = *key_len as usize;
                let new_len = key_len + value.len();
                if new_len <= buf.capacity() {
                    buf.truncate(key_len);
                    buf.extend_from_slice(value.as_bytes());
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Read helper that gets the occupied buffer slice and the split point between key and value.
    #[inline]
    fn get_split_and_buffer(&self) -> (usize, &[u8]) {
        match self {
            Self::Inline { buf, key_len, len } => (*key_len as usize, &buf[..*len as usize]),
            Self::Allocated { buf, key_len } => (*key_len as usize, buf),
        }
    }

    /// How many bytes would need to be allocated to clone this `Item`? Zero if not allocated or if
    /// otherwise small enough to fit inline.
    #[inline]
    fn clone_alloc_bytes_required(&self) -> usize {
        match self {
            Self::Inline { .. } => 0,
            Self::Allocated { buf, .. } => {
                if buf.len() <= N {
                    0
                } else {
                    buf.len()
                }
            }
        }
    }
}

impl<const N: usize> PartialEq for Item<N> {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.get_split_and_buffer()
            .eq(&other.get_split_and_buffer())
    }
}

impl<const N: usize> Eq for Item<N> {}

impl<const N: usize> Hash for Item<N> {
    #[inline]
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let (buf, key_len, len) = match self {
            Self::Inline { buf, key_len, len } => (buf.as_ref(), *key_len as usize, *len as usize),
            Self::Allocated { buf, key_len } => (buf.as_ref(), *key_len as usize, buf.len()),
        };
        // Inline exactly what `impl Hash for str` does via the experimental `Hasher::write_str()`
        state.write(&buf[..key_len]);
        state.write_u8(0xFF);
        state.write(&buf[key_len..len]);
        state.write_u8(0xFF);
    }
}

/// Minimum allocation when an `Item` needs to be allocated for the first time.
const DEFAULT_POOL_SIZE: usize = 64;

/// The maximum number of bytes that can be stored in Item::Inline without making it larger than
/// Item::Allocated, based on reading and experimentation related to enum layouts and BytesMut. It's
/// essentially free to always use at least this much inline capacity.
pub const DEFAULT_INLINE_CAPACITY: usize = 37;

/// Sensible default `PropList` parametrization.
pub type DefaultPropList = PropList<DEFAULT_INLINE_CAPACITY>;

/// An ordered string map that minimizes memory allocations, compared to `BTreeMap<String, String>`.
///
/// Allows updates and removals, but optimized for append-only operations.
pub struct PropList<const N: usize> {
    pool: Pool,
    items: Vec<Item<N>>,
}

impl<const N: usize> PropList<N> {
    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        debug_assert!(N < 256, "Item<N> too big for u8 length");
        Self {
            pool: Pool(None),
            items: Vec::with_capacity(capacity),
        }
    }

    /// Ensure enough space for `additional` items without re-allocating.
    pub fn reserve(&mut self, additional: usize) {
        self.items.reserve(additional);
    }

    /// Checks if the `PropList` contains `key` with `value`. Convenience method.
    pub fn contains(&self, key: &str, value: &str) -> bool {
        self.get_item(key)
            .map(|(_i, item)| item.value() == value)
            .unwrap_or(false)
    }

    // Standard HashMap-like methods

    pub fn clear(&mut self) {
        self.items.clear();
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
        self.get_item(key).map(|(_i, item)| item.key_value())
    }

    // pub fn get_mut(...)

    pub fn insert(&mut self, key: &str, value: &str) -> &mut Self {
        match self.get_item_index(key) {
            Ok(i) => {
                // Existing item, update it, in-place if possible
                let existing = &mut self.items[i];
                if !existing.try_update(value) {
                    // If it wasn't updated, then we need to allocate (because there's no reason to
                    // have previously allocated a buffer smaller than what could be inlined)
                    *existing = Item::new_allocated(key, value, &mut self.pool);
                }
            }
            Err(i) => {
                // No existing item, insert a new one, in the correct position
                let item = Item::new(key, value, &mut self.pool);
                self.items.insert(i, item);
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
        self.items.iter().map(|item| item.key_value())
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
                self.items.remove(i);
                true
            }
            Err(_) => false,
        }
    }

    // pub fn remove_entry(...)

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&str, &str) -> bool,
    {
        self.items.retain(|item| {
            let (k, v) = item.key_value();
            f(k, v)
        });
    }

    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.items.iter().map(|item| item.value())
    }

    // pub fn values_mut(...)

    // Helpers

    #[inline]
    fn get_item_index(&self, key: &str) -> Result<usize, usize> {
        self.items.binary_search_by(|item| item.key().cmp(key))
    }

    #[inline]
    fn get_item(&self, key: &str) -> Option<(usize, &Item<N>)> {
        self.get_item_index(key).ok().map(|i| (i, &self.items[i]))
    }
}

impl<const N: usize> Clone for PropList<N> {
    fn clone(&self) -> Self {
        // Find out how much is needed in Allocated items, because we might be able to fast path
        let alloc_bytes = self
            .items
            .iter()
            .map(|item| item.clone_alloc_bytes_required())
            .sum();

        // Pre-allocate the pool if necessary
        let mut pool = if alloc_bytes > 0 {
            Pool::with_capacity(alloc_bytes)
        } else {
            Pool::new()
        };

        // Clone the items
        let mut items = Vec::with_capacity(self.items.len());
        items.extend(self.items.iter().map(|item| item.clone(&mut pool)));

        Self { pool, items }
    }
}

impl<K: AsRef<str>, V: AsRef<str>, const N: usize> FromIterator<(K, V)> for PropList<N> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(into_iter: I) -> Self {
        let iter = into_iter.into_iter();
        let (lower, upper) = iter.size_hint();
        let mut new = Self::with_capacity(upper.unwrap_or(lower));
        for (key, value) in iter.into_iter() {
            new.insert(key.as_ref(), value.as_ref());
        }
        new
    }
}

impl<const N: usize> PartialEq for PropList<N> {
    fn eq(&self, other: &Self) -> bool {
        self.items.eq(&other.items)
    }
}

impl<const N: usize> Eq for PropList<N> {}

impl<const N: usize> Ord for PropList<N> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.iter().cmp(other.iter())
    }
}

impl<const N: usize> Hash for PropList<N> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        for item in self.items.iter() {
            item.hash(state);
        }
    }
}

impl<const N: usize> PartialOrd for PropList<N> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<const N: usize> std::fmt::Debug for PropList<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<const N: usize> std::fmt::Display for PropList<N> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if self.items.is_empty() {
            f.write_str("<empty>")
        } else {
            let item = &self.items[0];
            let (k, v) = item.key_value();
            f.write_str(k)?;
            f.write_str("=")?;
            f.write_str(v)?;
            for item in &self.items[1..] {
                let (k, v) = item.key_value();
                f.write_str(";")?;
                f.write_str(k)?;
                f.write_str("=")?;
                f.write_str(v)?;
            }
            Ok(())
        }
    }
}

struct PropListVisitor<const N: usize> {
    marker: PhantomData<fn() -> PropList<N>>,
}

impl<const N: usize> PropListVisitor<N> {
    fn new() -> Self {
        Self {
            marker: PhantomData,
        }
    }
}

impl<'de, const N: usize> Visitor<'de> for PropListVisitor<N> {
    type Value = PropList<N>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a string -> string map")
    }

    fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let capacity = access.size_hint().unwrap_or(0);
        let mut new = PropList::with_capacity(capacity);
        while let Some((key, value)) = access.next_entry()? {
            new.insert(key, value);
        }
        Ok(new)
    }
}

impl<'de, const N: usize> Deserialize<'de> for PropList<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PropListVisitor::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_proplist_crud() {
        let mut a = DefaultPropList::new();
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

        // Retain
        a.insert("foo", "bar");
        a.insert("baz", "quux");
        assert_eq!(format!("{a}"), "b=123;baz=quux;foo=bar");
        a.retain(|key, _| key.starts_with("b"));
        assert_eq!(format!("{a}"), "b=123;baz=quux");

        // Clear
        a.clear();
        assert!(a.is_empty());
        assert_eq!(format!("{a:?}"), "{}");
    }

    #[test]
    fn test_proplist_traits() {
        let mut a = DefaultPropList::new();
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
        assert!(DefaultPropList::new() < *DefaultPropList::new().insert("a", "a"));
        assert!(*DefaultPropList::new().insert("a", "a") > DefaultPropList::new());
        assert!(
            *DefaultPropList::new().insert("a", "a") < *DefaultPropList::new().insert("a", "b")
        );
        assert!(
            *DefaultPropList::new().insert("a", "a").insert("b", "a")
                < *DefaultPropList::new().insert("a", "a").insert("c", "a")
        );
    }
}
