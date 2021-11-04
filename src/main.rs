mod currant;

use std::fs::File;

fn main() {
    let outfile = File::create("output.txt").unwrap();
    let commands = vec![
        currant::Command::new_command_string(
            "test1",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test1.sh",
        ),
        currant::Command::new_command_string(
            "test2",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test2.sh",
        ),
        currant::Command::new_command_string(
            "test3",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test3.sh",
        ),
    ];

    let mut opts = currant::Options::new();
    opts.restart(currant::RestartOptions::Kill);

    let handle = currant::run_commands_writer(commands, outfile);
    handle.join();
}
