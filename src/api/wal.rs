use std::io;

use codeq::Segment;

pub trait WAL<R> {
    // type StateMachine: StateMachine<R>;

    fn append(&mut self, rec: &R) -> Result<(), io::Error>;

    fn last_segment(&self) -> Segment;
}
