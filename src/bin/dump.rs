use std::io;
// use std::io::stdout;
use std::path::PathBuf;

// use clap::Parser;
// use raft_log::dump_writer;
// use raft_log::Config;
// use raft_log::RaftLog;

#[derive(Clone, Debug, PartialEq, Eq, clap::Parser)]
#[clap(about = "dump RaftLog WAL", author)]
pub struct Args {
    #[arg(value_name = "PATH")]
    path: PathBuf,
}

fn main() -> Result<(), io::Error> {
    // let args = Args::parse();
    //
    // let config = Config::new(args.path.to_str().unwrap().to_string());
    //
    // RaftLog::dump_with(&config, stdout(), dump_writer::multiline_string)?;

    Ok(())
}
