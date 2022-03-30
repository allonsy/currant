use currant::{Color, Command, ConsoleCommand, Runner, CURRENT_WORKING_DIRECTORY};
fn main() {
    let handle = Runner::new()
        .command(
            ConsoleCommand::from_string("test1", "ls -la .", CURRENT_WORKING_DIRECTORY)
                .unwrap()
                .color(Color::BLUE),
        )
        .command(
            ConsoleCommand::from_string("test2", "ls -la ..", CURRENT_WORKING_DIRECTORY)
                .unwrap()
                .color(Color::RED),
        )
        .command(
            ConsoleCommand::from_string("test3", "ls -la ../..", CURRENT_WORKING_DIRECTORY)
                .unwrap()
                .color(Color::GREEN),
        )
        .execute();
    handle.join().unwrap();
}
