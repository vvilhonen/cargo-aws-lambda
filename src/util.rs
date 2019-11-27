use std::process::Command;

pub trait CommandExt {
    fn status_bool(&mut self) -> bool;
}

impl CommandExt for Command {
    fn status_bool(&mut self) -> bool {
        let result = self.status();
        result.map(|r| r.success()).unwrap_or(false)
    }
}
