# Remote execution runs control mode over SSH

A remote Machine is reached by running tmux's control mode across an SSH connection. This is a supported path rather than a workaround, and it yields the same pushed stream of Pane output as a local session, so the domain holds no notion of local versus remote: a Machine is simply a runtime connection.

## Consequences

Remote execution costs little beyond the connection itself, which is why v0 keeps it while dropping three of the four external providers. A Machine also fails in exactly one way — the connection is down — rather than through a separate remote code path with its own failure modes.

## Spike result

Issue #3 validated the SSH/control-mode transport portion of this decision on 2026-09-19. A real SSH client streamed tmux control mode from an existing remote Pane, delivered output and input through the same adapter seam as the local proof, resized the Pane, and recovered by attaching again after the control connection was dropped. The Pane ID and process PID stayed stable, and the remote tmux server retained one session, so attaching did not create a second remote session.

The proof used a real temporary `sshd` on loopback because no separate remote Machine was configured in the development environment. It therefore validates the SSH/control-mode transport and the no-new-session behavior, but not network latency, firewall policy, or differences on another Machine's operating system. The decision remains supported for v0; those deployment conditions are operational checks rather than a new domain path.
