use currant::CommandOperations;
fn main() {
    let mut first = currant::ConsoleCommand::full_cmd("test1", "ls -la .").unwrap();

    first.color(currant::Color::BLUE);
    let mut second = currant::ConsoleCommand::full_cmd("test2", "ls -la ..").unwrap();
    second.color(currant::Color::RED);
    let mut third = currant::ConsoleCommand::full_cmd("test3", "ls -la ../..").unwrap();
    third.color(currant::Color::GREEN);

    let commands = vec![first, second, third];

    let handle = currant::run_commands_stdout(commands);
    handle.join().unwrap();
}
