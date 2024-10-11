use std::io;
use std::sync::mpsc::SyncSender;

pub trait Callback {
    fn send(self, res: Result<(), io::Error>);
}

impl Callback for SyncSender<Result<(), io::Error>> {
    fn send(self, res: Result<(), io::Error>) {
        let _ = SyncSender::send(&self, res);
    }
}
