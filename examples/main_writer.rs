use currant::{Command, Runner, WriterCommand, CURRENT_WORKING_DIRECTORY};
use fs::File;
use std::fs;

fn main() {
    let log_file_name = "test_log.txt";
    let log_file = File::create(log_file_name).unwrap();

    run_cmds(log_file);

    let log_file_contents = std::fs::read(log_file_name).unwrap();

    println!("log file contents: ");
    println!("{}", String::from_utf8_lossy(&log_file_contents));

    fs::remove_file(log_file_name).unwrap();
}

fn run_cmds(file: File) {
    let handle = Runner::new()
        .command(
            WriterCommand::from_string("test1", "ls -la .", CURRENT_WORKING_DIRECTORY).unwrap(),
        )
        .command(
            WriterCommand::from_string("test2", "ls -la ..", CURRENT_WORKING_DIRECTORY).unwrap(),
        )
        .command(
            WriterCommand::from_string("test3", "ls -la ../..", CURRENT_WORKING_DIRECTORY).unwrap(),
        )
        .execute(file);

    handle.join().unwrap();
}
