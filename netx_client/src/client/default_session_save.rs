use super::SessionSave;

#[derive(Default)]
pub struct DefaultSessionStore{
    sessionid:i64
}

impl SessionSave for DefaultSessionStore{
    fn get_sessionid(&self) -> i64 {
        self.sessionid
    }
    fn store_sessionid(&mut self,sessionid:i64) {
        self.sessionid=sessionid
    }
}
