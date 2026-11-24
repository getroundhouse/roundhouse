use super::{
    ApiError, Arc, Duration, ErrorCode, GitHubRepository, HashSet, Hub, PullRequestNumber, PullRequestWaitUntil,
    PullRequestWatch, PullRequestsSnapshot, ReadPullRequests, START_WITHIN, SessionId, StartPullRequestWait, Wait,
    WaitAnswer, WaitEnd, WaitEnded, WaitUntil, WorkspaceId, new_id, now_ms, remote_rules,
};

impl Hub {
    pub(crate) async fn remote_for_agent(
        self: &Arc<Self>,
        id: &WorkspaceId,
        asked: ReadPullRequests,
    ) -> Result<PullRequestsSnapshot, ApiError> {
        let read = self.read_remote_for_agent(id, asked.pull_requests);
        let (snapshot, _held) = tokio::time::timeout(START_WITHIN, read).await.map_err(|_| too_long())??;
        self.observe_remote_waits(id, &snapshot).await;
        Ok(snapshot)
    }

    pub(crate) async fn start_remote_wait(
        self: &Arc<Self>,
        id: &WorkspaceId,
        session_id: SessionId,
        asked: StartPullRequestWait,
    ) -> Result<WaitAnswer, ApiError> {
        if let Some(started) = self.started_by(&session_id, asked.call.as_ref()).await? {
            return self.with_pull_request(started).await;
        }
        let timeout_ms = timeout_ms(asked.timeout_seconds)?;
        let read = self.read_remote_for_agent(id, asked.pull_requests.clone());
        let (snapshot, _held) = tokio::time::timeout(START_WITHIN, read).await.map_err(|_| too_long())??;
        self.observe_remote_waits(id, &snapshot).await;
        let at_ms = now_ms();
        let deadline_at_ms = timeout_ms
            .map(|ms| {
                at_ms
                    .checked_add(ms)
                    .ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, "timeout_seconds is too large"))
            })
            .transpose()?;
        let number = asked
            .pull_requests
            .numbers()
            .first()
            .ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, "no pull requests"))?
            .get();
        let until = match asked.until {
            PullRequestWaitUntil::RequiredChecks => WaitUntil::RequiredChecks,
            PullRequestWaitUntil::AllChecks => WaitUntil::AllChecks,
            PullRequestWaitUntil::Merged => WaitUntil::Merged,
        };
        let mut wait = Wait {
            id: new_id().into(),
            session_id,
            workspace_id: id.clone(),
            number,
            call: asked.call,
            until,
            started_at_ms: at_ms,
            ended: None,
            watch: Some(Box::new(PullRequestWatch {
                requested: asked.pull_requests,
                baseline: remote_rules::baseline(&snapshot),
                snapshot: snapshot.clone(),
                deadline_at_ms,
                read_error: None,
                last_attempt_at_ms: at_ms,
                affected: Vec::new(),
            })),
        };
        remote_rules::judge(&mut wait, at_ms);
        let wait = self.blocking(move |store| store.start_wait(&wait)).await?;
        self.announce_chat_of(&wait).await;
        self.with_pull_request(wait).await
    }

    pub(in crate::hub) async fn observe_remote_waits(
        self: &Arc<Self>,
        id: &WorkspaceId,
        snapshot: &PullRequestsSnapshot,
    ) {
        let (id, snapshot, at_ms) = (id.clone(), snapshot.clone(), now_ms());
        let changed = self
            .blocking(move |store| {
                store.revise_remote_waits(Some(&id), |wait| {
                    remote_rules::expire(wait, at_ms);
                    remote_rules::observe(wait, &snapshot, at_ms);
                })
            })
            .await;
        if let Ok(waits) = changed {
            self.announce_remote_changes(waits).await;
        }
    }

    /// Records on the workspace's waits that what they watch in `repository`, or in any repository, couldn't be read,
    /// as `error` says, of `affected`, or of every pull request when that's empty. One that may work later leaves them
    /// waiting, and one that says they're gone ends them.
    pub(in crate::hub) async fn remote_waits_failed(
        self: &Arc<Self>,
        id: &WorkspaceId,
        repository: Option<&GitHubRepository>,
        error: &ApiError,
        affected: &[PullRequestNumber],
    ) {
        let affected = affected.to_vec();
        let (id, repository, error, at_ms) = (id.clone(), repository.cloned(), error.clone(), now_ms());
        let changed = self
            .blocking(move |store| {
                store.revise_remote_waits(Some(&id), |wait| {
                    remote_rules::expire(wait, at_ms);
                    if wait.ended.is_some() {
                        return;
                    }
                    let Some(watch) = &mut wait.watch else { return };
                    if repository.as_ref().is_some_and(|repository| watch.snapshot.repository != *repository) {
                        return;
                    }
                    // Those it watches, which are those it was asked for while it has read none, as after an update
                    // emptied what an older hub kept.
                    let watched = watch.numbers();
                    let ours: Vec<_> = affected.iter().copied().filter(|number| watched.contains(number)).collect();
                    if !affected.is_empty() && ours.is_empty() {
                        return;
                    }
                    watch.read_error = Some(error.message.clone());
                    watch.last_attempt_at_ms = at_ms;
                    if matches!(error.code, ErrorCode::Unavailable | ErrorCode::Unsupported | ErrorCode::NotFound) {
                        watch.affected = ours;
                        wait.ended = Some(WaitEnded { at_ms, how: WaitEnd::Unavailable });
                    }
                })
            })
            .await;
        if let Ok(waits) = changed {
            self.announce_remote_changes(waits).await;
        }
    }

    async fn announce_remote_changes(self: &Arc<Self>, waits: Vec<Wait>) {
        let mut announced = HashSet::new();
        let mut woken = HashSet::new();
        for wait in &waits {
            if announced.insert(wait.session_id.clone()) {
                self.announce_chat_of(wait).await;
            }
            if wait.ended.is_some() && woken.insert(wait.session_id.clone()) {
                self.wake_for(wait).await;
            }
        }
    }

    /// Separate from GitHub reads so a stuck network call or a spent limit cannot postpone a deadline.
    #[expect(clippy::disallowed_methods, reason = "how often waits are looked at for any past its deadline")]
    pub(crate) async fn watch_wait_deadlines(self: Arc<Self>) {
        loop {
            let at_ms = now_ms();
            let changed = self.blocking(move |store| store.expire_remote_waits(at_ms)).await;
            if let Ok(waits) = changed {
                self.announce_remote_changes(waits).await;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

fn too_long() -> ApiError {
    ApiError::new(ErrorCode::Unavailable, "the pull requests couldn't be read before the tool's startup deadline")
}

fn timeout_ms(seconds: Option<u64>) -> Result<Option<i64>, ApiError> {
    seconds
        .map(|seconds| {
            if seconds == 0 {
                return Err(ApiError::new(ErrorCode::InvalidRequest, "timeout_seconds must be positive"));
            }
            seconds
                .checked_mul(1000)
                .and_then(|ms| i64::try_from(ms).ok())
                .ok_or_else(|| ApiError::new(ErrorCode::InvalidRequest, "timeout_seconds is too large"))
        })
        .transpose()
}
