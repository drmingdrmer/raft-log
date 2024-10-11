use std::fmt::Debug;

use codeq::Segment;

use crate::ChunkId;

pub trait StateMachine<R> {
    type Error: std::error::Error + Debug + 'static;

    fn apply(
        &mut self,
        record: &R,
        chunk_id: ChunkId,
        segment: Segment,
    ) -> Result<(), Self::Error>;
}
