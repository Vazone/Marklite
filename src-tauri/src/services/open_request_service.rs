use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_PENDING_OPEN_REQUESTS: usize = 50;

#[derive(Clone, Default)]
pub struct OpenRequestState {
    pending: Arc<Mutex<VecDeque<String>>>,
}

impl OpenRequestState {
    pub fn enqueue(&self, path: String) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pending.iter().any(|queued| queued == &path) {
            return;
        }
        if pending.len() == MAX_PENDING_OPEN_REQUESTS {
            pending.pop_front();
        }
        pending.push_back(path);
    }

    pub fn drain(&self) -> Vec<String> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenRequestState, MAX_PENDING_OPEN_REQUESTS};

    #[test]
    fn preserves_order_and_delivers_each_path_once() {
        let state = OpenRequestState::default();
        state.enqueue("A.md".into());
        state.enqueue("B.md".into());
        state.enqueue("A.md".into());

        assert_eq!(state.drain(), ["A.md", "B.md"]);
        assert!(state.drain().is_empty());
    }

    #[test]
    fn remains_bounded_and_keeps_the_latest_requests() {
        let state = OpenRequestState::default();
        for index in 0..MAX_PENDING_OPEN_REQUESTS + 2 {
            state.enqueue(format!("{index}.md"));
        }

        let drained = state.drain();
        assert_eq!(drained.len(), MAX_PENDING_OPEN_REQUESTS);
        assert_eq!(drained.first().map(String::as_str), Some("2.md"));
        assert_eq!(
            drained.last().map(String::as_str),
            Some(format!("{}.md", MAX_PENDING_OPEN_REQUESTS + 1).as_str())
        );
    }
}
