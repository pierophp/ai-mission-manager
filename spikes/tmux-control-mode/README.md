# Local tmux control-mode spike

Result: **yes**. tmux 3.7c can attach a control-mode client to a Pane created outside the app. `%output` notifications carry the Pane's terminal bytes, `send-keys -H` sends ordinary input and control bytes back to that same Pane, and `refresh-client -C columns,rows` changes its size. The proof keeps the original Pane ID and process PID across attach, interaction, resize, and detach.

This is disposable spike code for issue #2, not the production runtime adapter.

## Run the proof

Install the local tools and dependencies once:

```sh
brew install tmux
npm install
```

Start a Pane outside the bridge. The command below deliberately creates the session before the bridge starts:

```sh
tmux -f /dev/null -L mission-spike new-session -d -s proof sh -c 'printf "READY\n"; while IFS= read -r line; do printf "ECHO:%s\n" "$line"; done'
PANE=$(tmux -f /dev/null -L mission-spike list-panes -t proof -F '#{pane_id}')
```

Start the disposable browser bridge and open `http://127.0.0.1:4173`:

```sh
npm run build
node dist/src/server.js --socket mission-spike --session proof --pane "$PANE"
```

The view attaches to the supplied Pane. Type into it, press Ctrl-C, and resize the browser window. Closing the bridge detaches its control-mode client; it does not kill the tmux session or the Pane process.

The live proof is also automated:

```sh
npm test
```

## What the proof established

The adapter seam is intentionally small:

```text
attach(Machine/session/Pane identity)
  -> output: bytes
  -> sendInput(bytes)
  -> resize(columns, rows)
  -> snapshot(): bytes
  -> close()
```

The tmux implementation owns the control-mode child process and hides its protocol. Callers use the stable `%N` Pane identity and never resolve a Pane by its display name or numeric position. The later domain adapter can carry the Machine, session and Pane identity without exposing tmux commands to the domain.

The control-mode client uses one `-C` flag because the app owns pipes rather than a controlling terminal. `-CC` is the mode intended for a full terminal frontend, but tmux requires a tty for it; the pipe-based `-C` mode still supports the application path by sending input through `send-keys -H`. This is a transport detail for the tmux adapter, not a domain concern.

The browser bridge uses Server-Sent Events for output and small HTTP POSTs for input and resize. It is only a visual harness; the production Tauri shell should call the same adapter directly. `snapshot()` uses `capture-pane -p -e` to seed an xterm.js view with the current screen because `%output` only reports bytes produced after the control-mode client attaches.

## Remote result (issue #3)

Result: **transport path passed in this environment**. The same adapter now starts `ssh -T` with a remote `tmux -C attach-session` command. The live proof confirms that a remote Pane streams output, accepts ordinary input and Ctrl-C, resizes through `refresh-client -C`, and keeps the same Pane ID and process PID. It then kills the remote control-mode client, verifies that the remote session and Pane remain, reconnects over SSH, and receives input again. The remote tmux server reports one session throughout, so attaching does not create a second remote session.

The automated proof starts a real, temporary `sshd` on loopback with an ephemeral keypair. This proves the SSH transport and reconnect seam in this environment, but it is not a test against a separate physical Machine. Set `TMUX_REMOTE_PATH` when the remote tmux binary is not at the same absolute path as the local binary. The test requires `sshd`, `ssh-keygen`, and `tmux`.

The browser bridge remains transport-agnostic: it consumes the same `TmuxControlPane` output/input/resize interface that the local proof exercises. The remote-specific work is isolated to the process-launch adapter, so the terminal view does not need a local-versus-remote branch.

## Important edges

- `%output` payloads escape control bytes and backslashes as octal sequences; the adapter decodes them into bytes before handing them to xterm.js.
- A control-mode client attached to a session receives notifications for every Pane in that session, so the adapter filters notifications by stable Pane ID.
- `refresh-client -C` is the resize operation; ordinary window-size settings do not resize a control-mode client.
- In a split tmux window, tmux allocates the resized client dimensions across the existing layout; the selected Pane therefore receives its layout share rather than the full browser width. The proof uses a one-Pane session. A production multi-Pane view should expose the actual Pane geometry or choose a layout policy before presenting a full-width xterm.
- The spike does not validate latency, firewall behavior, or a remote Machine with a different operating system; those remain deployment checks for a real Machine.
