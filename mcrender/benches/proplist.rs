use std::collections::{BTreeMap, HashMap};
use std::hint::black_box;
use std::iter::repeat_with;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use rand::distr::{Alphanumeric, SampleString};
use rand::prelude::*;

use mcrender::proplist;

fn bench_ordered_insertion(c: &mut Criterion) {
    const KEY_SIZE: usize = 10;
    const VALUE_SIZE: usize = 10;
    let mut group = c.benchmark_group("ordered_insertion");

    for n in [1, 5, 15, 50] {
        let mut test_data: Vec<(String, String)> = Vec::with_capacity(n);
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..n {
            test_data.push((
                Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
            ));
        }
        test_data.sort_by(|(k1, _v1), (k2, v2)| k1.cmp(k2));

        group.bench_with_input(
            BenchmarkId::new("BTreeMap<String,String>", n),
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
            BenchmarkId::new("proplist::PropList", n),
            &test_data,
            |b, test_data| {
                b.iter(|| {
                    let mut map = proplist::PropList::new();
                    for (k, v) in test_data {
                        map.insert(black_box(k.as_str()), black_box(v.as_str()));
                    }
                    map
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new(
                "proplist::PropList::with_capacity(n*(KEY_SIZE+VALUE_SIZE), n)",
                n,
            ),
            &test_data,
            |b, test_data| {
                b.iter(|| {
                    let mut map = proplist::PropList::with_capacity(n * (KEY_SIZE + VALUE_SIZE), n);
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

fn bench_random_insertion(c: &mut Criterion) {
    const KEY_SIZE: usize = 10;
    const VALUE_SIZE: usize = 10;
    let mut group = c.benchmark_group("random_insertion");

    for n in [1, 5, 15, 50] {
        let mut test_data: Vec<(String, String)> = Vec::with_capacity(n);
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..n {
            test_data.push((
                Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
            ));
        }

        group.bench_with_input(
            BenchmarkId::new("BTreeMap<String,String>", n),
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
            BenchmarkId::new("proplist::PropList", n),
            &test_data,
            |b, test_data| {
                b.iter(|| {
                    let mut map = proplist::PropList::new();
                    for (k, v) in test_data {
                        map.insert(black_box(k.as_str()), black_box(v.as_str()));
                    }
                    map
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new(
                "proplist::PropList::with_capacity(n*(KEY_SIZE+VALUE_SIZE), n)",
                n,
            ),
            &test_data,
            |b, test_data| {
                b.iter(|| {
                    let mut map = proplist::PropList::with_capacity(n * (KEY_SIZE + VALUE_SIZE), n);
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

fn bench_iteration(c: &mut Criterion) {
    const KEY_SIZE: usize = 10;
    const VALUE_SIZE: usize = 10;
    let mut group = c.benchmark_group("iteration");

    for n in [1, 5, 15, 50] {
        let mut test_data: Vec<(String, String)> = Vec::with_capacity(n);
        let mut rng = StdRng::seed_from_u64(42);
        for _ in 0..n {
            test_data.push((
                Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
            ));
        }

        group.bench_with_input(BenchmarkId::new("BTreeMap", n), &n, |b, _| {
            let map = BTreeMap::from_iter(test_data.iter().cloned());
            b.iter(|| {
                for (key, value) in map.iter() {
                    black_box((key, value));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("PropList", n), &n, |b, _| {
            let map = proplist::PropList::from_iter(
                test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            );
            b.iter(|| {
                for (key, value) in map.iter() {
                    black_box((key, value));
                }
            });
        });
    }

    group.finish();
}

fn bench_lookup(c: &mut Criterion) {
    const KEY_SIZE: usize = 10;
    const VALUE_SIZE: usize = 10;
    let mut group = c.benchmark_group("lookup");

    for n in [1, 5, 15, 50] {
        let mut test_data: Vec<(String, String)> = Vec::with_capacity(n);
        let mut rng = StdRng::seed_from_u64(42);
        // Random key insertion order
        for _ in 0..n {
            test_data.push((
                Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
            ));
        }
        // Differently random key retrieval order
        let mut keys = test_data.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        keys.shuffle(&mut rng);

        group.bench_with_input(BenchmarkId::new("BTreeMap", n), &n, |b, _| {
            let map = BTreeMap::from_iter(test_data.iter().cloned());
            b.iter(|| {
                for k in keys.iter() {
                    black_box(map.get(black_box(k.as_str())));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("PropList", n), &n, |b, _| {
            let map = proplist::PropList::from_iter(
                test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            );
            b.iter(|| {
                for k in keys.iter() {
                    black_box(map.get(black_box(k.as_str())));
                }
            });
        });
    }

    group.finish();
}

fn bench_remove(c: &mut Criterion) {
    const KEY_SIZE: usize = 10;
    const VALUE_SIZE: usize = 10;
    let mut group = c.benchmark_group("remove");

    for n in [1, 5, 15, 50] {
        let mut test_data: Vec<(String, String)> = Vec::with_capacity(n);
        let mut rng = StdRng::seed_from_u64(42);
        // Random key insertion order
        for _ in 0..n {
            test_data.push((
                Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
            ));
        }
        // Differently random key retrieval order
        let mut keys = test_data.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
        keys.shuffle(&mut rng);

        group.bench_with_input(BenchmarkId::new("BTreeMap", n), &n, |b, _| {
            let mut map = BTreeMap::from_iter(test_data.iter().cloned());
            b.iter(|| {
                for k in keys.iter() {
                    map.remove(black_box(k.as_str()));
                }
            });
        });

        group.bench_with_input(BenchmarkId::new("PropList", n), &n, |b, _| {
            let mut map = proplist::PropList::from_iter(
                test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            );
            b.iter(|| {
                for k in keys.iter() {
                    map.remove(black_box(k.as_str()));
                }
            });
        });
    }

    group.finish();
}

fn bench_clone(c: &mut Criterion) {
    const KEY_SIZE: usize = 10;
    const VALUE_SIZE: usize = 10;
    let mut group = c.benchmark_group("clone");

    for n in [1, 5, 15, 50] {
        let mut test_data: Vec<(String, String)> = Vec::with_capacity(n);
        let mut rng = StdRng::seed_from_u64(42);
        // Random key insertion order
        for _ in 0..n {
            test_data.push((
                Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
            ));
        }

        group.bench_with_input(BenchmarkId::new("BTreeMap", n), &n, |b, _| {
            let mut map = BTreeMap::from_iter(test_data.iter().cloned());
            b.iter(|| {
                black_box(map.clone());
            });
        });

        group.bench_with_input(BenchmarkId::new("PropList", n), &n, |b, _| {
            let mut map = proplist::PropList::from_iter(
                test_data.iter().map(|(k, v)| (k.as_str(), v.as_str())),
            );
            b.iter(|| {
                black_box(map.clone());
            });
        });
    }

    group.finish();
}

fn bench_hashmap_key(c: &mut Criterion) {
    const KEY_SIZE: usize = 10;
    const VALUE_SIZE: usize = 10;
    let mut group = c.benchmark_group("hashmap_key");

    for key_item_count in [1, 5, 15, 50] {
        for key_count in [10, 100, 1000] {
            group.bench_with_input(
                BenchmarkId::new("BTreeMap", format!("{:?}", (key_item_count, key_count))),
                &(key_item_count, key_count),
                |b, _| {
                    let mut rng = StdRng::seed_from_u64(42);
                    let mut keys: Vec<BTreeMap<String, String>> = repeat_with(|| {
                        BTreeMap::from_iter(
                            repeat_with(|| {
                                (
                                    Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                                    Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
                                )
                            })
                            .take(key_item_count),
                        )
                    })
                    .take(key_count)
                    .collect();
                    let map: HashMap<BTreeMap<String, String>, u64> =
                        HashMap::from_iter(keys.iter().cloned().map(|k| (k, rng.random())));
                    keys.shuffle(&mut rng);
                    b.iter(|| {
                        for key in keys.iter() {
                            black_box(map.get(black_box(key)));
                        }
                    });
                },
            );

            group.bench_with_input(
                BenchmarkId::new("PropList", format!("{:?}", (key_item_count, key_count))),
                &(key_item_count, key_count),
                |b, _| {
                    let mut rng = StdRng::seed_from_u64(42);
                    let mut keys: Vec<proplist::PropList> = repeat_with(|| {
                        proplist::PropList::from_iter(
                            repeat_with(|| {
                                (
                                    Alphanumeric.sample_string(&mut rng, KEY_SIZE),
                                    Alphanumeric.sample_string(&mut rng, VALUE_SIZE),
                                )
                            })
                            .take(key_item_count),
                        )
                    })
                    .take(key_count)
                    .collect();
                    let map: HashMap<proplist::PropList, u64> =
                        HashMap::from_iter(keys.iter().cloned().map(|k| (k, rng.random())));
                    keys.shuffle(&mut rng);
                    b.iter(|| {
                        for key in keys.iter() {
                            black_box(map.get(black_box(key)));
                        }
                    });
                },
            );
        }
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_ordered_insertion,
    bench_random_insertion,
    bench_iteration,
    bench_lookup,
    bench_remove,
    bench_clone,
    bench_hashmap_key,
);
criterion_main!(benches);
