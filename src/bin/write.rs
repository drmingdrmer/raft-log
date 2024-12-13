#![allow(unused_imports)]

use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
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

fn main() -> Result<(), io::Error> {
    let args = Args::parse();
    let path = args.path.to_str().unwrap().to_string();

    println!("path: {}", path);

    let f =
        OpenOptions::new().read(true).write(true).create(true).open(&path)?;
    let mut f = Arc::new(f);

    let sf = f.clone();

    let _th = thread::spawn(move || loop {
        sf.sync_data().unwrap();
    });

    let mut buf = [0xffu8; 113];
    buf[0] = 1;
    loop {
        f.write_all(&buf)?;
    }
}
