use std::io::ErrorKind;
use std::process::Command;

pub fn exec_exists(exec_name: &str) -> bool {
    let child = Command::new(exec_name).spawn();

    match child {
        Ok(mut c) => {
            let _ = c.kill();
            true
        }
        Err(e) => !matches!(e.kind(), ErrorKind::NotFound),
    }
}
