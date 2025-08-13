use libc::*;

// create a sockaddr_un struct
pub fn create_sockaddr_un() -> sockaddr_un{
    sockaddr_un {
        sun_family: 0,            
        sun_path: [0; 108],     
    }
}
