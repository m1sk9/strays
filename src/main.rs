mod model;
mod provider;

use provider::{AgentProvider, ClaudeProvider};

fn main() {
    let provider = ClaudeProvider;
    match provider.list_sessions() {
        Ok(sessions) => {
            for session in sessions {
                println!("{} [{:?}] {}", session.id, session.kind, session.name);
            }
        }
        Err(err) => eprintln!("failed to list sessions: {err}"),
    }
}
