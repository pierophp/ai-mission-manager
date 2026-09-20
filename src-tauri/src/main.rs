fn main() {
    let mut args = std::env::args();
    let _executable = args.next();
    if args.next().as_deref() == Some("--agent-state-hook") {
        if let Err(error) = ai_mission_manager_lib::run_agent_state_hook(&args.collect::<Vec<_>>())
        {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return;
    }
    ai_mission_manager_lib::run();
}
