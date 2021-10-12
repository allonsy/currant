use std::thread;
use std::time;
mod currant;

fn main() {
    let commands = vec![
        currant::Command::new("lscur".to_string(), "ls .".to_string()),
        currant::Command::new("lspar".to_string(), "ls ..".to_string()),
    ];

    currant::run_commands(commands);
    thread::sleep(time::Duration::from_secs(10));
}
