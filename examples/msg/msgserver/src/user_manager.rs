use anyhow::Result;
use lazy_static::*;
use netxserver::prelude::Actor;
use packer::User;

lazy_static! {
    //为了方便实现,使用了actor
    //For the convenience of implementation, actor is used
    pub static ref USERMANAGER:Actor<UserManager>={
        let usermanager=Actor::new(UserManager::new());
        usermanager
    };
}

// 用户管理器
// user manager
pub struct UserManager {
    users: Vec<User>,
}

impl UserManager {
    pub fn new() -> UserManager {
        UserManager {
            users: Default::default(),
        }
    }

    pub fn check_nickname(&self, nickname: String) -> bool {
        for user in self.users.iter() {
            if user.nickname == nickname {
                return false;
            }
        }
        return true;
    }

    pub fn add(&mut self, user: User) {
        self.users.push(user)
    }

    pub fn find(&self, sessionid: i64) -> Option<User> {
        for user in self.users.iter() {
            if user.sessionid == sessionid {
                return Some(user.clone());
            }
        }
        None
    }

    pub fn find_by_nickname(&self, nickname: String) -> Option<User> {
        for user in self.users.iter() {
            if user.nickname == nickname {
                return Some(user.clone());
            }
        }
        None
    }

    pub fn remove(&mut self, sessionid: i64) -> Option<User> {
        for (index, user) in self.users.iter().enumerate() {
            if user.sessionid == sessionid {
                return Some(self.users.remove(index));
            }
        }
        None
    }

    pub fn get_users(&self) -> Vec<User> {
        self.users.clone()
    }
}

#[async_trait::async_trait]
pub trait IUserManager {
    async fn add(&self, user: User) -> Result<()>;
    async fn find(&self, sessionid: i64) -> Result<Option<User>>;
    async fn find_by_nickname(&self, nickname: String) -> Result<Option<User>>;
    async fn remove(&self, sessionid: i64) -> Result<Option<User>>;
    async fn get_users(&self) -> Vec<User>;
    async fn check_nickname(&self, nickname: String) -> Result<bool>;
}

#[async_trait::async_trait]
impl IUserManager for Actor<UserManager> {
    #[inline]
    async fn add(&self, user: User) -> Result<()> {
        self.inner_call(|inner| async move {
            inner.get_mut().add(user);
            Ok(())
        })
        .await
    }
    #[inline]
    async fn find(&self, sessionid: i64) -> Result<Option<User>> {
        self.inner_call(|inner| async move { Ok(inner.get_mut().find(sessionid)) })
            .await
    }
    #[inline]
    async fn find_by_nickname(&self, nickname: String) -> Result<Option<User>> {
        self.inner_call(|inner| async move { Ok(inner.get_mut().find_by_nickname(nickname)) })
            .await
    }

    #[inline]
    async fn remove(&self, sessionid: i64) -> Result<Option<User>> {
        self.inner_call(|inner| async move { Ok(inner.get_mut().remove(sessionid)) })
            .await
    }
    #[inline]
    async fn get_users(&self) -> Vec<User> {
        unsafe { self.deref_inner().get_users() }
    }
    #[inline]
    async fn check_nickname(&self, nickname: String) -> Result<bool> {
        self.inner_call(|inner| async move { Ok(inner.get().check_nickname(nickname)) })
            .await
    }
}
