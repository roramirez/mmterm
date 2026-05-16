use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use mmterm::terminal::grid::{Color, Grid, GridColors};

fn make_grid(cols: usize, rows: usize) -> Grid {
    Grid::with_colors(
        cols,
        rows,
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

fn bench_scroll(c: &mut Criterion) {
    let mut group = c.benchmark_group("scroll_up");

    for (cols, rows) in [(80usize, 24usize), (220, 50), (220, 100)] {
        let label = format!("{cols}x{rows}");
        let cell_size = std::mem::size_of::<mmterm::terminal::grid::Cell>();
        // throughput = bytes moved per scroll_up(1) call (one full grid shift)
        let bytes_moved = (cols * rows * cell_size) as u64;
        group.throughput(Throughput::Bytes(bytes_moved));
        group.bench_function(&label, |b| {
            let mut grid = make_grid(cols, rows);
            // Pre-fill with non-space chars so clones are non-trivial
            for row in 0..rows {
                for col in 0..cols {
                    grid.cell_mut(col, row).c = 'A';
                }
            }
            b.iter(|| {
                grid.scroll_up(1);
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_scroll);
criterion_main!(benches);
