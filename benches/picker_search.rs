use copypasta::clipboard::item::ClipboardItem;
use copypasta::ui::visible_indices_for_search;
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

fn item(id: u64, preview: String) -> ClipboardItem {
    ClipboardItem {
        id,
        created_unix_ms: id as i64,
        fingerprint: [id as u8; 32],
        preview,
        formats: Vec::new(),
    }
}

fn items(count: usize, long_preview: bool) -> Vec<ClipboardItem> {
    (0..count as u64)
        .map(|id| {
            let preview = if long_preview {
                format!(
                    "invoice customer project milestone repeated searchable text id {id} {}",
                    "details ".repeat(32)
                )
            } else {
                format!("snippet {id} searchable")
            };
            item(id, preview)
        })
        .collect()
}

fn picker_search_benchmarks(c: &mut Criterion) {
    let short_items = items(50, false);
    let long_items = items(50, true);

    c.bench_function("picker search empty query 50 short previews", |b| {
        b.iter(|| {
            black_box(visible_indices_for_search(
                black_box(&short_items),
                black_box(""),
            ))
        })
    });

    c.bench_function("picker search matching query 50 short previews", |b| {
        b.iter(|| {
            black_box(visible_indices_for_search(
                black_box(&short_items),
                black_box("searchable"),
            ))
        })
    });

    c.bench_function("picker search matching query 50 long previews", |b| {
        b.iter(|| {
            black_box(visible_indices_for_search(
                black_box(&long_items),
                black_box("milestone"),
            ))
        })
    });

    c.bench_function("picker search missing query 50 long previews", |b| {
        b.iter(|| {
            black_box(visible_indices_for_search(
                black_box(&long_items),
                black_box("not-present"),
            ))
        })
    });
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = picker_search_benchmarks
}
criterion_main!(benches);
