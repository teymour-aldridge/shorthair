use chrono::{DateTime, NaiveDateTime};
use fuzzcheck::{DefaultMutator, Mutator, mutators::map::MapMutator};

pub type NaiveDateTimeMutator = impl Mutator<NaiveDateTime>;

#[coverage(off)]
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
