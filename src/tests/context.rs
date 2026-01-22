use std::io;
use std::sync::Arc;

use tempfile::TempDir;

use crate::Config;
use crate::raft_log::raft_log::RaftLog;
use crate::testing::TestTypes;

pub(crate) fn new_testing()
-> Result<(TestContext, RaftLog<TestTypes>), io::Error> {
    let ctx = TestContext::new()?;
    let rl = ctx.new_raft_log()?;

    Ok((ctx, rl))
}

pub(crate) struct TestContext {
    pub(crate) config: Config,

    _temp_dir: TempDir,
}

impl TestContext {
    pub(crate) fn new() -> Result<TestContext, io::Error> {
        let temp_dir = tempfile::tempdir()?;

        let config = Config {
            dir: temp_dir.path().to_str().unwrap().to_string(),
            ..Default::default()
        };

        Ok(TestContext {
            config,
            _temp_dir: temp_dir,
        })
    }

    pub(crate) fn config(&self) -> Config {
        self.config.clone()
    }

    pub(crate) fn arc_config(&self) -> Arc<Config> {
        Arc::new(self.config.clone())
    }

    pub(crate) fn new_raft_log(&self) -> Result<RaftLog<TestTypes>, io::Error> {
        RaftLog::<TestTypes>::open(Arc::new(self.config()))
    }
}
