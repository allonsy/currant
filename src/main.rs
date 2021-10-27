mod currant;

fn main() {
    let commands = vec![
        currant::Command::new(
            "test1",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test1.sh",
            vec![""],
        ),
        currant::Command::new(
            "test2",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test2.sh",
            vec![""],
        ),
        currant::Command::new(
            "test3",
            "/home/alecsnyder/Projects/git/github.com/allonsy/currant/test3.sh",
            vec![""],
        ),
    ];

    let mut opts = currant::Options::new();
    opts.restart(currant::RestartOptions::Kill);

    let handle = currant::run_commands(commands, opts);
    let recv = handle.get_output_channel();
    let mut num_done = 0;

    loop {
        if num_done >= 3 {
            break;
        }
        let msg = recv.recv().unwrap();

        match msg.message {
            currant::OutputMessagePayload::Done(status) => {
                println!(
                    "currant: process {} exited with code: {:?}",
                    msg.name, status
                );
                num_done += 1;
            }
            currant::OutputMessagePayload::Stdout(_, line) => {
                println!("{} (o): {}", msg.name, String::from_utf8(line).unwrap());
            }
            currant::OutputMessagePayload::Stderr(_, line) => {
                println!("{} (e): {}", msg.name, String::from_utf8(line).unwrap());
            }
            currant::OutputMessagePayload::Error(e) => {
                println!("currant (e): {}", e);
            }
        }
    }
    handle.join();
}
