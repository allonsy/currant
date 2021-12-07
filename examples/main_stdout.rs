use currant::Color;
use currant::CommandOperations;
use currant::ConsoleCommand;
use currant::Runner;
fn main() {
    let handle = currant::run_commands_stdout(
        Runner::new()
            .command(
                ConsoleCommand::full_cmd("test1", "ls -la .")
                    .unwrap()
                    .color(Color::BLUE),
            )
            .command(
                ConsoleCommand::full_cmd("test2", "ls -la ..")
                    .unwrap()
                    .color(Color::RED),
            )
            .command(
                ConsoleCommand::full_cmd("test3", "ls -la ../..")
                    .unwrap()
                    .color(Color::GREEN),
            ),
    );
    handle.join().unwrap();
}
