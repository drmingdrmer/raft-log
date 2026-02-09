use std::fs;
use std::io;
use std::sync::Arc;
use std::sync::mpsc::sync_channel;

use raft_log::Config;
use raft_log::RaftLog;
use raft_log::Types;
use raft_log::api::raft_log_writer::RaftLogWriter;

const DATA_DIR: &str = "tests/compat";
const THIS_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Eq)]
#[derive(Default)]
pub(crate) struct MyType;

impl Types for MyType {
    type LogId = (u64, u64);
    type LogPayload = String;
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

#[test]
fn test_compat() -> Result<(), io::Error> {
    let versions = fs::read_dir(DATA_DIR)?
        .filter_map(|r| r.ok())
        .map(|dir| dir.path())
        .filter(|p| p.is_dir())
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect::<Vec<_>>();

    for version in versions {
        test_version_write_compat(&version)?;
        test_version_read_compat(&version)?;
    }
    Ok(())
}

/// Generate the sample data for this version.
///
/// Every new version should run this function once to make sure newer versions
/// can read data built by this version.
#[ignore]
#[test]
fn generate_data() -> Result<(), io::Error> {
    let version_data_dir = get_version_dir(THIS_VERSION);
    println!("Generating data for version: {}", version_data_dir);

    fs::create_dir_all(format!("{}/raft-log", version_data_dir))?;

    create_raft_log(&version_data_dir)?;

    Ok(())
}

/// Test this version generate the same data with a specific version.
///
/// This function:
/// 1. Creates a temporary directory and generates sample raft log data
/// 2. Compares the generated files with reference files from a specific version
/// 3. Verifies that all files match in both content and names
fn test_version_write_compat(version: &str) -> Result<(), io::Error> {
    println!(
        "Testing compatibility with version: {} (current: {})",
        version, THIS_VERSION
    );
    let version_data_dir = get_version_dir(version);

    let temp_dir = tempfile::tempdir()?;
    let temp_path = temp_dir.path().to_str().unwrap();
    fs::create_dir_all(format!("{}/raft-log", temp_path))?;
    create_raft_log(temp_path)?;

    let temp_paths = list_wal_files(temp_path)?;

    // Compare all files in the raft log directory
    for filename in &temp_paths {
        let actual = fs::read(temp_dir.path().join(filename))?;
        let want = fs::read(format!("{}/{}", version_data_dir, filename))?;

        println!(
            "comparing file: {} -> {}/{}",
            filename, version_data_dir, filename
        );

        assert_eq!(
            want.as_slice(),
            actual.as_slice(),
            "file content mismatch for version {}: {}",
            version,
            filename
        );
    }

    // Assert that all files in version directory are present in temp_dir
    let data_paths = list_wal_files(&version_data_dir)?;

    assert_eq!(
        data_paths, temp_paths,
        "files in {} and temp_dir should match",
        version_data_dir
    );

    Ok(())
}

/// Test if this version can read data from a specific version.
///
/// This function:
/// 1. Opens the raft log from a specific version's data directory
/// 2. Dumps the data and compares it with expected output
fn test_version_read_compat(version: &str) -> Result<(), io::Error> {
    println!(
        "Testing read compatibility with version: {} (current: {})",
        version, THIS_VERSION
    );

    let version_data_dir = get_version_dir(version);

    let config = Arc::new(Config {
        dir: format!("{}/raft-log", version_data_dir),
        ..Default::default()
    });

    let mut raft_log = RaftLog::<MyType>::open(config)?;
    let dump = dump_raft_log_data(&mut raft_log)?;

    let expected_dump =
        fs::read_to_string(format!("{}/dump.txt", version_data_dir))?;

    assert_eq!(
        expected_dump, dump,
        "dump output mismatch for version {}",
        version
    );

    Ok(())
}

fn create_raft_log(base_dir: &str) -> Result<(), io::Error> {
    let config = Arc::new(Config {
        dir: format!("{}/raft-log", base_dir),
        chunk_max_records: Some(5),
        ..Default::default()
    });

    let mut raft_log = RaftLog::<MyType>::open(config)?;

    let logs = [
        //
        ((1, 0), ss("hi")),
        ((1, 1), ss("hello")),
        ((1, 2), ss("world")),
        ((1, 3), ss("foo")),
    ];
    raft_log.append(logs)?;

    raft_log.truncate(2)?;

    let logs = [
        //
        ((2, 2), ss("world")),
        ((2, 3), ss("foo")),
    ];
    raft_log.append(logs)?;

    raft_log.commit((1, 2))?;
    raft_log.purge((1, 1))?;

    let logs = [
        //
        ((2, 4), ss("world")),
        ((2, 5), ss("foo")),
        ((2, 6), ss("bar")),
        ((2, 7), ss("wow")),
    ];
    raft_log.append(logs)?;

    // Flush the entries to the disk.
    let (tx, rx) = sync_channel(1);
    raft_log.flush(Some(tx))?;
    rx.recv().unwrap()?;

    // Replace the dump generation with new function call
    let dump_content = dump_raft_log_data(&mut raft_log)?;
    fs::write(format!("{}/dump.txt", base_dir), dump_content)?;

    Ok(())
}

/// Dumps the raft log data into a string format
fn dump_raft_log_data(
    raft_log: &mut RaftLog<MyType>,
) -> Result<String, io::Error> {
    let mut dump = raft_log.dump_data();
    let mut res = format!("{:?}\n", dump.state());
    for r in dump.iter() {
        let (log_id, payload) = r?;
        res.push_str(&format!("{:?}: {}\n", log_id, payload));
    }
    Ok(res)
}

fn get_version_dir(version: &str) -> String {
    format!("{}/{}", DATA_DIR, version)
}

fn list_wal_files(dir: &str) -> Result<Vec<String>, io::Error> {
    let mut paths: Vec<_> = fs::read_dir(dir)?
        .filter_map(|r| r.ok())
        .map(|dir| dir.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "wal"))
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    paths.sort();
    Ok(paths)
}

fn ss(x: impl ToString) -> String {
    x.to_string()
}
