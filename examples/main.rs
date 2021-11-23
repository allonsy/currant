fn main() {
    let dir: Option<String> = None;
    let commands = vec![
        currant::StandardOutCommand::new_command_string_with_color(
            "test1",
            "ls -la .",
            dir.clone(),
            currant::Color::BLUE,
        )
        .unwrap(),
        currant::StandardOutCommand::new_command_string_with_color(
            "test2",
            "ls -la ..",
            dir.clone(),
            currant::Color::RED,
        )
        .unwrap(),
        currant::StandardOutCommand::new_command_string_with_color(
            "test3",
            "ls -la ../..",
            dir,
            currant::Color::GREEN,
        )
        .unwrap(),
    ];

    let mut opts = currant::Options::new();
    opts.restart(currant::RestartOptions::Kill);

    let handle = currant::run_commands_stdout(commands);
    handle.join();
}
