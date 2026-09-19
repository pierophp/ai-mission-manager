# Remote execution runs control mode over SSH

A remote Machine is reached by running tmux's control mode across an SSH connection. This is a supported path rather than a workaround, and it yields the same pushed stream of Pane output as a local session, so the domain holds no notion of local versus remote: a Machine is simply a runtime connection.

## Consequences

Remote execution costs little beyond the connection itself, which is why v0 keeps it while dropping three of the four external providers. A Machine also fails in exactly one way — the connection is down — rather than through a separate remote code path with its own failure modes.
