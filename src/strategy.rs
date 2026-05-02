use crate::config::ScribbleConfig;
use crate::mutation::{Mutation, MutationKind};
use crate::target::Target;
use hekate_core::trace::{ColumnTrace, ColumnType};
use hekate_math::TowerField;
use hekate_program::ProgramWitness;
use proptest::prelude::*;
use proptest::sample::select;
use proptest::strategy::{BoxedStrategy, Union};

#[derive(Clone, Debug)]
struct Segment {
    target: Target,
    allowed_cols: Vec<usize>,
    num_rows: usize,
}

#[derive(Clone, Copy, Debug)]
struct Cell {
    target: Target,
    col: usize,
    col_type: ColumnType,
    num_rows: usize,
}

/// Proptest strategy producing Layer-1
/// mutations against `witness`.
pub fn mutation_strategy<F: TowerField>(
    witness: &ProgramWitness<F, ColumnTrace>,
    config: &ScribbleConfig,
) -> BoxedStrategy<Mutation> {
    let shapes = snapshot(witness, config);
    let mut subs: Vec<BoxedStrategy<Mutation>> = Vec::new();

    if config.is_kind_allowed(MutationKind::BitFlip) {
        let cells = cells_for(MutationKind::BitFlip, &shapes, config);
        if !cells.is_empty() {
            subs.push(bitflip_strategy(cells));
        }
    }

    if config.is_kind_allowed(MutationKind::OutOfBounds) {
        let cells = cells_for(MutationKind::OutOfBounds, &shapes, config);
        if !cells.is_empty() {
            subs.push(out_of_bounds_strategy(cells));
        }
    }

    if config.is_kind_allowed(MutationKind::FlipSelector) {
        let cells = cells_for(MutationKind::FlipSelector, &shapes, config);
        if !cells.is_empty() {
            subs.push(flip_selector_strategy(cells));
        }
    }

    if config.is_kind_allowed(MutationKind::ColumnUniformWrite) {
        let cells = cells_for(MutationKind::ColumnUniformWrite, &shapes, config);
        if !cells.is_empty() {
            subs.push(column_uniform_write_strategy(cells));
        }
    }

    if config.is_kind_allowed(MutationKind::MonotonicReplace) {
        let cells = cells_for(MutationKind::MonotonicReplace, &shapes, config);
        if !cells.is_empty() {
            subs.push(monotonic_replace_strategy(cells));
        }
    }

    if config.is_kind_allowed(MutationKind::RowSegmentZero) {
        let segments = segments_for(&shapes, config);
        if !segments.is_empty() {
            subs.push(row_segment_zero_strategy(segments));
        }
    }

    let row_targets: Vec<(Target, usize)> = shapes
        .iter()
        .filter(|(_, n, _)| *n >= 2)
        .map(|(t, n, _)| (*t, *n))
        .collect();

    if config.is_kind_allowed(MutationKind::SwapRows) && !row_targets.is_empty() {
        subs.push(swap_rows_strategy(row_targets.clone()));
    }

    if config.is_kind_allowed(MutationKind::DuplicateRow) && !row_targets.is_empty() {
        subs.push(duplicate_row_strategy(row_targets));
    }

    if subs.is_empty() {
        panic!("mutation_strategy: config + witness produced no candidate mutations");
    }

    Union::new(subs).boxed()
}

fn snapshot<F: TowerField>(
    witness: &ProgramWitness<F, ColumnTrace>,
    config: &ScribbleConfig,
) -> Vec<(Target, usize, Vec<ColumnType>)> {
    let mut out = Vec::new();
    if config.is_target_allowed(Target::Main) {
        push_shape(&mut out, Target::Main, &witness.trace);
    }

    for (i, ct) in witness.chiplet_traces.iter().enumerate() {
        let target = Target::Chiplet(i);
        if config.is_target_allowed(target) {
            push_shape(&mut out, target, ct);
        }
    }

    out
}

fn push_shape(
    out: &mut Vec<(Target, usize, Vec<ColumnType>)>,
    target: Target,
    trace: &ColumnTrace,
) {
    if trace.columns.is_empty() {
        return;
    }

    let num_rows = trace.columns[0].len();
    let col_types: Vec<ColumnType> = trace.columns.iter().map(|c| c.column_type()).collect();

    out.push((target, num_rows, col_types));
}

fn cells_for(
    kind: MutationKind,
    shapes: &[(Target, usize, Vec<ColumnType>)],
    config: &ScribbleConfig,
) -> Vec<Cell> {
    let mut out = Vec::new();
    for (target, num_rows, col_types) in shapes {
        if *num_rows == 0 {
            continue;
        }

        for (col, t) in col_types.iter().enumerate() {
            if !config.is_col_allowed(col) {
                continue;
            }

            if !kind_compatible(kind, *t) {
                continue;
            }

            out.push(Cell {
                target: *target,
                col,
                col_type: *t,
                num_rows: *num_rows,
            });
        }
    }

    out
}

fn kind_compatible(kind: MutationKind, t: ColumnType) -> bool {
    match kind {
        MutationKind::FlipSelector => matches!(t, ColumnType::Bit),
        _ => true,
    }
}

fn col_mask(t: ColumnType) -> u128 {
    match t {
        ColumnType::Bit => 1,
        ColumnType::B8 => 0xFF,
        ColumnType::B16 => 0xFFFF,
        ColumnType::B32 => 0xFFFF_FFFF,
        ColumnType::B64 => 0xFFFF_FFFF_FFFF_FFFF,
        ColumnType::B128 => u128::MAX,
    }
}

fn bitflip_strategy(cells: Vec<Cell>) -> BoxedStrategy<Mutation> {
    select(cells)
        .prop_flat_map(|cell| {
            let max = col_mask(cell.col_type);
            (Just(cell), 0..cell.num_rows, 1u128..=max)
        })
        .prop_map(|(cell, row, mask)| Mutation::BitFlip {
            target: cell.target,
            col: cell.col,
            row,
            mask,
        })
        .boxed()
}

fn out_of_bounds_strategy(cells: Vec<Cell>) -> BoxedStrategy<Mutation> {
    select(cells)
        .prop_flat_map(|cell| (Just(cell), 0..cell.num_rows, any::<u128>()))
        .prop_map(|(cell, row, value)| Mutation::OutOfBounds {
            target: cell.target,
            col: cell.col,
            row,
            value,
        })
        .boxed()
}

fn flip_selector_strategy(cells: Vec<Cell>) -> BoxedStrategy<Mutation> {
    select(cells)
        .prop_flat_map(|cell| (Just(cell), 0..cell.num_rows))
        .prop_map(|(cell, row)| Mutation::FlipSelector {
            target: cell.target,
            col: cell.col,
            row,
        })
        .boxed()
}

fn swap_rows_strategy(targets: Vec<(Target, usize)>) -> BoxedStrategy<Mutation> {
    select(targets)
        .prop_flat_map(|(target, num_rows)| (Just(target), 0..num_rows, 0..num_rows))
        .prop_filter("row_a != row_b", |(_, a, b)| a != b)
        .prop_map(|(target, row_a, row_b)| Mutation::SwapRows {
            target,
            row_a,
            row_b,
        })
        .boxed()
}

fn duplicate_row_strategy(targets: Vec<(Target, usize)>) -> BoxedStrategy<Mutation> {
    select(targets)
        .prop_flat_map(|(target, num_rows)| (Just(target), 0..num_rows, 0..num_rows))
        .prop_filter("src_row != dst_row", |(_, s, d)| s != d)
        .prop_map(|(target, src_row, dst_row)| Mutation::DuplicateRow {
            target,
            src_row,
            dst_row,
        })
        .boxed()
}

fn column_uniform_write_strategy(cells: Vec<Cell>) -> BoxedStrategy<Mutation> {
    select(cells)
        .prop_flat_map(|cell| {
            let max = col_mask(cell.col_type);
            (Just(cell), 0u128..=max)
        })
        .prop_map(|(cell, value)| Mutation::ColumnUniformWrite {
            target: cell.target,
            col: cell.col,
            value,
        })
        .boxed()
}

fn monotonic_replace_strategy(cells: Vec<Cell>) -> BoxedStrategy<Mutation> {
    select(cells)
        .prop_flat_map(|cell| {
            let max = col_mask(cell.col_type);
            (Just(cell), 0u128..=max, 0u128..=max)
        })
        .prop_map(|(cell, base, step)| Mutation::MonotonicReplace {
            target: cell.target,
            col: cell.col,
            base,
            step,
        })
        .boxed()
}

fn segments_for(
    shapes: &[(Target, usize, Vec<ColumnType>)],
    config: &ScribbleConfig,
) -> Vec<Segment> {
    shapes
        .iter()
        .filter_map(|(target, num_rows, col_types)| {
            let allowed_cols: Vec<usize> = (0..col_types.len())
                .filter(|c| config.is_col_allowed(*c))
                .collect();
            if allowed_cols.is_empty() || *num_rows == 0 {
                return None;
            }
            Some(Segment {
                target: *target,
                allowed_cols,
                num_rows: *num_rows,
            })
        })
        .collect()
}

fn row_segment_zero_strategy(segments: Vec<Segment>) -> BoxedStrategy<Mutation> {
    select(segments)
        .prop_flat_map(|seg| {
            let n_cols = seg.allowed_cols.len();
            let col_idx = proptest::collection::vec(0..n_cols, 1..=n_cols.min(8));
            let rows = proptest::collection::vec(0..seg.num_rows, 1..=seg.num_rows.min(8));

            (Just(seg), rows, col_idx)
        })
        .prop_map(|(seg, mut rows, idx)| {
            let mut cols: Vec<usize> = idx.into_iter().map(|i| seg.allowed_cols[i]).collect();

            rows.sort_unstable();
            rows.dedup();

            cols.sort_unstable();
            cols.dedup();

            Mutation::RowSegmentZero {
                target: seg.target,
                rows,
                cols,
            }
        })
        .boxed()
}
