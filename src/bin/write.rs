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

    let _th = std::thread::Builder::new()
            .name("fdatasync".to_string())
            .spawn(move || {
        let mut s = 0;
        loop {
            if let Err(e) = sf.sync_data() {
                println!("error fdatasync: {}", e);
            }

            s += 1;
            if s % 500 == 0 {
                println!("fdatasync {s}");
            }

        }
    });

    let mut buf = [0xffu8; 113];
    buf[0] = 1;
    let mut i = 0;
    loop {
        f.write_all(&buf)?;
        i += 1;
        if i % 500 == 1 {
            println!("written {i}");
        }
    }
}
