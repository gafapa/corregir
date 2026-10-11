//! One local-model request at a time, with a bounded queue and cooperative cancellation.
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    sync::{Arc, Mutex},
};
use tauri::{ipc::Channel, State};
use tokio::sync::{watch, OwnedSemaphorePermit, Semaphore};

#[derive(Serialize)]
pub struct InferenceProgress {
    pub request_id: String,
    pub stage: String,
}

pub struct InferenceJobs {
    active: Mutex<HashMap<String, watch::Sender<bool>>>,
    early_cancelled: Mutex<HashSet<String>>,
    worker: Arc<Semaphore>,
}
impl Default for InferenceJobs {
    fn default() -> Self {
        Self {
            active: Mutex::new(HashMap::new()),
            early_cancelled: Mutex::new(HashSet::new()),
            worker: Arc::new(Semaphore::new(1)),
        }
    }
}

pub struct InferenceJob {
    id: String,
    jobs: Arc<InferenceJobs>,
    cancelled: watch::Receiver<bool>,
    progress: Option<Channel<InferenceProgress>>,
    _permit: Option<OwnedSemaphorePermit>,
}
impl Drop for InferenceJob {
    fn drop(&mut self) {
        if let Ok(mut active) = self.jobs.active.lock() {
            active.remove(&self.id);
        }
    }
}
impl InferenceJobs {
    pub async fn begin(
        self: &Arc<Self>,
        id: String,
        progress: Option<Channel<InferenceProgress>>,
    ) -> Result<InferenceJob, String> {
        if id.is_empty() || id.len() > 128 {
            return Err("invalid inference request ID".into());
        }
        let (sender, cancelled) = watch::channel(false);
        {
            let mut active = self.active.lock().map_err(|error| error.to_string())?;
            if self
                .early_cancelled
                .lock()
                .map_err(|error| error.to_string())?
                .remove(&id)
            {
                return Err("AI request cancelled".into());
            }
            if active.len() >= 4 || active.contains_key(&id) {
                return Err(
                    "the AI queue is full; wait for a request to finish or cancel one".into(),
                );
            }
            active.insert(id.clone(), sender);
        }
        let mut job = InferenceJob {
            id,
            jobs: self.clone(),
            cancelled,
            progress,
            _permit: None,
        };
        job.report("queued");
        let permit = tokio::select! {
            biased;
            _ = job.cancelled.changed() => return Err("AI request cancelled".into()),
            permit = self.worker.clone().acquire_owned() => permit.map_err(|error| error.to_string())?,
        };
        job.check()?;
        job._permit = Some(permit);
        job.report("running");
        Ok(job)
    }
    pub fn cancel(&self, id: &str) -> Result<(), String> {
        if id.is_empty() || id.len() > 128 {
            return Err("invalid inference request ID".into());
        }
        let active = self.active.lock().map_err(|error| error.to_string())?;
        if let Some(sender) = active.get(id) {
            let _ = sender.send(true);
        } else {
            let mut early = self
                .early_cancelled
                .lock()
                .map_err(|error| error.to_string())?;
            if early.len() >= 64 {
                return Err("too many pending cancellation requests".into());
            }
            early.insert(id.into());
        }
        Ok(())
    }
}
impl InferenceJob {
    pub fn report(&self, stage: &str) {
        if let Some(channel) = &self.progress {
            let _ = channel.send(InferenceProgress {
                request_id: self.id.clone(),
                stage: stage.into(),
            });
        }
    }
    pub fn check(&self) -> Result<(), String> {
        if *self.cancelled.borrow() {
            Err("AI request cancelled".into())
        } else {
            Ok(())
        }
    }
    pub async fn run<T>(
        &mut self,
        future: impl Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        self.check()?;
        tokio::select! {
            biased;
            _ = self.cancelled.changed() => Err("AI request cancelled".into()),
            result = future => result,
        }
    }
}

#[tauri::command]
pub fn cmd_cancel_inference(
    jobs: State<'_, Arc<InferenceJobs>>,
    request_id: String,
) -> Result<(), String> {
    jobs.cancel(&request_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn cancellation_before_registration_prevents_the_request_from_starting() {
        let jobs = Arc::new(InferenceJobs::default());
        jobs.cancel("not-yet-started").unwrap();
        assert!(jobs.begin("not-yet-started".into(), None).await.is_err());
        assert!(jobs.active.lock().unwrap().is_empty());
        assert_eq!(jobs.worker.available_permits(), 1);
    }
    #[tokio::test]
    async fn cancelling_running_or_queued_requests_releases_capacity() {
        let jobs = Arc::new(InferenceJobs::default());
        let mut first = jobs.begin("first".into(), None).await.unwrap();
        let queued = {
            let jobs = jobs.clone();
            tokio::spawn(async move { jobs.begin("second".into(), None).await })
        };
        tokio::task::yield_now().await;
        assert_eq!(jobs.worker.available_permits(), 0);
        jobs.cancel("second").unwrap();
        assert!(queued.await.unwrap().is_err());
        jobs.cancel("first").unwrap();
        assert!(first
            .run(std::future::pending::<Result<(), String>>())
            .await
            .is_err());
        drop(first);
        let next = jobs.begin("next".into(), None).await.unwrap();
        assert_eq!(jobs.active.lock().unwrap().len(), 1);
        drop(next);
        assert_eq!(jobs.worker.available_permits(), 1);
    }
    #[tokio::test]
    async fn cancelling_after_http_starts_drops_the_request_future() {
        let jobs = Arc::new(InferenceJobs::default());
        let mut job = jobs.begin("active".into(), None).await.unwrap();
        let cancel = jobs.clone();
        let response =
            tokio::spawn(
                async move { job.run(std::future::pending::<Result<(), String>>()).await },
            );
        tokio::task::yield_now().await;
        cancel.cancel("active").unwrap();
        assert!(response.await.unwrap().is_err());
        assert!(jobs.active.lock().unwrap().is_empty());
    }
}
