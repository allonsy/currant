mod currant;

fn main() {
    let commands = vec![
        currant::StandardOutCommand::new_command_string_with_color(
            "test1",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test1.sh",
            currant::Color::Blue,
        ),
        currant::StandardOutCommand::new_command_string_with_color(
            "test2",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test2.sh",
            currant::Color::Red,
        ),
        currant::StandardOutCommand::new_command_string_with_color(
            "test3",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test3.sh",
            currant::Color::Green,
        ),
    ];

    let mut opts = currant::Options::new();
    opts.restart(currant::RestartOptions::Kill);

    let handle = currant::run_commands_stdout(commands);
    handle.join();
}
