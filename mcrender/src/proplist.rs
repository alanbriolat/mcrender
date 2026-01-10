use bytes::{Bytes, BytesMut};
use std::hash::{Hash, Hasher};

struct BuilderEntry {
    key: Bytes,
    value: BytesMut,
}

impl BuilderEntry {
    fn key(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.key) }
    }

    fn value(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.value) }
    }
}

pub struct Builder {
    unallocated: BytesMut,
    entries: Vec<BuilderEntry>,
    key_sep: BytesMut,
    item_sep: BytesMut,
}

impl Builder {
    pub fn with_capacity(data: usize, entries: usize) -> Self {
        let buffer = BytesMut::with_capacity(data);
        let mut unallocated = buffer.clone();
        let key_sep = push(&mut unallocated, "=".as_bytes());
        let item_sep = push(&mut unallocated, ";".as_bytes());
        Builder {
            unallocated,
            entries: Vec::with_capacity(entries),
            key_sep,
            item_sep,
        }
    }

    pub fn with_separators(mut self, key: &str, item: &str) -> Self {
        reuse_or_push(&mut self.key_sep, &mut self.unallocated, key.as_bytes());
        reuse_or_push(&mut self.item_sep, &mut self.unallocated, item.as_bytes());
        self
    }

    pub fn with_entry(mut self, key: &str, value: &str) -> Self {
        self.insert(key, value);
        self
    }

    pub fn with_entries<'a>(
        mut self,
        entries: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        for (key, value) in entries.into_iter() {
            self.insert(key, value);
        }
        self
    }

    pub fn insert(&mut self, key: &str, value: &str) {
        match self.entries.binary_search_by(|entry| entry.key().cmp(key)) {
            Ok(i) => {
                // Existing entry, update it, in-place if possible
                let entry = &mut self.entries[i];
                reuse_or_push(&mut entry.value, &mut self.unallocated, value.as_bytes());
            }
            Err(i) => {
                // No existing entry, insert a new one, in the correct position
                self.unallocated.reserve(key.len() + value.len());
                let key = push(&mut self.unallocated, key.as_bytes()).freeze();
                let value = push(&mut self.unallocated, value.as_bytes());
                let entry = BuilderEntry { key, value };
                self.entries.insert(i, entry);
            }
        }
    }

    pub fn build(self) -> PropList {
        let mut buffer_size: usize = self
            .entries
            .iter()
            .map(|entry| entry.key.len() + entry.value.len())
            .sum();
        buffer_size += self.key_sep.len() * self.entries.len();
        if self.entries.len() != 0 {
            buffer_size += self.item_sep.len() * (self.entries.len() - 1);
        }

        let mut buffer = BytesMut::with_capacity(buffer_size);
        let mut first = true;
        for entry in self.entries.iter() {
            if !first {
                buffer.extend_from_slice(self.item_sep.as_ref());
            } else {
                first = false;
            }
            buffer.extend_from_slice(entry.key.as_ref());
            buffer.extend_from_slice(self.key_sep.as_ref());
            buffer.extend_from_slice(entry.value.as_ref());
        }
        let buffer = buffer.freeze();

        let mut remaining = buffer.clone();
        let mut entries = Vec::with_capacity(self.entries.len());
        let mut first = true;
        for entry in self.entries {
            if !first {
                remaining.split_to(self.item_sep.len());
            } else {
                first = false;
            }
            let key = remaining.split_to(entry.key.len());
            remaining.split_to(self.key_sep.len());
            let value = remaining.split_to(entry.value.len());
            entries.push(Entry { key, value });
        }

        PropList { buffer, entries }
    }
}

fn push(unallocated: &mut BytesMut, data: &[u8]) -> BytesMut {
    unallocated.extend_from_slice(data);
    unallocated.split_to(data.len())
}

fn reuse_or_push(buffer: &mut BytesMut, unallocated: &mut BytesMut, data: &[u8]) {
    if buffer.capacity() >= data.len() {
        buffer.truncate(0);
        buffer.extend_from_slice(data);
    } else {
        *buffer = push(unallocated, data);
    }
}

#[derive(Clone, Hash, Eq, PartialEq)]
pub struct Entry {
    key: Bytes,
    value: Bytes,
}

impl Entry {
    pub fn key(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.key) }
    }

    pub fn value(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.value) }
    }
}

#[derive(Clone, derive_more::Eq, derive_more::PartialEq)]
pub struct PropList {
    #[eq(skip)]
    buffer: Bytes,
    entries: Vec<Entry>,
}

impl PropList {
    pub fn builder() -> Builder {
        Self::builder_with_capacity(64, 8)
    }

    pub fn builder_with_capacity(data: usize, entries: usize) -> Builder {
        Builder::with_capacity(data, entries)
    }

    pub fn as_str(&self) -> &str {
        unsafe { str::from_utf8_unchecked(&self.buffer) }
    }

    pub fn contains(&self, key: &str, value: &str) -> bool {
        self.entry(key)
            .map(|entry| entry.value() == value)
            .unwrap_or(false)
    }

    // Standard HashMap/BTreeMap-like methods (for non-mut)

    pub fn contains_key(&self, key: &str) -> bool {
        self.entry(key).is_some()
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entry(key).map(|entry| entry.value())
    }

    pub fn get_key_value(&self, key: &str) -> Option<(&str, &str)> {
        self.entry(key).map(|entry| (entry.key(), entry.value()))
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries
            .iter()
            .map(|entry| (entry.key(), entry.value()))
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.key())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|entry| entry.value())
    }

    // Helpers

    fn entry(&self, key: &str) -> Option<&Entry> {
        self.entries
            .binary_search_by(|entry| entry.key().cmp(key))
            .map(|i| &self.entries[i])
            .ok()
    }
}

impl AsRef<str> for PropList {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl AsRef<[u8]> for PropList {
    fn as_ref(&self) -> &[u8] {
        self.buffer.as_ref()
    }
}

impl std::fmt::Debug for PropList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut ds = f.debug_struct("PropList");
        for entry in self.entries.iter() {
            ds.field(entry.key(), &entry.value());
        }
        ds.finish()
    }
}

impl std::fmt::Display for PropList {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Hash for PropList {
    fn hash<H: Hasher>(&self, state: &mut H) {
        for entry in self.entries.iter() {
            entry.hash(state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push() {
        let mut buf = BytesMut::with_capacity(64);
        let mut hello = push(&mut buf, b"hello");
        assert_eq!(hello.len(), 5);
        assert_eq!(hello.capacity(), 5);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 59);

        reuse_or_push(&mut hello, &mut buf, b"foo");
        assert_eq!(hello.len(), 3);
        assert_eq!(hello.capacity(), 5);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 59);

        reuse_or_push(&mut hello, &mut buf, b"foobar");
        assert_eq!(hello.len(), 6);
        assert_eq!(hello.capacity(), 6);
        assert_eq!(buf.len(), 0);
        assert_eq!(buf.capacity(), 53);
    }

    #[test]
    fn test_proplist() {
        let a = PropList::builder()
            .with_entry("b", "123")
            .with_entry("a", "456")
            .build();
        assert!(a.contains_key("a"));
        assert!(a.contains("b", "123"));
        assert!(!a.contains("b", "456"));
        assert_eq!(a.get("a"), Some("456"));
        assert_eq!(a.as_str(), "a=456;b=123");

        let b = a.clone();
        assert_eq!(b, a);
        assert_eq!(b.as_str(), a.as_str());

        let c = PropList::builder()
            .with_entry("b", "123")
            .with_entry("a", "456")
            .build();
        assert_eq!(c, a);

        let d = PropList::builder()
            .with_separators("::", "")
            .with_entry("b", "123")
            .with_entry("a", "456")
            .build();
        assert_eq!(d.as_str(), "a::456b::123");
        assert_eq!(d, a);
    }
}
