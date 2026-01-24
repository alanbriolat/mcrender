use arcstr::ArcStr;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Default)]
struct Interner {
    data: RwLock<HashMap<ArcStr, ()>>,
}

impl Interner {
    fn get<S: AsRef<str>>(&self, key: S) -> ArcStr {
        // Attempt to get the shared string using only a read lock
        let lock = self.data.read();
        if let Some((k, _)) = lock.get_key_value(key.as_ref()) {
            return k.clone();
        }
        drop(lock);
        // Otherwise, get the write lock, try one last time for existing shared string, otherwise
        // create a new one
        let mut lock = self.data.write();
        if let Some((k, _)) = lock.get_key_value(key.as_ref()) {
            k.clone()
        } else {
            let k = ArcStr::from(key.as_ref());
            lock.insert(k.clone(), ());
            k
        }
    }
}

static INTERNER: OnceLock<Interner> = OnceLock::new();

pub fn intern_str<S: AsRef<str>>(s: S) -> ArcStr {
    INTERNER.get_or_init(Default::default).get(s)
}
