mod currant;
use std::fs::File;

fn main() {
    let newfile = File::create("outputlscur.txt").unwrap();
    let commands = vec![
        currant::Command::new(
            "lscur".to_string(),
            "ls".to_string(),
            vec!["-l".to_string(), ".".to_string()],
            currant::Output::File(newfile),
        ),
        currant::Command::new(
            "lspar".to_string(),
            "ls".to_string(),
            vec!["-l".to_string(), "..".to_string()],
            currant::Output::Stdout,
        ),
    ];

    let handle = currant::run_commands(commands);
    handle.join();
}
