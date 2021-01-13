
#[derive(Clone)]
pub struct ServerOption{
    pub addr:String,
    pub service_name:String,
    pub verify_key:String
}


impl ServerOption{
    #[inline]
    pub fn new(addr:&str,service_name:&str,verify_key:&str)->ServerOption{
        ServerOption{
            addr:addr.to_string(),
            service_name:service_name.to_string(),
            verify_key:verify_key.to_string()
        }
    }
}