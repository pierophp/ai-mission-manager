# No separate daemon in v0

The original design had a `missiond` daemon behind a Unix socket with the desktop app as its client, so polling and reminders could run while the app was closed. v0 collapses that into the Tauri backend: one process, no IPC protocol of our own, no LaunchAgent. Because the domain core carries no Tauri dependency, extracting the daemon later is a packaging change rather than a rewrite — we will do it when "notify me while the app is closed" is a real need rather than an anticipated one.
