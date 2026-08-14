mod model;
mod provider;

use provider::{AgentProvider, ClaudeProvider};

fn main() -> std::process::ExitCode {
    let provider = ClaudeProvider;
    match provider.list_sessions() {
        Ok(sessions) => {
            for session in sessions {
                println!("{} [{:?}] {}", session.id, session.kind, session.name);
            }
            std::process::ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("failed to list sessions: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}
