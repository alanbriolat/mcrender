use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};
use rand::prelude::*;

use mcrender::canvas::{Rgb, Rgba, avx2, scalar, sse4};

const RANDOM_SEED: u64 = 42;

fn bench_overlay_final_rgba_rgba(c: &mut Criterion) {
    let mut group = c.benchmark_group("rgba8_overlay_final");

    for buffer_size in [24, 128, 256] {
        let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
        // Generate a random base image row, with alpha = 255 and random color values
        let mut dst_base = vec![Rgba([0, 0, 0, 255]); buffer_size];
        for p in dst_base.iter_mut() {
            p[0] = rng.random();
            p[1] = rng.random();
            p[2] = rng.random();
        }
        // Generate a random foreground image row, with random colors and alpha
        let mut src = vec![Rgba([0, 0, 0, 0]); buffer_size];
        for p in src.iter_mut() {
            p[0] = rng.random();
            p[1] = rng.random();
            p[2] = rng.random();
            p[3] = rng.random();
        }

        group.bench_function(BenchmarkId::new("scalar", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base.clone(),
                |dst| {
                    black_box(unsafe {
                        scalar::rgba8_overlay_final(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("sse4", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base.clone(),
                |dst| {
                    black_box(unsafe {
                        sse4::rgba8_overlay_final(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("avx2", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base.clone(),
                |dst| {
                    black_box(unsafe {
                        avx2::rgba8_overlay_final(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

fn bench_overlay_rgba_onto_rgb(c: &mut Criterion) {
    let mut group = c.benchmark_group("rgba8_onto_rgb8_overlay");

    for buffer_size in [24, 128, 256] {
        let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
        // Generate a random base image row, with alpha = 255 and random color values
        let mut dst_base = vec![Rgb([0, 0, 0]); buffer_size];
        for p in dst_base.iter_mut() {
            p[0] = rng.random();
            p[1] = rng.random();
            p[2] = rng.random();
        }
        // Generate a random foreground image row, with random colors and alpha
        let mut src = vec![Rgba([0, 0, 0, 0]); buffer_size];
        for p in src.iter_mut() {
            p[0] = rng.random();
            p[1] = rng.random();
            p[2] = rng.random();
            p[3] = rng.random();
        }

        group.bench_function(BenchmarkId::new("scalar", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base.clone(),
                |dst| {
                    black_box(unsafe {
                        scalar::rgba8_onto_rgb8_overlay(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("sse4", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base.clone(),
                |dst| {
                    black_box(unsafe {
                        sse4::rgba8_onto_rgb8_overlay(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("avx2", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base.clone(),
                |dst| {
                    black_box(unsafe {
                        avx2::rgba8_onto_rgb8_overlay(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_overlay_final_rgba_rgba,
    bench_overlay_rgba_onto_rgb,
);
criterion_main!(benches);
