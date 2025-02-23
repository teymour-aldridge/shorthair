use std::ops::Range;

use fuzzcheck::{
    Mutator,
    mutators::{integer_within_range::U64WithinRangeMutator, map::MapMutator},
};

pub type UsizeWithinRangeMutator = impl Mutator<usize>;

#[coverage(off)]
pub fn usize_within_range_mutator(
    range: Range<u64>,
) -> UsizeWithinRangeMutator {
    MapMutator::new(
        U64WithinRangeMutator::new(range),
        |out: &usize| u64::try_from(*out).ok(),
        // don't run this on 32-bit systems
        |u64: &u64| *u64 as usize,
        |_, cplx| cplx,
    )
}
