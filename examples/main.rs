fn main() {
    let commands = vec![
        currant::StandardOutCommand::new_command_string("test1", "ls -la .")
            .unwrap()
            .color(currant::Color::BLUE),
        currant::StandardOutCommand::new_command_string("test2", "ls -la ..")
            .unwrap()
            .color(currant::Color::RED),
        currant::StandardOutCommand::new_command_string("test3", "ls -la ../..")
            .unwrap()
            .color(currant::Color::GREEN),
    ];

    let mut opts = currant::Options::new();
    opts.restart(currant::RestartOptions::Kill);

    let handle = currant::run_commands_stdout(commands);
    handle.join();
}
