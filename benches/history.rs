use copypasta::clipboard::item::ClipboardItem;
use copypasta::history::History;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn item(id: u64) -> ClipboardItem {
    ClipboardItem {
        id,
        created_unix_ms: id as i64,
        fingerprint: [id as u8; 32],
        preview: format!("benchmark item {id}"),
        formats: Vec::new(),
    }
}

fn filled_history(count: usize) -> History {
    let mut history = History::new(count);
    for id in 1..=count as u64 {
        history.add_or_bump_full(item(id));
    }
    history
}

fn history_benchmarks(c: &mut Criterion) {
    c.bench_function("history add 50 unique items", |b| {
        b.iter(|| {
            let mut history = History::new(50);
            for id in 1..=50 {
                history.add_or_bump_full(black_box(item(id)));
            }
            black_box(history.len())
        })
    });

    c.bench_function("history bump oldest item in 50", |b| {
        b.iter_batched(
            || filled_history(50),
            |mut history| black_box(history.bump_by_fingerprint([1; 32])),
            criterion::BatchSize::SmallInput,
        )
    });

    c.bench_function("history remove middle item in 50", |b| {
        b.iter_batched(
            || filled_history(50),
            |mut history| black_box(history.remove_by_id(25)),
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = history_benchmarks
}
criterion_main!(benches);
