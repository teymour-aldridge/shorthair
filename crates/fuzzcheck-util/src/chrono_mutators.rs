use chrono::{DateTime, NaiveDateTime};
use fuzzcheck::{mutators::map::MapMutator, DefaultMutator, Mutator};

pub type NaiveDateTimeMutator = impl Mutator<NaiveDateTime>;

#[coverage(off)]
#[define_opaque(NaiveDateTimeMutator)]
pub fn naive_date_time_mutator() -> NaiveDateTimeMutator {
    MapMutator::new(
        i32::default_mutator(),
        |ndt: &NaiveDateTime| i32::try_from(ndt.and_utc().timestamp()).ok(),
        |secs: &i32| {
            DateTime::from_timestamp(*secs as i64, 0)
                .unwrap()
                .naive_utc()
        },
        |_, cplx| cplx,
    )
}
