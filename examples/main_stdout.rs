use currant::Color;
use currant::CommandOperations;
use currant::ConsoleCommand;
use currant::Runner;
fn main() {
    let handle = Runner::new()
        .command(
            ConsoleCommand::from_string("test1", "ls -la .")
                .unwrap()
                .color(Color::BLUE),
        )
        .command(
            ConsoleCommand::from_string("test2", "ls -la ..")
                .unwrap()
                .color(Color::RED),
        )
        .command(
            ConsoleCommand::from_string("test3", "ls -la ../..")
                .unwrap()
                .color(Color::GREEN),
        )
        .execute();
    handle.join().unwrap();
}
