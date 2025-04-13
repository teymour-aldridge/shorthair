#[derive(Debug, Clone, Copy)]
pub enum ResourceRef {
    Group(GroupRef),
    Spar(SparRef),
    SparSeries(SparSeriesRef),
    User(UserRef),
    SparRoom(SparRoomRef),
    UserBallot(UserBallotRef),
}

#[derive(Debug, Clone, Copy)]
pub struct GroupRef(pub i64);

#[derive(Debug, Clone, Copy)]
pub struct SparRef(pub i64);

#[derive(Debug, Clone, Copy)]
pub struct SparSeriesRef(pub i64);

#[derive(Debug, Clone, Copy)]
pub struct UserRef(pub i64);

#[derive(Debug, Clone, Copy)]
pub struct SparRoomRef(pub i64);

#[derive(Debug, Clone, Copy)]
pub struct UserBallotRef(pub i64);
