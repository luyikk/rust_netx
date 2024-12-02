use super::SessionSave;

/// A default implementation of the `SessionSave` trait.
#[derive(Default)]
pub struct DefaultSessionStore {
    /// The session ID.
    session_id: i64,
}

impl SessionSave for DefaultSessionStore {
    /// Retrieves the session ID.
    fn get_session_id(&self) -> i64 {
        self.session_id
    }
    /// Stores the session ID.
    fn store_session_id(&mut self, session_id: i64) {
        self.session_id = session_id
    }
}
