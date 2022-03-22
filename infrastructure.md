# Infrastructure and Design

## Walkthrough of Execution
In order to walk through the design of currant, let's suppose that currant is called with some number of commands.
Firstly, currant spawns a supervisor thread which is in charge of orchestrating all the commands. This supervisor thread creates the resources that all the subcommands need.
This includes the channel for sending messages and the kill barrier (see the kill barrier section for details on that structure).
For each command, a new thread is spawned from within the supervisor thread. This new thread spawns the desired shell subprocess and listens on standard out and standard error as well as listening for command termination. 
In an infinite loop, this thread listens to three types of commands: Standard out bytes, Standard Error bytes, and command termination. Upon receiving data from stdout or stdin, the thread passes along that payload to the channel. 
Upon receiving an exit status, the thread will also pass that along. However, if the restart condition is `Restart` then the subprocess is respawned if the exit status isn't success (`0`). If the restart condition is `Kill` and the exit status is non-zero, the kill barrier is activated and all other threads are killed.

### Joining
If the user wants to join on the sub commands (wait for completion), then, the user calls the `join` method on the `Runner` struct. 
This calls `join` on the handle on the supervisor thread.
Internally, the supervisor maintains a list of all the children thread that are running the subshells. Each child thread will only exit when the underlying subprocess completes (not including restarts due to failed status codes). The supervisor thread, after spawning all the child threads, will join on all child threads and only then, will exit, causing the user to wake up from the join.

### Killing
If the user wishes to manually kill all the commands, the user can call the `kill` method on the `Runner` struct.
This causes the kill trigger to be initiated from the supervisor thread. Note that this doesn't wait on the child processes. It merely initiates the shutdown signal. In order to be sure that all child processes have completed, it is necessary to still call `join` on the `Runner` struct. 

## Kill Barriers
In the program, kill barriers are used to signal child threads that they need to kill the underlying process. 
Basically, a kill barrier is a combination of the barrier and conditional variable (condvar) synchronization primitives.
A kill barrier contains two parts, a barrier and a kill switch. Any thread which waits at the barrier will wait perpetually until the kill switch is "thrown" at which point all threads are let through and any future threads that wait on the barrier also return immeditately. 
The kill barrier is implemented with an `mpsc` channel and a mutex lock. 
There is a kill barrier supervisor thread which locks the mutex lock and waits on the mpsc channel. Then, for every child thread, there is a kill barrier thread spawned. This kill barrier thread contains a handle to the spawned subprocess. The kill barrier child thread then waits on the mutex lock. Of course, since the kill barrier supervisor thread which holds the lock so all child kill barrier threads will wait indefinitely. 
When the kill switch is thrown, either by the call to `kill` by the user or if a subprocess failed under `RestartOptions::Kill`, then a message is sent on the channel. This wakes up the kill barrier supervisor thread which then releases the lock. This causes a cascade in all the child processes, each one aquiring the lock, killing the underlying child subprocess and then releasing the lock. 
Any future child kill barriers that are spawned will see the lock in the unlocked state and will immeditately kill the corresponding child process. 