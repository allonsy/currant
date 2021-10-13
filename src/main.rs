mod currant;

fn main() {
    let commands = vec![
        currant::Command::new(
            "lscur".to_string(),
            "ls".to_string(),
            vec!["-l".to_string(), ".".to_string()],
        ),
        currant::Command::new(
            "lspar".to_string(),
            "ls".to_string(),
            vec!["-l".to_string(), "..".to_string()],
        ),
    ];

    let handle = currant::run_commands(commands);
    handle.join();
}
