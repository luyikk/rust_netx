use super::SessionSave;

#[derive(Default)]
pub struct DefaultSessionStore {
    sessionid: i64,
}

impl SessionSave for DefaultSessionStore {
    fn get_session_id(&self) -> i64 {
        self.sessionid
    }
    fn store_session_id(&mut self, session_id: i64) {
        self.sessionid = session_id
    }
}
