use criterion::{BatchSize, Criterion, Throughput, criterion_group, criterion_main};
use mmterm::terminal::TerminalParser;
use mmterm::terminal::grid::{Color, GridColors};

fn make_parser() -> TerminalParser {
    TerminalParser::new_with_colors(
        220,
        50,
        GridColors {
            fg: Color::WHITE,
            bg: Color::BLACK,
            cursor: Color::WHITE,
            selection: Color::WHITE,
            palette: [Color::BLACK; 16],
        },
        10_000,
    )
}

/// Realistic terminal output: printable ASCII with ANSI color codes.
/// Roughly matches what `ls --color`, `git log`, etc. produce.
fn realistic_payload(size: usize) -> Vec<u8> {
    let line = b"\x1b[32mhello world\x1b[0m foo bar baz qux 1234567890\r\n";
    line.iter().cycle().take(size).copied().collect()
}

/// Pure printable ASCII — best-case for a SIMD fast-path.
fn ascii_payload(size: usize) -> Vec<u8> {
    b"hello world foo bar baz qux 1234567890\r\n"
        .iter()
        .cycle()
        .take(size)
        .copied()
        .collect()
}

/// Dense SGR: lots of color-change sequences, few printable chars per sequence.
/// Stress-tests the escape-sequence dispatch path.
fn dense_sgr_payload(size: usize) -> Vec<u8> {
    let seq = b"\x1b[1;32mA\x1b[0m\x1b[1;31mB\x1b[0m\x1b[1;34mC\x1b[0m";
    seq.iter().cycle().take(size).copied().collect()
}

fn bench_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("parser");

    // Realistic output — setup (make_parser) excluded from measurement
    let payload = realistic_payload(256 * 1024);
    group.throughput(Throughput::Bytes(payload.len() as u64));
    group.bench_function("realistic_256kb", |b| {
        b.iter_batched(
            make_parser,
            |mut p| p.process(&payload),
            BatchSize::LargeInput,
        );
    });

    // Pure ASCII — isolates the raw byte-loop cost
    let payload = ascii_payload(256 * 1024);
    group.throughput(Throughput::Bytes(payload.len() as u64));
    group.bench_function("ascii_256kb", |b| {
        b.iter_batched(
            make_parser,
            |mut p| p.process(&payload),
            BatchSize::LargeInput,
        );
    });

    // Dense SGR — stress-tests escape-sequence dispatch
    let payload = dense_sgr_payload(64 * 1024);
    group.throughput(Throughput::Bytes(payload.len() as u64));
    group.bench_function("dense_sgr_64kb", |b| {
        b.iter_batched(
            make_parser,
            |mut p| p.process(&payload),
            BatchSize::LargeInput,
        );
    });

    // Steady-state: same parser reused across calls (simulates a running terminal)
    let payload = realistic_payload(256 * 1024);
    group.throughput(Throughput::Bytes(payload.len() as u64));
    group.bench_function("realistic_256kb_steady", |b| {
        let mut p = make_parser();
        b.iter(|| p.process(&payload));
    });

    group.finish();
}

fn bench_seq_simulation(c: &mut Criterion) {
    // Simulate exactly `seq 1 100000` — the workload the user observed as slow.
    // Each line triggers scroll_up(1) after the first 50 rows fill the grid.
    let payload: Vec<u8> = (1u32..=100_000)
        .flat_map(|n| format!("{n}\n").into_bytes())
        .collect();

    let mut group = c.benchmark_group("seq_simulation");
    group.sample_size(20); // slow benchmark — fewer samples
    group.throughput(Throughput::Bytes(payload.len() as u64));

    // Full end-to-end: parse all 588KB including ~99,950 scroll_up calls
    group.bench_function("seq_1_100000", |b| {
        b.iter_batched(
            make_parser,
            |mut p| p.process(&payload),
            BatchSize::LargeInput,
        );
    });

    // Same payload but on a tall grid (200 rows) — far fewer scroll_up calls.
    // Isolates how much of the cost is scroll vs pure parsing.
    group.bench_function("seq_1_100000_tall_grid", |b| {
        b.iter_batched(
            || {
                TerminalParser::new_with_colors(
                    220,
                    200,
                    GridColors {
                        fg: Color::WHITE,
                        bg: Color::BLACK,
                        cursor: Color::WHITE,
                        selection: Color::WHITE,
                        palette: [Color::BLACK; 16],
                    },
                    10_000,
                )
            },
            |mut p| p.process(&payload),
            BatchSize::LargeInput,
        );
    });

    group.finish();
}

criterion_group!(benches, bench_parser, bench_seq_simulation);
criterion_main!(benches);
