//! The HTTP API and the DSU motion server.

use super::Service;
use crate::dsu::DsuServer;
use crate::httpapi::{HttpApi, Job};

impl Service {
    /// Start, stop or move the HTTP API to match the settings.
    pub(super) fn apply_http(&mut self) {
        if self.state.http.enabled && self.state.http.token.is_empty() {
            self.update_state(|s| s.http.token = crate::httpapi::new_token());
        }
        let (settings, live) = (&self.state.http, &self.shared_live);
        self.http
            .restart(settings.enabled, "the HTTP API", settings.port, || {
                HttpApi::start(settings, live.clone())
            });
    }

    /// Answer what the HTTP API's clients asked for since the last tick.
    pub(super) fn serve_http(&mut self) {
        let Some(api) = &self.http.running else {
            return;
        };
        let jobs: Vec<Job> = api.jobs().try_iter().collect();
        for job in jobs {
            let Job::Ipc(req, reply) = job;
            let answer = self.handle_ipc(req);
            // As for IPC calls: a script reading the live state right after a command sees
            // its effect.
            self.publish_live();
            let _ = reply.send(answer);
        }
    }

    /// Start, stop or move the DSU server to match the settings.
    pub(super) fn apply_dsu(&mut self) {
        let want = self.state.dsu;
        self.dsu
            .restart(want.enabled, "the motion server", want.port, || {
                DsuServer::start(want)
            });
    }
}
