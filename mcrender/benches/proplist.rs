use std::collections::{BTreeMap, HashMap};
use std::hash::{Hash, Hasher};
use std::hint::black_box;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rand::distr::uniform::SampleRange;
use rand::distr::{Alphanumeric, SampleString};
use rand::prelude::*;

use mcrender::proplist::DefaultPropList as PropList;

const RANDOM_SEED: u64 = 42;

/// Generate `count` random `String` key-value pairs with random sizes according to
/// `key_size` and `value_size` ranges.
fn gen_test_data<K, V>(
    key_size: K,
    value_size: V,
    count: usize,
    rng: &mut StdRng,
) -> Vec<(String, String)>
where
    K: SampleRange<usize> + Clone,
    V: SampleRange<usize> + Clone,
{
    let mut test_data = Vec::with_capacity(count);
    for _ in 0..count {
        let key_len = rng.random_range(key_size.clone());
        let value_len = rng.random_range(value_size.clone());
        test_data.push((
            Alphanumeric.sample_string(rng, key_len),
            Alphanumeric.sample_string(rng, value_len),
        ))
    }
    test_data
}

/// Sort (key, value) data by key.
fn sort_test_data(test_data: &mut Vec<(String, String)>) {
    test_data.sort_by(|(k1, _v1), (k2, _v2)| k1.cmp(k2));
}

/// Items inserted in key-sorted order, e.g. when being added from another ordered source of key-value pairs.
fn bench_ordered_insertion(c: &mut Criterion) {
    // 2 different key-value sizes, one all gets inlined, the other all gets allocated
    for (key_size, value_size) in [(10, 10), (20, 30)] {
        let mut group = c.benchmark_group(format!("ordered_insertion/k={key_size},v={value_size}"));

        // Different numbers of key-value pairs, to test how performance scales with number of items
        for count in [1, 10, 100, 1000] {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let mut test_data = gen_test_data(
                key_size..=key_size,
                value_size..=value_size,
                count,
                &mut rng,
            );
            // Order the test data before iterating it to create a map
            sort_test_data(&mut test_data);

            group.bench_with_input(
                BenchmarkId::new("BTreeMap<String,String>", count),
                &test_data,
                |b, test_data| {
                    b.iter(|| {
                        let mut map = BTreeMap::<String, String>::new();
                        for (k, v) in test_data {
                            map.insert(
                                black_box(k.as_str()).to_owned(),
                                black_box(v.as_str()).to_owned(),
                            );
                        }
                        map
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("PropList", count),
                &test_data,
                |b, test_data| {
                    b.iter(|| {
                        let mut map = PropList::new();
                        for (k, v) in test_data {
                            map.insert(black_box(k.as_str()), black_box(v.as_str()));
                        }
                        map
                    });
                },
            );

            // How does performance change when we already know how many items there are going to be?
            group.bench_with_input(
                BenchmarkId::new("PropList::with_capacity(n)", count),
                &test_data,
                |b, test_data| {
                    b.iter(|| {
                        let mut map = PropList::with_capacity(count);
                        for (k, v) in test_data {
                            map.insert(black_box(k.as_str()), black_box(v.as_str()));
                        }
                        map
                    });
                },
            );
        }

        group.finish();
    }
}

/// Random insertion of items.
fn bench_random_insertion(c: &mut Criterion) {
    for (key_size, value_size) in [(10, 10), (20, 30)] {
        let mut group = c.benchmark_group(format!("random_insertion/k={key_size},v={value_size}"));

        for count in [1, 10, 100, 1000] {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let mut test_data = gen_test_data(
                key_size..=key_size,
                value_size..=value_size,
                count,
                &mut rng,
            );
            // Randomize the test data before iterating it to create a map
            test_data.shuffle(&mut rng);

            group.bench_with_input(
                BenchmarkId::new("BTreeMap<String,String>", count),
                &test_data,
                |b, test_data| {
                    b.iter(|| {
                        let mut map = BTreeMap::<String, String>::new();
                        for (k, v) in test_data {
                            map.insert(
                                black_box(k.as_str()).to_owned(),
                                black_box(v.as_str()).to_owned(),
                            );
                        }
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("PropList", count),
                &test_data,
                |b, test_data| {
                    b.iter(|| {
                        let mut map = PropList::new();
                        for (k, v) in test_data {
                            map.insert(black_box(k.as_str()), black_box(v.as_str()));
                        }
                        map
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("PropList::with_capacity(n)", count),
                &test_data,
                |b, test_data| {
                    b.iter(|| {
                        let mut map = PropList::with_capacity(count);
                        for (k, v) in test_data {
                            map.insert(black_box(k.as_str()), black_box(v.as_str()));
                        }
                        map
                    });
                },
            );
        }

        group.finish();
    }
}

/// Consuming iteration over every item.
fn bench_iteration(c: &mut Criterion) {
    for (key_size, value_size) in [(10, 10), (20, 30)] {
        let mut group = c.benchmark_group(format!("iteration/k={key_size},v={value_size}"));

        for count in [1, 10, 100, 1000] {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let test_data = gen_test_data(
                key_size..=key_size,
                value_size..=value_size,
                count,
                &mut rng,
            );

            group.bench_with_input(BenchmarkId::new("BTreeMap", count), &test_data, |b, _| {
                let map = BTreeMap::from_iter(test_data.iter().cloned());
                b.iter(|| {
                    for (key, value) in map.iter() {
                        black_box((key, value));
                    }
                });
            });

            group.bench_with_input(BenchmarkId::new("PropList", count), &test_data, |b, _| {
                let map =
                    PropList::from_iter(test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                b.iter(|| {
                    for (key, value) in map.iter() {
                        black_box((key, value));
                    }
                });
            });
        }

        group.finish();
    }
}

/// Looking up every item by key in a random order.
fn bench_lookup(c: &mut Criterion) {
    for (key_size, value_size) in [(10, 10), (20, 30)] {
        let mut group = c.benchmark_group(format!("lookup/k={key_size},v={value_size}"));

        for count in [1, 10, 100, 1000] {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let test_data = gen_test_data(
                key_size..=key_size,
                value_size..=value_size,
                count,
                &mut rng,
            );

            group.bench_with_input(BenchmarkId::new("BTreeMap", count), &test_data, |b, _| {
                let map = BTreeMap::from_iter(test_data.iter().cloned());
                // Randomize the keys for lookup order
                let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
                let mut keys = test_data.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
                keys.shuffle(&mut rng);
                b.iter(|| {
                    for k in keys.iter() {
                        black_box(map.get(black_box(k.as_str())));
                    }
                });
            });

            group.bench_with_input(BenchmarkId::new("PropList", count), &test_data, |b, _| {
                let map =
                    PropList::from_iter(test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                // Randomize the keys for lookup order
                let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
                let mut keys = test_data.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
                keys.shuffle(&mut rng);
                b.iter(|| {
                    for k in keys.iter() {
                        black_box(map.get(black_box(k.as_str())));
                    }
                });
            });
        }

        group.finish();
    }
}

// TODO: rewrite this, currently only the first iteration does anything
// fn bench_remove(c: &mut Criterion) {
//     const KEY_SIZE: usize = 10;
//     const VALUE_SIZE: usize = 10;
//     let mut group = c.benchmark_group("remove");
//
//     for n in [1, 5, 15, 50] {
//         let mut test_data: Vec<(String, String)> = Vec::with_capacity(n);
//         let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
//         // Random key insertion order
//         for _ in 0..n {
//             test_data.push((
//                 Alphanumeric.sample_string(&mut rng, KEY_SIZE),
//                 Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
//             ));
//         }
//         // Differently random key retrieval order
//         let mut keys = test_data.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
//         keys.shuffle(&mut rng);
//
//         group.bench_with_input(BenchmarkId::new("BTreeMap", n), &n, |b, _| {
//             let mut map = BTreeMap::from_iter(test_data.iter().cloned());
//             b.iter(|| {
//                 for k in keys.iter() {
//                     map.remove(black_box(k.as_str()));
//                 }
//             });
//         });
//
//         group.bench_with_input(BenchmarkId::new("PropList", n), &n, |b, _| {
//             let mut map = proplist::PropList::from_iter(
//                 test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())),
//             );
//             b.iter(|| {
//                 for k in keys.iter() {
//                     map.remove(black_box(k.as_str()));
//                 }
//             });
//         });
//     }
//
//     group.finish();
// }

/// Creating a copy of the data structure.
fn bench_clone(c: &mut Criterion) {
    for (key_size, value_size) in [(10, 10), (20, 30)] {
        let mut group = c.benchmark_group(format!("clone/k={key_size},v={value_size}"));

        for count in [1, 10, 100, 1000] {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let test_data = gen_test_data(
                key_size..=key_size,
                value_size..=value_size,
                count,
                &mut rng,
            );

            group.bench_with_input(BenchmarkId::new("BTreeMap", count), &test_data, |b, _| {
                let map = BTreeMap::from_iter(test_data.iter().cloned());
                b.iter(|| {
                    black_box(map.clone());
                });
            });

            group.bench_with_input(BenchmarkId::new("PropList", count), &test_data, |b, _| {
                let map =
                    PropList::from_iter(test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())));
                b.iter(|| {
                    black_box(map.clone());
                });
            });
        }

        group.finish();
    }
}

/// Performance of the implementation of Hash (contributes to performance when used as HashMap key).
fn bench_hash(c: &mut Criterion) {
    for (key_size, value_size) in [(10, 10), (20, 30)] {
        let mut group = c.benchmark_group(format!("hash/k={key_size},v={value_size}"));

        for count in [1, 10, 100] {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let test_data = gen_test_data(
                key_size..=key_size,
                value_size..=value_size,
                count,
                &mut rng,
            );

            group.bench_with_input(BenchmarkId::new("BTreeMap", count), &test_data, |b, _| {
                let map = BTreeMap::from_iter(test_data.iter().cloned());
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                b.iter(|| {
                    black_box(&map).hash(&mut hasher);
                });
                black_box(hasher.finish());
            });

            group.bench_with_input(BenchmarkId::new("PropList", count), &test_data, |b, _| {
                let map = PropList::from_iter(test_data.iter().cloned());
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                b.iter(|| {
                    black_box(&map).hash(&mut hasher);
                });
                black_box(hasher.finish());
            });
        }

        group.finish();
    }
}

/// Performance of the implementation of Eq (contributes to performance when used as HashMap key).
fn bench_eq(c: &mut Criterion) {
    for (key_size, value_size) in [(10, 10), (20, 30)] {
        let mut group = c.benchmark_group(format!("eq/k={key_size},v={value_size}"));

        for count in [1, 10, 100] {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let test_data = gen_test_data(
                key_size..=key_size,
                value_size..=value_size,
                count,
                &mut rng,
            );

            group.bench_with_input(BenchmarkId::new("BTreeMap", count), &test_data, |b, _| {
                let map = BTreeMap::from_iter(test_data.iter().cloned());
                let other = BTreeMap::from_iter(test_data.iter().cloned());
                b.iter(|| {
                    black_box(black_box(&map).eq(black_box(&other)));
                });
            });

            group.bench_with_input(BenchmarkId::new("PropList", count), &test_data, |b, _| {
                let map = PropList::from_iter(test_data.iter().cloned());
                let other = PropList::from_iter(test_data.iter().cloned());
                b.iter(|| {
                    black_box(black_box(&map).eq(black_box(&other)));
                });
            });
        }

        group.finish();
    }
}

/// Using a mapping as a HashMap key.
fn bench_hashmap_key(c: &mut Criterion) {
    for hashmap_key_count in [10, 100, 1000] {
        for (key_size, value_size) in [(10, 10), (20, 30)] {
            let mut group = c.benchmark_group(format!(
                "hashmap_key/n={hashmap_key_count}/k={key_size},v={value_size}"
            ));

            for count in [1, 10, 100] {
                let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
                let mut test_data: Vec<(Vec<(String, String)>, u64)> =
                    Vec::with_capacity(hashmap_key_count);
                for _ in 0..hashmap_key_count {
                    let value = rng.random();
                    let key = gen_test_data(
                        key_size..=key_size,
                        value_size..=value_size,
                        count,
                        &mut rng,
                    );
                    test_data.push((key, value));
                }

                group.bench_with_input(
                    BenchmarkId::new("BTreeMap", count),
                    &(count, hashmap_key_count),
                    |b, _| {
                        let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
                        let mut keys = Vec::with_capacity(hashmap_key_count);
                        let mut map = BTreeMap::new();
                        for (key, value) in test_data.iter() {
                            let key = BTreeMap::<String, String>::from_iter(key.iter().cloned());
                            map.insert(key.clone(), value.clone());
                            keys.push(key);
                        }
                        // Ensure keys get accessed in random order
                        keys.shuffle(&mut rng);
                        b.iter(|| {
                            for key in keys[0..3].iter() {
                                black_box(map.get(black_box(key)));
                            }
                        });
                    },
                );

                group.bench_with_input(
                    BenchmarkId::new("PropList", count),
                    &(count, hashmap_key_count),
                    |b, _| {
                        let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
                        let mut keys = Vec::with_capacity(hashmap_key_count);
                        let mut map = HashMap::new();
                        for (key, value) in test_data.iter() {
                            let key = PropList::from_iter(key.iter().cloned());
                            map.insert(key.clone(), value);
                            keys.push(key);
                        }
                        // Ensure keys get accessed in random order
                        keys.shuffle(&mut rng);
                        b.iter(|| {
                            for key in keys[0..3].iter() {
                                black_box(map.get(black_box(key)));
                            }
                        });
                    },
                );
            }

            group.finish();
        }
    }
}

/// Investigate why `PropList` is slower than `BTreeMap<String, String>` as `HashMap` key, despite
/// `Hash` and `Eq` both being equal or faster.
///
/// A large part of it seems to simply be that `size_of::<PropList>()` is 56 bytes compared to the
/// 24 bytes of `size_of::<BTreeMap<K, V>>`, making for larger buckets in `HashMap` and therefore
/// traversing more data during lookup. Is there any way to make `PropList` smaller?
fn bench_hashmap_key_bytes(c: &mut Criterion) {
    for hashmap_key_count in [10, 100, 1000] {
        let mut group = c.benchmark_group(format!("hashmap_key_bytes/n={hashmap_key_count}"));

        group.bench_with_input(BenchmarkId::new("hashmap", 24), &24, |b, _| {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let mut test_data: Vec<([u8; 24], u64)> = Vec::with_capacity(hashmap_key_count);
            for _ in 0..hashmap_key_count {
                let mut key = [0u8; 24];
                rng.fill_bytes(&mut key);
                let value = rng.random();
                test_data.push((key, value));
            }
            let map: HashMap<[u8; 24], u64> = HashMap::from_iter(test_data.iter().cloned());
            test_data.shuffle(&mut rng);
            b.iter(|| {
                for (key, _) in test_data[0..3].iter() {
                    black_box(map.get(black_box(key)));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("hashmap", 56), &56, |b, _| {
            let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
            let mut test_data: Vec<([u8; 56], u64)> = Vec::with_capacity(hashmap_key_count);
            for _ in 0..hashmap_key_count {
                let mut key = [0u8; 56];
                rng.fill_bytes(&mut key);
                let value = rng.random();
                test_data.push((key, value));
            }
            let map: HashMap<[u8; 56], u64> = HashMap::from_iter(test_data.iter().cloned());
            test_data.shuffle(&mut rng);
            b.iter(|| {
                for (key, _) in test_data[0..3].iter() {
                    black_box(map.get(black_box(key)));
                }
            });
        });

        group.finish();
    }
}

criterion_group!(
    benches,
    bench_ordered_insertion,
    bench_random_insertion,
    bench_iteration,
    bench_lookup,
    // bench_remove,
    bench_clone,
    bench_hash,
    bench_eq,
    bench_hashmap_key,
    bench_hashmap_key_bytes,
);
criterion_main!(benches);
