#![allow(unused_imports)]

use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

use clap::Parser;
use raft_log::api::raft_log_writer::RaftLogWriter;
use raft_log::Config;
use raft_log::Types;

#[derive(Clone, Debug, PartialEq, Eq, clap::Parser)]
#[clap(about = "dump RaftLog WAL", author)]
pub struct Args {
    #[arg(value_name = "PATH")]
    path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub(crate) struct TestTypes;

impl Types for TestTypes {
    /// (term, index)
    type LogId = (u64, u64);

    type LogPayload = String;
    /// (term, voted_for)
    type Vote = (u64, u64);

    type Callback = std::sync::mpsc::SyncSender<Result<(), io::Error>>;

    type UserData = String;

    fn log_index(log_id: &Self::LogId) -> u64 {
        log_id.1
    }

    fn payload_size(payload: &Self::LogPayload) -> u64 {
        payload.len() as u64
    }
}

fn main() -> Result<(), io::Error> {
    let args = Args::parse();
    let path = args.path.to_str().unwrap().to_string();

    println!("raft-dir path: {}", path);

    let config = Config {
        dir: path.clone(),
        log_cache_max_items: Some(1024 * 1024),
        log_cache_capacity: Some(1024 * 1024 * 1024),
        chunk_max_records: Some(128 * 1024),
        chunk_max_size: Some(256 * 1024 * 1024),
        ..Default::default()
    };

    let config = Arc::new(config);

    let mut rl = raft_log::RaftLog::<TestTypes>::open(config)?;

    let n = 1024 * 1024;
    let step = 500;

    let mut start = Instant::now();

    // let mut rxs = Vec::new();

    for index in 0..n {
        rl.append([((1, index), "foooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo".to_string())])?;
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        rl.flush(tx)?;

        let _ = rx;

        // rx.recv().unwrap()?;

        if index % step == 1 {
            println!("index: {}", index);

            let elapsed = start.elapsed();

            println!(
                "elapsed: {:?}, {:?}/op, {} ops/ms",
                elapsed,
                elapsed / (step as u32),
                step / (elapsed.as_millis() as u64 + 1)
            );

            start = Instant::now();
        }
    }

    // for rx in rxs {
    //     rx.recv().unwrap()?;
    // }

    println!("write done");
    sleep(Duration::from_secs(10));
    Ok(())
}
