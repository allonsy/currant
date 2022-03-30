use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Command;

pub fn exec_exists(exec_name: &str, dir: &Option<PathBuf>) -> bool {
    let mut command = Command::new(exec_name);

    if let Some(path) = dir {
        command.current_dir(path);
    }

    let child = command.spawn();

    match child {
        Ok(mut c) => {
            let _ = c.kill();
            true
        }
        Err(e) => !matches!(e.kind(), ErrorKind::NotFound),
    }
}
