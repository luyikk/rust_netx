use packer::User;
use netxserver::aqueue::AResult;
use netxserver::Actor;
use netxserver::aqueue;
use lazy_static::*;

lazy_static!{
    pub static ref USERMANAGER:Actor<UserManager>={
        let usermanager=Actor::new(UserManager::new());
        usermanager
    };
}


pub struct UserManager{
    users:Vec<User>
}

impl UserManager{
    pub fn new()->UserManager{
        UserManager{
            users:Default::default()
        }
    }

    pub fn check_nickname(&self,nickname:String)->bool{
        for user in self.users.iter() {
            if user.nickname==nickname{
                return false
            }
        }
        return true
    }

    pub fn add(&mut self,user:User){
        self.users.push(user)
    }

    pub fn find(&self,sessionid:i64)->Option<User>{
        for user in self.users.iter(){
            if user.sessionid==sessionid{
                return Some(user.clone())
            }
        }
        None
    }

    pub fn remove(&mut self,sessionid:i64)->Option<User>{
        for (index,user) in self.users.iter().enumerate(){
            if user.sessionid==sessionid{
               return Some(self.users.remove(index))
            }
        }
        None
    }

    pub fn get_users(&self)->Vec<User>{
        self.users.clone()
    }
}
#[aqueue::aqueue_trait]
pub trait IUserManager{
    async fn add(&self,user:User)->AResult<()>;
    async fn find(&self,sessionid:i64)->AResult<Option<User>>;
    async fn remove(&self,sessionid:i64)->AResult<Option<User>>;
    async fn get_users(&self)->Vec<User>;
    async fn check_nickname(&self,nickname:String)->AResult<bool>;
}

#[aqueue::aqueue_trait]
impl IUserManager for Actor<UserManager>{
    #[inline]
    async fn add(&self, user: User) -> AResult<()> {
       self.inner_call(async move |inner|{
           inner.get_mut().add(user);
           Ok(())
       }).await
    }
    #[inline]
    async fn find(&self, sessionid: i64) -> AResult<Option<User>> {
        self.inner_call(async move |inner|{
            Ok(inner.get_mut().find(sessionid))
        }).await
    }
    #[inline]
    async fn remove(&self, sessionid: i64) -> AResult<Option<User>> {
        self.inner_call(async move|inner|{
           Ok(inner.get_mut().remove(sessionid))
        }).await
    }
    #[inline]
    async fn get_users(&self) -> Vec<User> {
        unsafe {
            self.deref_inner().get_users()
        }
    }
    #[inline]
    async fn check_nickname(&self, nickname: String) -> AResult<bool> {
        self.inner_call(async move|inner|{
            Ok(inner.get().check_nickname(nickname))
        }).await
    }
}