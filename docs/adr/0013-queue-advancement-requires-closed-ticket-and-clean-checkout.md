# Queue advancement requires a closed ticket and a clean checkout

An Implementation Queue entry is complete only when the agent turn has ended, its GitHub ticket is closed, and the associated checkout is clean. The agent's `finished` state reports the end of a turn; by itself, it does not establish that the work is complete.

When all three conditions hold, the application marks the entry done, closes its tmux session while retaining the Run in history, and starts the next entry through the normal launch gate. Completing the last entry finishes the queue without changing the Item's state. If the ticket remains open or the checkout is dirty, the queue waits on that entry.
