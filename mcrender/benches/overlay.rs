use std::hint::black_box;

use criterion::{BatchSize, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use rand::prelude::*;

use mcrender::canvas::{Rgb, Rgba, avx2, scalar, sse4};

const RANDOM_SEED: u64 = 42;

fn bench_rgba8_overlay(c: &mut Criterion) {
    let mut group = c.benchmark_group("rgba8_overlay");

    for buffer_size in [24, 128, 256] {
        group.throughput(Throughput::Elements(buffer_size as u64));
        let mut rng = StdRng::seed_from_u64(RANDOM_SEED);
        // Generate a random base image row, with alpha = 255 and random color values
        let mut dst_base_rgba = vec![Rgba([0, 0, 0, 255]); buffer_size];
        for p in dst_base_rgba.iter_mut() {
            p[0] = rng.random();
            p[1] = rng.random();
            p[2] = rng.random();
        }
        // A copy of the base image in RGB format, for comparing RGBA and RGB `overlay_final()` implementations
        let dst_base_rgb: Vec<_> = dst_base_rgba.iter().map(|p| p.to_rgb()).collect();
        // Generate a random foreground image row, with random colors and alpha
        let mut src = vec![Rgba([0, 0, 0, 0]); buffer_size];
        for p in src.iter_mut() {
            p[0] = rng.random();
            p[1] = rng.random();
            p[2] = rng.random();
            p[3] = rng.random();
        }

        group.bench_function(BenchmarkId::new("rgba8_scalar", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base_rgba.clone(),
                |dst| {
                    black_box(unsafe {
                        scalar::rgba8_overlay_final(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("rgba8_sse4", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base_rgba.clone(),
                |dst| {
                    black_box(unsafe {
                        sse4::rgba8_overlay_final(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("rgba8_avx2", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base_rgba.clone(),
                |dst| {
                    black_box(unsafe {
                        avx2::rgba8_overlay_final(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("rgba8_to_rgb8_scalar", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base_rgb.clone(),
                |dst| {
                    black_box(unsafe {
                        scalar::rgba8_onto_rgb8_overlay(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("rgba8_to_rgb8_sse4", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base_rgb.clone(),
                |dst| {
                    black_box(unsafe {
                        sse4::rgba8_onto_rgb8_overlay(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(BenchmarkId::new("rgba8_to_rgb8_avx2", buffer_size), |b| {
            b.iter_batched_ref(
                || dst_base_rgb.clone(),
                |dst| {
                    black_box(unsafe {
                        avx2::rgba8_onto_rgb8_overlay(black_box(dst), black_box(&src))
                    });
                },
                BatchSize::LargeInput,
            );
        });

        group.bench_function(
            BenchmarkId::new("full_rgba8_as_rgba32f_scalar", buffer_size),
            |b| {
                b.iter_batched_ref(
                    || dst_base_rgba.clone(),
                    |dst| {
                        black_box(unsafe {
                            scalar::rgba8_as_rgba32f_overlay(black_box(dst), black_box(&src))
                        });
                    },
                    BatchSize::LargeInput,
                );
            },
        );

        group.bench_function(
            BenchmarkId::new("full_rgba8_as_rgba32f_sse4", buffer_size),
            |b| {
                b.iter_batched_ref(
                    || dst_base_rgba.clone(),
                    |dst| {
                        black_box(unsafe {
                            sse4::rgba8_as_rgba32f_overlay(black_box(dst), black_box(&src))
                        });
                    },
                    BatchSize::LargeInput,
                );
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_rgba8_overlay,);
criterion_main!(benches);
