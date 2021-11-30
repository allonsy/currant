fn main() {
    let commands = vec![
        currant::ConsoleCommand::full_cmd("test1", "ls -la .")
            .unwrap()
            .color(currant::Color::BLUE),
        currant::ConsoleCommand::full_cmd("test2", "ls -la ..")
            .unwrap()
            .color(currant::Color::RED),
        currant::ConsoleCommand::full_cmd("test3", "ls -la ../..")
            .unwrap()
            .color(currant::Color::GREEN),
    ];

    let mut opts = currant::Options::new();
    opts.restart(currant::RestartOptions::Kill);

    let handle = currant::run_commands_stdout(commands);
    handle.join().unwrap();
}
